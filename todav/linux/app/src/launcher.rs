//! The number on the app's icon in a dock, through the Unity Launcher API that Dash to Dock,
//! Plank, KDE's task manager and others read (`launcher_count`). Left alone by default.

use crate::core;
use gtk::gio;
use gtk::prelude::*;
use std::cell::Cell;
use std::collections::HashMap;

thread_local! {
    /// What the dock was last told to show; `None`: nothing of ours.
    // ponytail: told once per change, so a dock that restarts shows nothing until the number changes.
    static SENT: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Tell the dock the number the setting asks for, if it changed since.
pub fn update() {
    let want = todav_app::service::launcher_count(core(), core().local_now());
    if SENT.get() == want {
        return;
    }
    let Some(conn) = gio::Application::default().and_then(|a| a.dbus_connection()) else {
        return;
    };
    let count = want.unwrap_or(0);
    let props = HashMap::from([
        ("count".to_string(), i64::from(count).to_variant()),
        ("count-visible".to_string(), (count > 0).to_variant()),
    ]);
    let args = (format!("application://{}.desktop", crate::APP_ID), props).to_variant();
    let sent = conn.emit_signal(
        None,
        "/com/canonical/unity/launcher",
        "com.canonical.Unity.LauncherEntry",
        "Update",
        Some(&args),
    );
    if sent.is_ok() {
        SENT.set(want);
    }
}
