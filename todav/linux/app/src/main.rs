//! Todav for Linux: GTK UI + background service (sync worker, ntfy push listener, D-Bus API).

mod calendar;
mod connection;
mod dates;
mod dbus;
mod goa;
mod reminders;
mod settings;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};
use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use todav_core::{ChangeListener, Client};

pub const APP_ID: &str = "io.github.selfref.Todav";

static CORE: OnceLock<Arc<Client>> = OnceLock::new();
static SYNC: OnceLock<Sender<()>> = OnceLock::new();
static SYNC_STATE: Mutex<String> = Mutex::new(String::new());
/// The ntfy server the push listener was started for.
static PUSH_URL: Mutex<String> = Mutex::new(String::new());

pub fn core() -> &'static Arc<Client> {
    CORE.get().expect("core opened at startup")
}

/// Ask the worker for a full sync; requests arriving while one runs are coalesced.
pub fn request_sync() {
    if let Some(tx) = SYNC.get() {
        let _ = tx.send(());
    }
}

/// True while an account is active and the background service runs.
pub fn logged_in() -> bool {
    SYNC.get().is_some() && core().account().is_some()
}

pub fn sync_state() -> String {
    SYNC_STATE.lock().unwrap().clone()
}

/// Events from background threads, handled on the main thread.
#[derive(Debug, Clone)]
pub enum Event {
    Changed(String),
    SyncState(String),
}

fn post(ev: Event) {
    glib::MainContext::default().invoke(move || {
        if let Event::SyncState(s) = &ev {
            *SYNC_STATE.lock().unwrap() = s.clone();
        }
        window::on_event(&ev);
        dbus::on_event(&ev);
    });
}

fn set_state(r: &Result<impl std::fmt::Debug, todav_core::Error>) {
    post(Event::SyncState(match r {
        Ok(_) => "idle".into(),
        Err(e) => format!("error:{e}"),
    }));
}

struct Listener;
impl ChangeListener for Listener {
    fn changed(&self, list_href: String, _uids: Vec<String>) {
        post(Event::Changed(list_href));
    }

    /// Synced settings (e.g. task order) affect every list's view; a synced ntfy server is
    /// listened to at once.
    fn settings_changed(&self) {
        for l in core().lists() {
            post(Event::Changed(l.href));
        }
        let ntfy = core().setting("ntfy_url".into()).unwrap_or_default();
        if ntfy != *PUSH_URL.lock().unwrap() {
            restart_push();
        }
    }
}

// --- keyring -----------------------------------------------------------------

/// `TODAV_SANDBOX` set: a separate copy for testing other connections (`make -C todav/linux
/// sandbox`). It runs next to the normal app, keeps its data where XDG_DATA_HOME says and its
/// passwords apart, so logging out there leaves the normal app signed in.
pub fn sandbox() -> bool {
    std::env::var_os("TODAV_SANDBOX").is_some()
}

fn schema() -> libsecret::Schema {
    let attrs = HashMap::from([
        ("url", libsecret::SchemaAttributeType::String),
        ("user", libsecret::SchemaAttributeType::String),
    ]);
    let name = if sandbox() {
        format!("{APP_ID}.Sandbox")
    } else {
        APP_ID.into()
    };
    libsecret::Schema::new(&name, libsecret::SchemaFlags::NONE, attrs)
}

pub fn password_load(url: &str, user: &str) -> Option<String> {
    let attrs = HashMap::from([("url", url), ("user", user)]);
    libsecret::password_lookup_sync(Some(&schema()), attrs, gio::Cancellable::NONE)
        .ok()?
        .map(|s| s.to_string())
}

pub fn password_store(url: &str, user: &str, password: &str) -> Result<(), glib::Error> {
    let attrs = HashMap::from([("url", url), ("user", user)]);
    let label = format!("Todav ({user} @ {url})");
    libsecret::password_store_sync(
        Some(&schema()),
        attrs,
        Some(libsecret::COLLECTION_DEFAULT),
        &label,
        password,
        gio::Cancellable::NONE,
    )
}

pub fn password_clear(url: &str, user: &str) {
    let attrs = HashMap::from([("url", url), ("user", user)]);
    let _ = libsecret::password_clear_sync(Some(&schema()), attrs, gio::Cancellable::NONE);
}

// --- account -----------------------------------------------------------------

/// The GNOME Online Accounts account Todav connects through; None means signed in here.
pub fn goa_id() -> Option<String> {
    core()
        .setting("goa_account".into())
        .filter(|id| !id.is_empty())
}

