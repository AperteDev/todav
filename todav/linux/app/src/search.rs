//! Search in GNOME Shell's Activities overview (`org.gnome.Shell.SearchProvider2`, declared by
//! `data/dev.aperte.Todav.search-provider.ini`): open tasks and events whose titles hold every
//! word typed. A result's id is its `todav://` link, which opening it follows.

use crate::{core, dates, window::folded};
use glib::prelude::*;
use gtk::{gio, glib};
use std::collections::HashMap;
use todav_app::links::{self, Link};

pub const PATH: &str = "/dev/aperte/Todav/SearchProvider";
const IFACE: &str = "org.gnome.Shell.SearchProvider2";

const XML: &str = r#"<node>
<interface name="org.gnome.Shell.SearchProvider2">
  <method name="GetInitialResultSet"><arg direction="in" name="terms" type="as"/><arg direction="out" type="as"/></method>
  <method name="GetSubsearchResultSet"><arg direction="in" name="previous" type="as"/><arg direction="in" name="terms" type="as"/><arg direction="out" type="as"/></method>
  <method name="GetResultMetas"><arg direction="in" name="ids" type="as"/><arg direction="out" type="aa{sv}"/></method>
  <method name="ActivateResult"><arg direction="in" name="id" type="s"/><arg direction="in" name="terms" type="as"/><arg direction="in" name="timestamp" type="u"/></method>
  <method name="LaunchSearch"><arg direction="in" name="terms" type="as"/><arg direction="in" name="timestamp" type="u"/></method>
</interface>
</node>"#;

/// The most results the overview is given; it shows a few.
const MAX: usize = 20;

pub fn register(conn: &gio::DBusConnection) {
    let info = gio::DBusNodeInfo::for_xml(XML).expect("valid introspection xml");
    let iface = info.lookup_interface(IFACE).expect("interface in xml");
    let res = conn
        .register_object(PATH, &iface)
        .method_call(|_, _, _, _, method, params, inv| {
            inv.return_value(call(method, &params).as_ref())
        })
        .build();
    if let Err(e) = res {
        eprintln!("Search provider registration failed: {e}");
    }
}

fn call(method: &str, params: &glib::Variant) -> Option<glib::Variant> {
    let terms = |i: usize| -> Vec<String> { params.child_value(i).get().unwrap_or_default() };
    match method {
        "GetInitialResultSet" => Some((results(&terms(0)),).to_variant()),
        "GetSubsearchResultSet" => Some((results(&terms(1)),).to_variant()),
        "GetResultMetas" => {
            let metas: Vec<HashMap<String, glib::Variant>> =
                terms(0).iter().filter_map(|id| meta(id)).collect();
            Some((metas,).to_variant())
        }
        "ActivateResult" => {
            let id: String = params.child_value(0).get().unwrap_or_default();
            crate::open::show(&id);
            None
        }
        "LaunchSearch" => {
            crate::window::search(&terms(0).join(" "));
            None
        }
        _ => None,
    }
}

/// Links to the open tasks, then the events, whose titles hold every term.
fn results(terms: &[String]) -> Vec<String> {
    let terms: Vec<String> = terms
        .iter()
        .map(|t| folded(t))
        .filter(|t| !t.is_empty())
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let hit = |text: &str| {
        let text = folded(text);
        terms.iter().all(|t| text.contains(t.as_str()))
    };
    let c = core();
    let hidden = todav_app::prefs::hidden(c);
    let mut found: Vec<String> = c
        .lists()
        .into_iter()
        .filter(|l| !hidden.contains(&l.href))
        .flat_map(|l| c.tasks(l.href, false))
        .filter(|t| !t.done && hit(&t.summary))
        .map(|t| links::task(&t.uid))
        .collect();
    found.truncate(MAX);
    let events = todav_app::calendar::search(c, c.local_now(), |e| hit(&e.summary));
    found.extend(events.iter().map(|e| links::event(&e.uid)));
    found.truncate(MAX);
    found
}

/// What the overview shows of result `id`: its title, where it is (a task's list, an event's
/// time and calendar) and an icon.
fn meta(id: &str) -> Option<HashMap<String, glib::Variant>> {
    let c = core();
    let list_name = |href: &str| {
        c.lists()
            .into_iter()
            .chain(c.calendars())
            .find(|l| l.href == href)
            .map(|l| l.display_name)
            .unwrap_or_default()
    };
    let (name, description, icon) = match links::parse(id)? {
        Link::Task(uid) => {
            let t = c.task(uid)?;
            (t.summary, list_name(&t.list_href), "checkbox-symbolic")
        }
        Link::Event(uid) => {
            let e = c.event(uid)?;
            let when = dates::label(e.start, e.all_day, c.local_now());
            let place = list_name(&e.list_href);
            (
                e.summary,
                format!("{when} · {place}"),
                "x-office-calendar-symbolic",
            )
        }
    };
    Some(HashMap::from([
        ("id".to_string(), id.to_variant()),
        ("name".to_string(), name.to_variant()),
        ("description".to_string(), description.to_variant()),
        ("gicon".to_string(), icon.to_variant()),
    ]))
}
