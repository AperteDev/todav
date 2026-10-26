//! Noticing that Todav was updated while it runs, and restarting into the new version: at once
//! while no window is open (the background service), otherwise when the user picks Restart on the
//! banner. A package replaces the executable (its file is then no longer the running one); in
//! the Flatpak, the portal's update monitor tells when a newer version is installed.
//!
//! Restarting: the process runs the new executable in its own place once the app has quit
//! (`exec_if_restarting`), keeping its systemd unit, which a child would not survive. The Flatpak
//! asks the portal to start the latest version, which takes over the bus name
//! (`--gapplication-replace`) as this one quits.

use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;

/// Seconds without changes to the executable before it counts as installed.
const SETTLE: u32 = 3;

thread_local! {
    static UPDATED: Cell<bool> = const { Cell::new(false) };
    /// The executable to run on quitting, with its arguments.
    static RESTART: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    static MONITOR: RefCell<Option<gio::FileMonitor>> = const { RefCell::new(None) };
    static PENDING: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
    static PORTAL: RefCell<Option<gio::SignalSubscription>> = const { RefCell::new(None) };
}

fn flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// A newer version is installed than the one running.
pub fn updated() -> bool {
    UPDATED.get()
}

/// Watch for an update, from the app's startup.
pub fn watch(app: &adw::Application) {
    if flatpak() {
        return watch_portal(app);
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Ok(monitor) = gio::File::for_path(&exe).monitor_file(
        gio::FileMonitorFlags::WATCH_MOVES,
        None::<&gio::Cancellable>,
    ) else {
        return;
    };
    let app = app.downgrade();
    monitor.connect_changed(move |_, _, _, _| {
        // Installing writes for a while: check once the file has been left alone.
        let (app, exe) = (app.clone(), exe.clone());
        let id = glib::timeout_add_seconds_local_once(SETTLE, move || {
            PENDING.take();
            if let Some(app) = app.upgrade()
                && replaced(&exe)
            {
                found(&app);
            }
        });
        if let Some(old) = PENDING.replace(Some(id)) {
            old.remove();
        }
    });
    MONITOR.set(Some(monitor));
}

/// The file at `exe` is an executable, and not the one running.
fn replaced(exe: &PathBuf) -> bool {
    let (Ok(now), Ok(running)) = (std::fs::metadata(exe), std::fs::metadata("/proc/self/exe"))
    else {
        return false;
    };
    now.is_file()
        && now.mode() & 0o111 != 0
        && (now.dev(), now.ino()) != (running.dev(), running.ino())
}

/// The Flatpak portal's update monitor: an update is installed once the local commit is not the
/// running one (a newer one on the remote only is not).
fn watch_portal(app: &adw::Application) {
    let Some(conn) = app.dbus_connection() else {
        return;
    };
    let args = glib::Variant::tuple_from_iter([glib::VariantDict::new(None).end()]);
    let app = app.downgrade();
    let bus = conn.clone();
    conn.call(
        Some("org.freedesktop.portal.Flatpak"),
        "/org/freedesktop/portal/Flatpak",
        "org.freedesktop.portal.Flatpak",
        "CreateUpdateMonitor",
        Some(&args),
        Some(glib::VariantTy::new("(o)").unwrap()),
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        move |r| {
            let Some(path) = r
                .ok()
                .and_then(|v| v.child_value(0).str().map(String::from))
            else {
                return;
            };
            let subscription = bus.subscribe_to_signal(
                Some("org.freedesktop.portal.Flatpak"),
                Some("org.freedesktop.portal.Flatpak.UpdateMonitor"),
                Some("UpdateAvailable"),
                Some(&path),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    let info = glib::VariantDict::new(Some(&signal.parameters.child_value(0)));
                    let commit = |k: &str| info.lookup::<String>(k).ok().flatten();
                    let (running, local) = (commit("running-commit"), commit("local-commit"));
                    if let (Some(app), true) = (app.upgrade(), local.is_some() && local != running)
                    {
                        found(&app);
                    }
                },
            );
            PORTAL.set(Some(subscription));
        },
    );
}

fn found(app: &adw::Application) {
    if UPDATED.replace(true) {
        return;
    }
    if !window_open(app) {
        return restart(app, true);
    }
    crate::window::update_banners();
}

fn window_open(app: &adw::Application) -> bool {
    app.windows().iter().any(|w| w.is_visible())
}

/// The window was closed: with an update waiting, restart in the background now.
pub fn window_closed(app: &adw::Application) {
    if updated() && !window_open(app) {
        restart(app, true);
    }
}

/// Quit and start the new version: in the `background`, or with its window.
pub fn restart(app: &adw::Application, background: bool) {
    let mut args = vec!["todav-linux".to_string()];
    // The sandboxed copy has no bus name to take over.
    if !crate::sandbox() {
        args.push("--gapplication-replace".into());
    }
    if background {
        args.push("--gapplication-service".into());
    }
    if flatpak() {
        return spawn_latest(app, args);
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // Running, the path reads ".../todav-linux (deleted)" once a package replaced the file.
    let exe = exe.to_string_lossy();
    args[0] = exe.strip_suffix(" (deleted)").unwrap_or(&exe).to_string();
    RESTART.set(Some(args));
    app.quit();
}

/// After the app has quit: run the new version in this process's place, if it is to restart.
pub fn exec_if_restarting() {
    let Some(args) = RESTART.take() else {
        return;
    };
    let err = std::process::Command::new(&args[0]).args(&args[1..]).exec();
    eprintln!("Cannot restart {}: {err}", args[0]);
}

/// The Flatpak portal starts the latest version installed; this one quits once it has.
fn spawn_latest(app: &adw::Application, args: Vec<String>) {
    let Some(conn) = app.dbus_connection() else {
        return;
    };
    let bytes = |s: &str| {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        glib::Variant::array_from_fixed_array(&b)
    };
    let argv = glib::Variant::array_from_iter_with_type(
        glib::VariantTy::BYTE_STRING,
        args.iter().map(|a| bytes(a)),
    );
    let empty = |t| {
        glib::Variant::array_from_iter_with_type(
            glib::VariantTy::new(t).unwrap(),
            [] as [glib::Variant; 0],
        )
    };
    let (fds, envs) = (empty("{uh}"), empty("{ss}"));
    // FLATPAK_SPAWN_FLAGS_LATEST_VERSION
    let flags = 2u32.to_variant();
    let call = glib::Variant::tuple_from_iter([
        bytes("/"),
        argv,
        fds,
        envs,
        flags,
        glib::VariantDict::new(None).end(),
    ]);
    let app = app.downgrade();
    conn.call(
        Some("org.freedesktop.portal.Flatpak"),
        "/org/freedesktop/portal/Flatpak",
        "org.freedesktop.portal.Flatpak",
        "Spawn",
        Some(&call),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        move |r| match r {
            Ok(_) => {
                if let Some(app) = app.upgrade() {
                    app.quit();
                }
            }
            Err(e) => eprintln!("Cannot restart: {e}"),
        },
    );
}
