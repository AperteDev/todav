//! Todav for Linux: GTK UI + background service (sync worker, ntfy push listener, D-Bus API).

mod add_account;
mod calendar;
mod dates;
mod dbus;
mod goa;
mod i18n;
mod lists;
mod reminders;
mod settings;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};
use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use todav_core::{Account, AccountKind, ChangeListener, Client};

pub const APP_ID: &str = "dev.aperte.Todav";

static CORE: OnceLock<Arc<Client>> = OnceLock::new();
static SYNC: OnceLock<Sender<()>> = OnceLock::new();
static SYNC_STATE: Mutex<String> = Mutex::new(String::new());
/// Why each account's last sync failed, by id; gone once one goes through.
static SYNC_ERRORS: Mutex<Option<HashMap<i64, String>>> = Mutex::new(None);
/// The ntfy server the push listener was started for.
static PUSH_URL: Mutex<String> = Mutex::new(String::new());
/// What the push listener last failed with; cleared by its next success.
static PUSH_ERROR: Mutex<Option<String>> = Mutex::new(None);

pub fn core() -> &'static Arc<Client> {
    CORE.get().expect("core opened at startup")
}

/// After an edit: sync if it left something for the server (edits in lists on this device
/// leave nothing, so no sync runs for them).
pub fn request_sync() {
    if core().pending(None) {
        sync_now();
    }
}

/// Ask the worker for a full sync; requests arriving while one runs are coalesced.
pub fn sync_now() {
    if let Some(tx) = SYNC.get() {
        let _ = tx.send(());
    }
}

/// The main window has something to show: an account (one not connected too: its lists are
/// here, a banner offers to sign in again).
pub fn ready() -> bool {
    SYNC.get().is_some() && !core().accounts().is_empty()
}

/// Some account syncs: the sync buttons work.
pub fn can_sync() -> bool {
    core().accounts().iter().any(syncing)
}

/// Why account `id`'s last sync failed.
pub fn sync_error(id: i64) -> Option<String> {
    SYNC_ERRORS.lock().unwrap().as_ref()?.get(&id).cloned()
}

pub fn sync_state() -> String {
    SYNC_STATE.lock().unwrap().clone()
}

pub fn push_error() -> Option<String> {
    PUSH_ERROR.lock().unwrap().clone()
}

/// Events from background threads, handled on the main thread.
#[derive(Debug, Clone)]
pub enum Event {
    Changed(String),
    SyncState(String),
    /// Items the server refused in a sync that otherwise went through.
    Refused(Vec<todav_core::Refused>),
}

fn post(ev: Event) {
    glib::MainContext::default().invoke(move || {
        if let Event::SyncState(s) = &ev {
            *SYNC_STATE.lock().unwrap() = s.clone();
        }
        window::on_event(&ev);
        dbus::on_event(&ev);
        if let Event::SyncState(_) = &ev {
            settings::status_changed();
        }
    });
}

/// What a sync or a push did: items the server refused, the state the extension and the
/// window show (naming the accounts that failed).
fn set_state(r: Result<todav_core::SyncReport, todav_core::Error>) {
    let report = match r {
        Ok(r) => r,
        Err(e) => return post(Event::SyncState(format!("error:{e}"))),
    };
    if !report.refused.is_empty() {
        post(Event::Refused(report.refused));
    }
    let names: HashMap<i64, String> = core()
        .accounts()
        .iter()
        .map(|a| (a.id, account_name(a)))
        .collect();
    let failed: Vec<String> = report
        .failed
        .iter()
        .map(|(id, e)| match names.get(id) {
            Some(name) => format!("{name}: {e}"),
            None => e.to_string(),
        })
        .collect();
    post(Event::SyncState(if failed.is_empty() {
        "idle".into()
    } else {
        format!("error:{}", failed.join("; "))
    }));
}

