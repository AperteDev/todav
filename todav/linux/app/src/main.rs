//! Todav for Linux: GTK UI + background service (sync worker, ntfy push listener, D-Bus API).

mod add_account;
mod calendar;
mod dates;
mod dbus;
mod goa;
mod i18n;
mod lists;
mod open;
mod reminders;
mod search;
mod settings;
mod update;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use todav_app::accounts::Keyring;
use todav_app::service::{Service, Shell, State};
use todav_core::{Account, AccountKind, ChangeListener, Client};

pub const APP_ID: &str = "dev.aperte.Todav";

static CORE: OnceLock<Arc<Client>> = OnceLock::new();
static SERVICE: OnceLock<Arc<Service>> = OnceLock::new();
/// `State` as D-Bus tells it (see `state_text`).
static SYNC_STATE: Mutex<String> = Mutex::new(String::new());

pub fn core() -> &'static Arc<Client> {
    CORE.get().expect("core opened at startup")
}

/// The background service: sync worker, push listener, reminders.
pub fn service() -> &'static Arc<Service> {
    SERVICE.get().expect("service made at startup")
}

/// After an edit: sync if it left something for the server.
pub fn request_sync() {
    service().request_sync();
}

/// Ask the worker for a full sync.
pub fn sync_now() {
    service().sync_now();
}

/// The main window has something to show: an account (one not connected too: its lists are
/// here, a banner offers to sign in again).
pub fn ready() -> bool {
    service().running() && !core().accounts().is_empty()
}

/// Some account syncs: the sync buttons work.
pub fn can_sync() -> bool {
    service().can_sync()
}

