//! Session-bus API for the GNOME Shell extension, exported on the GApplication's own connection.

use crate::{Event, core, request_sync, sync_state, window};
use glib::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::collections::HashMap;

pub const IFACE: &str = "dev.aperte.Todav1";

const XML: &str = r#"<node>
<interface name="dev.aperte.Todav1">
  <method name="GetLists"><arg direction="out" type="a(ssus)"/></method>
  <method name="GetTasks"><arg direction="in" name="list" type="s"/><arg direction="out" type="a(sssbu)"/></method>
  <method name="AddTask"><arg direction="in" name="list" type="s"/><arg direction="in" name="summary" type="s"/><arg direction="out" type="s"/></method>
  <method name="ShowList"><arg direction="in" name="list" type="s"/></method>
  <method name="ShowTask"><arg direction="in" name="uid" type="s"/></method>
  <method name="GetDays"><arg direction="in" name="from" type="x"/><arg direction="in" name="days" type="u"/><arg direction="out" type="aas"/></method>
  <method name="GetAgenda"><arg direction="in" name="days" type="u"/><arg direction="out" type="a(xsssbb)"/></method>
  <method name="ShowDay"><arg direction="in" name="at" type="x"/></method>
  <method name="NewEvent"><arg direction="in" name="summary" type="s"/></method>
  <method name="GetSettings"><arg direction="out" type="a{ss}"/></method>
  <method name="SetSetting"><arg direction="in" name="key" type="s"/><arg direction="in" name="value" type="s"/></method>
  <method name="Sync"/>
  <signal name="Changed"><arg name="list" type="s"/></signal>
  <property name="SyncState" type="s" access="read"/>
</interface>
</node>"#;

thread_local! {
    static BUS: RefCell<Option<(gio::DBusConnection, String)>> = const { RefCell::new(None) };
}

pub fn register(conn: &gio::DBusConnection, path: &str) {
    let info = gio::DBusNodeInfo::for_xml(XML).expect("valid introspection xml");
    let iface = info.lookup_interface(IFACE).expect("interface in xml");
    let res = conn
        .register_object(path, &iface)
        .method_call(
            |_, _, _, _, method, params, inv| match call(method, &params) {
                Ok(v) => inv.return_value(v.as_ref()),
                Err(e) => inv.return_dbus_error(&format!("{IFACE}.Error"), &e),
            },
        )
        .property(|_, _, _, _, _| sync_state().to_variant())
        .build();
    match res {
        Ok(_) => BUS.with(|b| *b.borrow_mut() = Some((conn.clone(), path.to_string()))),
        Err(e) => eprintln!("D-Bus registration failed: {e}"),
    }
}