/// A full sync tried every account that syncs: their errors replace the last ones.
fn set_errors(report: &todav_core::SyncReport) {
    let errors = report
        .failed
        .iter()
        .map(|(id, e)| (*id, e.to_string()))
        .collect();
    *SYNC_ERRORS.lock().unwrap() = Some(errors);
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

// --- accounts ----------------------------------------------------------------

/// What an account is called: the name given to it, else "user@host", or "On This Device".
pub fn account_name(a: &Account) -> String {
    if !a.name.is_empty() {
        return a.name.clone();
    }
    match a.kind {
        AccountKind::Local => i18n::gettext("On This Device"),
        AccountKind::Nextcloud => {
            let host = a.url.split_once("://").map_or(a.url.as_str(), |(_, h)| h);
            format!("{}@{}", a.user, host.trim_end_matches('/'))
        }
    }
}

/// Connect a Nextcloud account with its saved password: from GNOME Online Accounts (its
/// `source`) or Todav's keyring entry. False without one (keyring cleared, GOA account gone or
/// with tasks turned off).
pub fn connect_saved(a: &Account) -> bool {
    if a.kind != AccountKind::Nextcloud {
        return false;
    }
    let pw = if a.source.is_empty() {
        password_load(&a.url, &a.user)
    } else if goa::accounts().iter().any(|g| g.id == a.source) {
        goa::password(&a.source)
    } else {
        None
    };
    pw.is_some_and(|pw| core().connect(a.id, pw).is_ok())
}

/// Connect every account with its saved password; false when there is none (the welcome page
/// shows).
pub fn restore_accounts() -> bool {
    forget_old_sign_in();
    let all = core().accounts();
    for a in &all {
        connect_saved(a);
    }
    !all.is_empty()
}

/// Todav 1.2 remembered a browser sign-in while GNOME Online Accounts was used, to go back to
/// it; its password goes unless an account still uses it.
fn forget_old_sign_in() {
    let get = |k: &str| core().setting(k.into()).filter(|v| !v.is_empty());
    let (Some(url), Some(user)) = (get("manual_url"), get("manual_user")) else {
        return;
    };
    let used = core()
        .accounts()
        .iter()
        .any(|a| a.source.is_empty() && a.url == url && a.user == user);
    if !used {
        password_clear(&url, &user);
    }
    for k in ["manual_url", "manual_user"] {
        let _ = core().set_setting(k.into(), String::new());
    }
}

/// Switched on and connected: an account that syncs.
fn syncing(a: &Account) -> bool {
    a.enabled && core().connected(a.id)
}

/// Why an account cannot go yet: edits not on the server would be lost with the local copy.
fn pending_error(e: &todav_core::Error) -> String {
    i18n::gettext("Cannot upload pending changes first: {error}").replace("{error}", &e.to_string())
}

/// A Nextcloud account to add.
pub enum Target {
    Goa(goa::Account),
    /// Signed in here (browser or password); the app password goes into Todav's keyring entry.
    Manual(todav_core::Credentials),
}

/// Add a Nextcloud account and connect it (`settings`: settings sync through it), then run
/// the background service with it.
pub async fn add_account(target: Target, settings: bool) -> Result<i64, String> {
    let id = gio::spawn_blocking(move || -> Result<i64, String> {
        let (url, user, pw, source) = match target {
            Target::Goa(a) => {
                let pw = goa::password(&a.id).ok_or_else(|| {
                    i18n::gettext("Online Accounts has no password for this account")
                })?;
                (a.url, a.user, pw, a.id)
            }
            Target::Manual(c) => (c.server, c.login_name, c.app_password, String::new()),
        };
        let url = url.trim_end_matches('/').to_string();
        let kind = AccountKind::Nextcloud;
        let id = core()
            .add_account(kind, url.clone(), user.clone(), source.clone())
            .map_err(|e| match e {
                todav_core::Error::Exists => i18n::gettext("This account is already added"),
                e => e.to_string(),
            })?;
        if source.is_empty()
            && let Err(e) = password_store(&url, &user, &pw)
        {
            let _ = core().remove_account(id);
            return Err(
                i18n::gettext("Cannot save password: {error}").replace("{error}", &e.to_string())
            );
        }
        core().connect(id, pw).map_err(|e| e.to_string())?;
        if settings {
            let _ = core().set_settings_account(Some(id));
        }
        Ok(id)
    })
    .await
    .map_err(|_| i18n::gettext("Connecting failed"))??;
    started();
    Ok(id)
}

/// The local account, the first time with a task list holding a task to start from and an
/// empty calendar.
pub fn add_local() -> Result<i64, String> {
    let id = core()
        .add_account(
            AccountKind::Local,
            String::new(),
            String::new(),
            String::new(),
        )
        .map_err(|e| e.to_string())?;
    lists::start_local(id).map_err(|e| e.to_string())?;
    start_background();
    Ok(id)
}

/// Account `id` (signed in here) at `url` with `password`: a new app password, or the same
/// user on a moved server (then its lists come again from there, after the edits waiting for
/// the old one went up).
pub async fn reconnect(id: i64, url: String, password: String) -> Result<(), String> {
    gio::spawn_blocking(move || -> Result<(), String> {
        let a = core()
            .account(id)
            .ok_or_else(|| i18n::gettext("Connecting failed"))?;
        let url = url.trim_end_matches('/').to_string();
        password_store(&url, &a.user, &password).map_err(|e| {
            i18n::gettext("Cannot save password: {error}").replace("{error}", &e.to_string())
        })?;
        if url == a.url {
            return core().connect(id, password).map_err(|e| e.to_string());
        }
        if core().connected(id) || core().pending(Some(id)) {
            core().sync_account(id).map_err(|e| pending_error(&e))?;
        }
        core()
            .move_account(id, url, password)
            .map_err(|e| e.to_string())?;
        password_clear(&a.url, &a.user);
        Ok(())
    })
    .await
    .map_err(|_| i18n::gettext("Connecting failed"))??;
    started();
    Ok(())
}

/// Switch account `id` on or off (off hides its lists and stops syncing them).
pub async fn enable_account(id: i64, on: bool) {
    let _ = gio::spawn_blocking(move || core().set_account_enabled(id, on)).await;
    window::account_changed();
    calendar::refresh();
    if on {
        started();
    }
}

/// Remove account `id` from this device, with its saved password; edits waiting for its server
/// go up first, unless `force` (then they are lost).
pub async fn remove_account(id: i64, force: bool) -> Result<(), String> {
    gio::spawn_blocking(move || -> Result<(), String> {
        let Some(a) = core().account(id) else {
            return Ok(());
        };
        if a.kind == AccountKind::Nextcloud && !force {
            if core().connected(id) {
                core().sync_account(id).map_err(|e| pending_error(&e))?;
            } else if core().pending(Some(id)) {
                return Err(i18n::gettext(
                    "It has changes not uploaded yet: sign in again to upload them",
                ));
            }
        }
        if a.kind == AccountKind::Nextcloud && a.source.is_empty() {
            password_clear(&a.url, &a.user);
        }
        core().remove_account(id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| i18n::gettext("Removing the account failed"))??;
    if let Some(errors) = SYNC_ERRORS.lock().unwrap().as_mut() {
        errors.remove(&id);
    }
    if !can_sync() {
        post(Event::SyncState("idle".into())); // no sync left to fail
    } else {
        sync_now();
    }
    if core().accounts().is_empty() {
        window::show_welcome();
    } else {
        window::account_changed();
        calendar::refresh();
    }
    Ok(())
}

/// An account came, connected or was switched on: sync it, listen for its pushes.
fn started() {
    let running = SYNC.get().is_some();
    start_background();
    if running {
        restart_push(); // registers push for the account's lists now, not within the hour
    }
}

// --- background --------------------------------------------------------------

/// Stop the push listener and start a new one if an ntfy server is configured.
pub fn restart_push() {
    core().stop_listening();
    let url = core().setting("ntfy_url".into()).unwrap_or_default();
    *PUSH_URL.lock().unwrap() = url.clone();
    *PUSH_ERROR.lock().unwrap() = None;
    if let Some(ntfy) = Some(url).filter(|u| !u.is_empty() && can_sync()) {
        std::thread::spawn(move || {
            // Blocks until the next restart_push; reconnects internally.
            let _ = core().listen(ntfy, &|r| {
                *PUSH_ERROR.lock().unwrap() = r.as_ref().err().map(|e| e.to_string());
                set_state(r);
            });
        });
    }
}

/// Start the sync worker and, if an ntfy server is configured, the push listener. Idempotent.
pub fn start_background() {
    if SYNC.get().is_some() {
        sync_now();
        return;
    }
    let (tx, rx) = channel::<()>();
    let _ = SYNC.set(tx);
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            std::thread::sleep(Duration::from_millis(300)); // let a burst of edits settle
            while rx.try_recv().is_ok() {}
            if !can_sync() {
                continue; // no account, or no password for one: nothing to sync until one connects
            }
            post(Event::SyncState("syncing".into()));
            let report = core().sync();
            set_errors(&report);
            set_state(Ok(report));
        }
    });
    sync_now();

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
            sync_now();
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
            n.set_body(Some(
                &i18n::gettext("Due {date}")
                    .replace("{date}", &dates::label(d, t.due_all_day, now)),
            ));
        }
        n.set_default_action_and_target_value("app.show-list", Some(&t.list_href.to_variant()));
        n.add_button_with_target_value(
            &i18n::gettext("Mark as Done"),
            "app.complete",
            Some(&t.uid.to_variant()),
        );
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
    opts.insert(
        "reason",
        i18n::gettext("Sync tasks and receive changes in the background"),
    );
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
    i18n::init();
    gio::resources_register_include!("icons.gresource").expect("register icons");
    let data = glib::user_data_dir().join("todav");
    let client = Client::open(data.to_string_lossy().into()).expect("open local database");
    client.subscribe(Box::new(Listener));
    let _ = CORE.set(Arc::new(client));

    let app: App = glib::Object::builder()
        .property("application-id", APP_ID)
        .property(
            "flags",
            if sandbox() {
                gio::ApplicationFlags::NON_UNIQUE
            } else {
                gio::ApplicationFlags::empty()
            },
        )
        .build();
    app.connect_startup(|app| {
        // Keep running as a background service after the window closes; quit is explicit.
        std::mem::forget(app.hold());
        if let Some(conn) = app.dbus_connection()
            && std::path::Path::new("/.flatpak-info").exists()
        {
            request_autostart(&conn);
        }
        window::setup_actions(app.upcast_ref());
        settings::apply_color_scheme();
        if restore_accounts() {
            start_background();
        }
    });
    app.connect_activate(|app| window::present(app.upcast_ref()));
    app.run()
}

glib::wrapper! {
    /// The application, exporting the D-Bus API before it takes its bus name.
    pub struct App(ObjectSubclass<imp::App>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

mod imp {
    use adw::subclass::prelude::*;
    use gtk::{gio, glib};

    #[derive(Default)]
    pub struct App;

    #[glib::object_subclass]
    impl ObjectSubclass for App {
        const NAME: &'static str = "TodavApp";
        type Type = super::App;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for App {}

    impl ApplicationImpl for App {
        // Before the bus name is ours: a call that starts the app (D-Bus activation, from the
        // extension) waits for the name, and would otherwise find no interface yet.
        fn dbus_register(&self, conn: &gio::DBusConnection, path: &str) -> Result<(), glib::Error> {
            self.parent_dbus_register(conn, path)?;
            crate::dbus::register(conn, path);
            Ok(())
        }
    }

    impl GtkApplicationImpl for App {}
    impl AdwApplicationImpl for App {}
}