pub fn sync_state() -> String {
    SYNC_STATE.lock().unwrap().clone()
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

/// `State` as the D-Bus `SyncState` property and the window take it: "idle", "syncing" or
/// "error:" with what failed, naming the accounts.
fn state_text(state: &State) -> String {
    let failed = match state {
        State::Idle => return "idle".into(),
        State::Syncing => return "syncing".into(),
        State::Failed(failed) => failed,
    };
    let names: HashMap<i64, String> = core()
        .accounts()
        .iter()
        .map(|a| (a.id, account_name(a)))
        .collect();
    let failed: Vec<String> = failed
        .iter()
        .map(|f| match f.account.and_then(|id| names.get(&id)) {
            Some(name) => format!("{name}: {}", f.error),
            None => f.error.clone(),
        })
        .collect();
    format!("error:{}", failed.join("; "))
}

struct Ui;
impl Shell for Ui {
    fn state(&self, state: State) {
        post(Event::SyncState(state_text(&state)));
    }

    fn refused(&self, items: Vec<todav_core::Refused>) {
        post(Event::Refused(items));
    }
}

struct Listener;
impl ChangeListener for Listener {
    fn changed(&self, list_href: String, _uids: Vec<String>) {
        post(Event::Changed(list_href));
    }

    /// Synced settings (e.g. task order) affect every list's view (the service listens to a
    /// synced ntfy server itself).
    fn settings_changed(&self) {
        for l in core().lists() {
            post(Event::Changed(l.href));
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

/// The Secret Service, for the accounts the core adds and removes.
struct Secrets;
impl Keyring for Secrets {
    fn store(&self, url: &str, user: &str, password: &str) -> Result<(), String> {
        password_store(url, user, password).map_err(|e| e.to_string())
    }

    fn clear(&self, url: &str, user: &str) {
        password_clear(url, user);
    }
}

// --- accounts ----------------------------------------------------------------

/// What an account is called: the name given to it, else "user@host", or "On This Device".
pub fn account_name(a: &Account) -> String {
    if !a.name.is_empty() {
        return a.name.clone();
    }
    match a.kind {
        AccountKind::Local => i18n::gettext("On This Device"),
        AccountKind::Nextcloud => format!("{}@{}", a.user, todav_app::accounts::host(a)),
        AccountKind::Google => a.user.clone(),
        AccountKind::System => i18n::gettext("System Calendars"), // Android's
    }
}

/// Connect a Nextcloud or Google account with its saved password: from GNOME Online Accounts
/// (its `source`) or Todav's keyring entry. False without one (keyring cleared, GOA account
/// gone or with tasks or calendars turned off).
pub fn connect_saved(a: &Account) -> bool {
    if a.kind == AccountKind::Local {
        return false;
    }
    let pw = if a.source.is_empty() {
        password_load(&a.url, &a.user)
    } else if goa::accounts().iter().any(|g| g.id == a.source) {
        goa::password(&a.source, a.kind)
    } else {
        None
    };
    pw.is_some_and(|pw| core().connect(a.id, pw).is_ok())
}

/// Google's access tokens last an hour: each minute the connected accounts take the one GOA
/// has now, renewed when ten minutes or less were left.
fn renew_tokens() {
    std::thread::spawn(|| {
        for a in core().accounts() {
            if a.kind == AccountKind::Google && a.enabled && core().connected(a.id) {
                connect_saved(&a);
            }
        }
    });
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

/// Why an account could not be added, reconnected or removed, in words.
fn account_error(e: todav_app::accounts::Error) -> String {
    use todav_app::accounts::Error;
    match e {
        Error::Exists => i18n::gettext("This account is already added"),
        Error::Gone => i18n::gettext("Connecting failed"),
        Error::SavePassword(e) => {
            i18n::gettext("Cannot save password: {error}").replace("{error}", &e)
        }
        // Edits not on the server would be lost with the local copy.
        Error::Upload(e) => i18n::gettext("Cannot upload pending changes first: {error}")
            .replace("{error}", &e.to_string()),
        Error::NotUploaded => {
            i18n::gettext("It has changes not uploaded yet: sign in again to upload them")
        }
        Error::Core(e) => e.to_string(),
    }
}

/// A Nextcloud or Google account to add.
pub enum Target {
    Goa(goa::Account),
    /// Signed in here (browser or password); the app password goes into Todav's keyring entry.
    Manual(todav_core::Credentials),
}

/// Add a Nextcloud or Google account and connect it (`settings`: settings sync through it),
/// then run the background service with it. A Google account whose calendars Google keeps from
/// Todav is not added.
pub async fn add_account(target: Target, settings: bool) -> Result<i64, String> {
    let id = gio::spawn_blocking(move || -> Result<i64, String> {
        let (kind, url, user, pw, source) = match target {
            Target::Goa(a) => {
                let pw = goa::password(&a.id, a.kind).ok_or_else(|| {
                    i18n::gettext("Online Accounts has no password for this account")
                })?;
                (a.kind, a.url, a.user, pw, a.id)
            }
            Target::Manual(c) => (
                AccountKind::Nextcloud,
                c.server,
                c.login_name,
                c.app_password,
                String::new(),
            ),
        };
        let id =
            todav_app::accounts::add(core(), &Secrets, kind, &url, &user, pw, source, settings)
                .map_err(account_error)?;
        if kind == AccountKind::Google
            && let Err(e) = core().sync_account(id)
            && todav_app::accounts::no_access(&e.to_string())
        {
            let _ = core().remove_account(id);
            return Err(no_calendar_access());
        }
        Ok(id)
    })
    .await
    .map_err(|_| i18n::gettext("Connecting failed"))??;
    started();
    Ok(id)
}

/// What to do when Google gave Todav no access to the calendars.
pub fn no_calendar_access() -> String {
    i18n::gettext(
        "Google gave no access to your calendars. Remove the account in Settings → Online \
         Accounts and add it again, ticking the access to Google Calendar",
    )
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
    let starter = todav_app::lists::Starter {
        tasks: i18n::gettext("Tasks"),
        task: i18n::gettext("Welcome to Todav"),
        note: i18n::gettext(
            "Tick tasks off when done; open one to add a due date, reminders or subtasks.",
        ),
        calendar: i18n::gettext("Calendar"),
    };
    todav_app::lists::start_local(core(), id, starter).map_err(|e| e.to_string())?;
    start_background();
    Ok(id)
}

/// Account `id` (signed in here) at `url` with `password`: a new app password, or the same
/// user on a moved server (then its lists come again from there, after the edits waiting for
/// the old one went up).
pub async fn reconnect(id: i64, url: String, password: String) -> Result<(), String> {
    gio::spawn_blocking(move || {
        todav_app::accounts::reconnect(core(), &Secrets, id, &url, password).map_err(account_error)
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
    gio::spawn_blocking(move || {
        todav_app::accounts::remove(core(), &Secrets, id, force).map_err(account_error)
    })
    .await
    .map_err(|_| i18n::gettext("Removing the account failed"))??;
    service().account_removed(id);
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
    service().account_added(true);
}

// --- background --------------------------------------------------------------

/// Stop the push listener and start a new one if an ntfy server is configured.
pub fn restart_push() {
    service().restart_push();
}

/// Start the sync worker, the push listener and the timers for reminders and syncs. Idempotent.
pub fn start_background() {
    if !service().start(true) {
        return;
    }
    notify_reminders();
    glib::timeout_add_seconds_local(30, || {
        notify_reminders();
        glib::ControlFlow::Continue
    });
    glib::timeout_add_seconds_local(60, || {
        renew_tokens();
        service().tick(window::is_active());
        glib::ControlFlow::Continue
    });
}

/// Notify about reminders that went off since the last check.
fn notify_reminders() {
    let Some(app) = gio::Application::default() else {
        return;
    };
    let now = core().local_now();
    let (tasks, events) = service().due_reminders();
    for t in tasks {
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
    for e in events {
        let n = gio::Notification::new(&e.summary);
        let when = dates::label(e.start, e.all_day, now);
        n.set_body(Some(&match &e.location {
            Some(l) => format!("{when} · {l}"),
            None => when,
        }));
        n.set_default_action_and_target_value("app.show-day", Some(&e.start.to_variant()));
        app.send_notification(Some(&format!("event-{}-{}", e.uid, e.start)), &n);
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
    let client = Arc::new(client);
    let _ = SERVICE.set(Service::new(client.clone(), Box::new(Ui)));
    let _ = CORE.set(client);

    let app: App = glib::Object::builder()
        .property("application-id", APP_ID)
        .property(
            "flags",
            if sandbox() {
                gio::ApplicationFlags::NON_UNIQUE
            } else {
                // A restart after an update starts the new version before this one has quit.
                gio::ApplicationFlags::ALLOW_REPLACEMENT
            } | gio::ApplicationFlags::HANDLES_OPEN,
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
        update::watch(app.upcast_ref());
    });
    app.connect_activate(|app| window::present(app.upcast_ref()));
    app.connect_open(|app, files, _| open::open(app.upcast_ref(), files));
    let code = app.run();
    update::exec_if_restarting();
    code
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
            crate::search::register(conn);
            Ok(())
        }
    }

    impl GtkApplicationImpl for App {}
    impl AdwApplicationImpl for App {}
}