/// Restore the account, its password from GOA or the keyring; false → the UI shows the
/// welcome page (also when the GOA account is gone or has tasks turned off).
pub fn restore_account() -> bool {
    let Some(acc) = core().account() else {
        return false;
    };
    let (url, user, pw) = match goa_id() {
        Some(id) => match goa::accounts().into_iter().find(|a| a.id == id) {
            Some(a) => (a.url, a.user, goa::password(&id)),
            None => return false,
        },
        None => (
            acc.url.clone(),
            acc.user.clone(),
            password_load(&acc.url, &acc.user),
        ),
    };
    let Some(pw) = pw else {
        return false;
    };
    core().set_account(url, user, pw).is_ok()
}

/// (server, user) of the last browser sign-in.
fn remembered() -> Option<(String, String)> {
    let get = |k: &str| core().setting(k.into()).filter(|v| !v.is_empty());
    Some((get("manual_url")?, get("manual_user")?))
}

/// The last browser sign-in, while its password is still saved: where turning GOA off goes.
pub fn remembered_sign_in() -> Option<todav_core::Credentials> {
    let (server, login_name) = remembered()?;
    let app_password = password_load(&server, &login_name)?;
    Some(todav_core::Credentials {
        server,
        login_name,
        app_password,
    })
}

/// Forget the browser sign-in (Sign Out): its saved password goes too.
pub fn forget_sign_in() {
    if let Some((u, n)) = remembered() {
        password_clear(&u, &n);
    }
    let _ = core().set_setting("manual_url".into(), String::new());
    let _ = core().set_setting("manual_user".into(), String::new());
}

/// Stop using the current account and drop the local copy; the window shows the welcome page.
/// Pending edits are uploaded first, unless `force` (then they are lost when that fails).
pub async fn disconnect(force: bool) -> Result<(), String> {
    gio::spawn_blocking(move || -> Result<(), String> {
        if core().account().is_some()
            && let Err(e) = core().sync()
            && !force
        {
            return Err(format!("Cannot upload pending changes first: {e}"));
        }
        core().stop_listening();
        let _ = core().set_setting("goa_account".into(), String::new());
        core().logout().map_err(|e| e.to_string()) // also stops server pushes
    })
    .await
    .map_err(|_| "Disconnecting failed".to_string())??;
    window::show_welcome();
    Ok(())
}

/// An account to connect with.
pub enum Target {
    Goa(goa::Account),
    /// Signed in here (browser login); the app password goes into Todav's keyring entry.
    Manual(todav_core::Credentials),
}

/// Connect with `target`, replacing the current account (its pending edits are uploaded
/// first), then run the background service with it.
pub async fn switch_account(target: Target) -> Result<(), String> {
    gio::spawn_blocking(move || -> Result<(), String> {
        let (url, user, pw, goa) = match target {
            Target::Goa(a) => {
                let pw = goa::password(&a.id)
                    .ok_or("Online Accounts has no password for this account")?;
                (a.url, a.user, pw, a.id)
            }
            Target::Manual(c) => (c.server, c.login_name, c.app_password, String::new()),
        };
        if let Some(old) = core().account().filter(|_| logged_in())
            && (old.url.as_str(), old.user.as_str()) != (url.as_str(), user.as_str())
        {
            // A different account drops the local copy: pending edits must reach the old one.
            core()
                .sync()
                .map_err(|e| format!("Cannot upload pending changes first: {e}"))?;
            let _ = core().push_unregister();
        }
        // The browser sign-in is remembered while GOA is used, to go back to it.
        if goa.is_empty() {
            if let Some((u, n)) = remembered().filter(|r| *r != (url.clone(), user.clone())) {
                password_clear(&u, &n);
            }
            password_store(&url, &user, &pw).map_err(|e| format!("Cannot save password: {e}"))?;
            let _ = core().set_setting("manual_url".into(), url.clone());
            let _ = core().set_setting("manual_user".into(), user.clone());
        }
        core()
            .set_account(url, user, pw)
            .map_err(|e| e.to_string())?;
        core()
            .set_setting("goa_account".into(), goa)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Connecting failed".to_string())??;
    let running = logged_in();
    start_background();
    if running {
        restart_push(); // registers push for the new account's lists
    }
    Ok(())
}

// --- background --------------------------------------------------------------

/// Stop the push listener and start a new one if an ntfy server is configured.
pub fn restart_push() {
    core().stop_listening();
    let url = core().setting("ntfy_url".into()).unwrap_or_default();
    *PUSH_URL.lock().unwrap() = url.clone();
    if let Some(ntfy) = Some(url).filter(|u| !u.is_empty()) {
        std::thread::spawn(move || {
            // Blocks until the next restart_push; reconnects internally.
            let _ = core().listen(ntfy, &|r| set_state(&r));
        });
    }
}

/// Start the sync worker and, if an ntfy server is configured, the push listener. Idempotent.
pub fn start_background() {
    if SYNC.get().is_some() {
        request_sync();
        return;
    }
    let (tx, rx) = channel::<()>();
    let _ = SYNC.set(tx);
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            std::thread::sleep(Duration::from_millis(300)); // let a burst of edits settle
            while rx.try_recv().is_ok() {}
            if core().account().is_none() {
                continue; // disconnected: nothing to sync until the next account
            }
            post(Event::SyncState("syncing".into()));
            set_state(&core().sync());
        }
    });
    request_sync();

    restart_push();

    notify_reminders();
    glib::timeout_add_seconds_local(30, || {
        notify_reminders();
        glib::ControlFlow::Continue
    });

    // Safety net for lost pushes: every minute while the window is focused, every 30 min otherwise.
    let mut ticks = 0u32;
    glib::timeout_add_seconds_local(60, move || {
        ticks += 1;
        if window::is_active() || ticks.is_multiple_of(30) {
            request_sync();
        }
        glib::ControlFlow::Continue
    });
}