fn call(method: &str, params: &glib::Variant) -> Result<Option<glib::Variant>, String> {
    let c = core();
    let bad = || format!("bad arguments for {method}");
    Ok(match method {
        // (href, name, open tasks, colour as the app shows it)
        "GetLists" => {
            // In the sidebar's order: by account.
            let lists: Vec<(String, String, u32, String)> = crate::lists::by_place(c.lists())
                .into_iter()
                .map(|l| {
                    let color = crate::lists::color(&l);
                    (l.href, l.display_name, l.open_count, color)
                })
                .collect();
            Some((lists,).to_variant())
        }
        // (uid, summary, category, done, depth: 0 for a task, 1 for its subtask, ...), in the
        // app's order.
        "GetTasks" => {
            let (list,) = params.get::<(String,)>().ok_or_else(bad)?;
            let mut out: Vec<(String, String, String, bool, u32)> = Vec::new();
            let order = crate::settings::order();
            for g in c.grouped_by(
                list,
                order.tasks,
                order.subtasks,
                crate::settings::done_view(),
            ) {
                let cat = g.name.unwrap_or_default();
                // Parents come before their subtasks.
                let mut depths: HashMap<String, u32> = HashMap::new();
                for t in g.tasks {
                    let depth = t
                        .parent_uid
                        .as_ref()
                        .and_then(|p| depths.get(p))
                        .map_or(0, |d| d + 1);
                    depths.insert(t.uid.clone(), depth);
                    out.push((t.uid, t.summary, cat.clone(), t.done, depth));
                }
            }
            Some((out,).to_variant())
        }
        "AddTask" => {
            let (list, summary) = params.get::<(String, String)>().ok_or_else(bad)?;
            let t = c
                .add_task(
                    list,
                    summary,
                    None,
                    None,
                    crate::settings::order().task_start,
                )
                .map_err(|e| e.to_string())?;
            request_sync();
            Some((t.uid,).to_variant())
        }
        "ShowList" => {
            let (list,) = params.get::<(String,)>().ok_or_else(bad)?;
            window::show_list(&list);
            None
        }
        "ShowTask" => {
            let (uid,) = params.get::<(String,)>().ok_or_else(bad)?;
            window::show_task(&uid);
            None
        }
        "NewEvent" => {
            let (summary,) = params.get::<(String,)>().ok_or_else(bad)?;
            if let Some(app) = gio::Application::default().and_downcast::<adw::Application>() {
                window::present(&app);
            }
            crate::calendar::new_event_titled(&summary);
            None
        }
        // The extension's options, see settings::EXTENSION; it may only change those.
        "GetSettings" => {
            let mut map: HashMap<String, String> =
                crate::settings::extension().into_iter().collect();
            // Todav 1.2 kept the server's path: the list at that path.
            if let Some(list) = map.get_mut("ext_list").filter(|l| l.starts_with('/')) {
                let found = c
                    .lists()
                    .into_iter()
                    .find(|l| todav_core::same_list(list, &l.href));
                if let Some(l) = found {
                    *list = l.href;
                }
            }
            Some((map,).to_variant())
        }
        "SetSetting" => {
            let (key, value) = params.get::<(String, String)>().ok_or_else(bad)?;
            if !crate::settings::EXTENSION.iter().any(|(k, _)| *k == key) {
                return Err(format!("not an extension setting: {key}"));
            }
            c.set_setting(key, value).map_err(|e| e.to_string())?;
            None
        }
        // For each of `days` days from `from` (local midnight), the colours of the calendars
        // with something on it: the month's dots.
        "GetDays" => {
            let (from, days) = params.get::<(i64, u32)>().ok_or_else(bad)?;
            let days = crate::calendar::day_colors(from, i64::from(days.min(42)));
            Some((days,).to_variant())
        }
        // (day as local midnight for ShowDay, time, title, colour, is a task, done) for each
        // thing on the `days` days from today, in the calendars shown.
        "GetAgenda" => {
            let (days,) = params.get::<(u32,)>().ok_or_else(bad)?;
            let items = crate::calendar::agenda(i64::from(days.min(31)));
            Some((items,).to_variant())
        }
        "ShowDay" => {
            let (at,) = params.get::<(i64,)>().ok_or_else(bad)?;
            if let Some(app) = gio::Application::default().and_downcast::<adw::Application>() {
                window::present(&app);
            }
            crate::calendar::show_day(at);
            None
        }
        "Sync" => {
            crate::sync_now();
            None
        }
        m => return Err(format!("unknown method {m}")),
    })
}

pub fn on_event(ev: &Event) {
    BUS.with(|b| {
        let Some((conn, path)) = &*b.borrow() else {
            return;
        };
        let _ = match ev {
            Event::Changed(list) => {
                conn.emit_signal(None, path, IFACE, "Changed", Some(&(list,).to_variant()))
            }
            Event::SyncState(s) => {
                let changed = HashMap::from([("SyncState".to_string(), s.to_variant())]);
                let args = (IFACE, changed, Vec::<String>::new()).to_variant();
                conn.emit_signal(
                    None,
                    path,
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    Some(&args),
                )
            }
            Event::Refused(_) => return, // the window tells the user
        };
    });
}