/// Notify about reminders that went off since the last check: at most a day back, so a device
/// that was off does not replay old ones.
fn notify_reminders() {
    let Some(app) = gio::Application::default() else {
        return;
    };
    let now = core().local_now();
    let from = core()
        .setting("reminded_until".into())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(now)
        .max(now - 86400);
    for t in core().reminders(from, now) {
        let n = gio::Notification::new(&t.summary);
        if let Some(d) = t.due {
            n.set_body(Some(&format!(
                "Due {}",
                dates::label(d, t.due_all_day, now)
            )));
        }
        n.set_default_action_and_target_value("app.show-list", Some(&t.list_href.to_variant()));
        n.add_button_with_target_value("Mark as Done", "app.complete", Some(&t.uid.to_variant()));
        app.send_notification(Some(&format!("reminder-{}", t.uid)), &n);
    }
    for e in core().event_reminders(from, now) {
        let n = gio::Notification::new(&e.summary);
        let when = dates::label(e.start, e.all_day, now);
        n.set_body(Some(&match &e.location {
            Some(l) => format!("{when} · {l}"),
            None => when,
        }));
        n.set_default_action_and_target_value("app.show-day", Some(&e.start.to_variant()));
        app.send_notification(Some(&format!("event-{}-{}", e.uid, e.start)), &n);
    }
    if from != now {
        let _ = core().set_setting("reminded_until".into(), now.to_string());
    }
}

/// The Flatpak cannot reach the host's autostart folder: ask the Background portal to start the
/// service at login instead (GNOME may ask the user once).
fn request_autostart(conn: &gio::DBusConnection) {
    let opts = glib::VariantDict::new(None);
    opts.insert("reason", "Sync tasks and receive changes in the background");
    opts.insert("autostart", true);
    opts.insert("commandline", vec!["todav-linux", "--gapplication-service"]);
    let args = glib::Variant::tuple_from_iter(["".to_variant(), opts.end()]);
    conn.call(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Background",
        "RequestBackground",
        Some(&args),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        |r| {
            if let Err(e) = r {
                eprintln!("Background portal: {e}");
            }
        },
    );
}

fn main() -> glib::ExitCode {
    gio::resources_register_include!("icons.gresource").expect("register icons");
    let data = glib::user_data_dir().join("todav");
    let client = Client::open(data.to_string_lossy().into()).expect("open local database");
    client.subscribe(Box::new(Listener));
    let _ = CORE.set(Arc::new(client));

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(if sandbox() {
            gio::ApplicationFlags::NON_UNIQUE
        } else {
            gio::ApplicationFlags::empty()
        })
        .build();
    app.connect_startup(|app| {
        // Keep running as a background service after the window closes; quit is explicit.
        std::mem::forget(app.hold());
        // First: the bus name is already ours, so calls arriving before this would fail.
        if let (Some(conn), Some(path)) = (app.dbus_connection(), app.dbus_object_path()) {
            dbus::register(&conn, &path);
            if std::path::Path::new("/.flatpak-info").exists() {
                request_autostart(&conn);
            }
        }
        window::setup_actions(app);
        if restore_account() {
            start_background();
        }
    });
    app.connect_activate(window::present);
    app.run()
}
