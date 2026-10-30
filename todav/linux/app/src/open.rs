//! What Todav is asked to open: iCalendar files (as the system's calendar app, by `text/calendar`),
//! their events and tasks imported into the calendar and the list picked for them; and links to
//! a task or an event (`todav://`, `todav_app::links`), shown where they are.

use crate::core;
use crate::i18n::{gettext, ngettext};
use adw::prelude::*;
use gtk::{gio, glib};
use todav_core::List;

/// Open `files` (the window first, as the place to ask in).
pub fn open(app: &adw::Application, files: &[gio::File]) {
    crate::window::present(app);
    for file in files {
        // GIO knows no such scheme: the URI as given, not `has_uri_scheme`.
        let uri = file.uri();
        if uri.starts_with("todav:") {
            show(&uri);
            continue;
        }
        let file = file.clone();
        glib::spawn_future_local(async move {
            let name = file
                .basename()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            match file.load_contents_future().await {
                Ok((bytes, _)) => import(&name, String::from_utf8_lossy(&bytes).into_owned()).await,
                Err(e) => crate::window::toast(
                    &gettext("Cannot open {name}: {error}")
                        .replace("{name}", &name)
                        .replace("{error}", e.message()),
                ),
            }
        });
    }
}

/// Show the task (on its list) or the event (its day) link `url` is to.
pub fn show(url: &str) {
    use todav_app::links::{Link, parse};
    let found = match parse(url) {
        Some(Link::Task(uid)) if core().task(uid.clone()).is_some() => {
            crate::window::show_task(&uid);
            true
        }
        Some(Link::Event(uid)) => core()
            .event(uid)
            .map(|e| crate::calendar::show_day(e.start))
            .is_some(),
        _ => false,
    };
    if !found {
        crate::window::toast(&gettext("Not found: it may have been deleted"));
    }
}

/// Put the link to a task or an event on the clipboard.
pub fn copy_link(widget: &impl IsA<gtk::Widget>, link: String) {
    widget.clipboard().set_text(&link);
    crate::window::toast(&gettext("Link copied"));
}

/// Ask where the events and tasks of iCalendar text `ics` (file `name`) go, then import them.
async fn import(name: &str, ics: String) {
    let Some(window) = crate::window::window() else {
        return;
    };
    let count = core().ics_count(ics.clone());
    if count.events + count.tasks == 0 {
        crate::window::toast(&gettext("{name} holds no events or tasks").replace("{name}", name));
        return;
    }
    let writable = |lists: Vec<List>| -> Vec<List> {
        let mut v = todav_app::lists::by_place(core(), lists);
        v.retain(|l| l.writable);
        v
    };
    let calendars = writable(core().calendars());
    let lists = writable(core().lists());
    let group = adw::PreferencesGroup::new();
    let row = |title: String, n: u32, one: &str, many: &str, choices: &[List]| {
        let row = adw::ComboRow::builder()
            .title(title)
            .subtitle(ngettext(one, many, n.into()).replace("{n}", &n.to_string()))
            .build();
        crate::lists::picker(&row, choices, None);
        group.add(&row);
        row
    };
    let calendar = (count.events > 0 && !calendars.is_empty()).then(|| {
        row(
            gettext("Calendar"),
            count.events,
            "{n} event",
            "{n} events",
            &calendars,
        )
    });
    let list = (count.tasks > 0 && !lists.is_empty()).then(|| {
        row(
            gettext("Task list"),
            count.tasks,
            "{n} task",
            "{n} tasks",
            &lists,
        )
    });
    if calendar.is_none() && list.is_none() {
        crate::window::toast(&gettext("No calendar or task list to import into"));
        return;
    }

    let alert = adw::AlertDialog::new(
        Some(&gettext("Import “{name}”?").replace("{name}", name)),
        None,
    );
    alert.set_extra_child(Some(&group));
    alert.add_responses(&[
        ("cancel", &gettext("Cancel")),
        ("import", &gettext("Import")),
    ]);
    alert.set_response_appearance("import", adw::ResponseAppearance::Suggested);
    alert.set_default_response(Some("import"));
    alert.set_close_response("cancel");
    if alert.choose_future(Some(&window)).await != "import" {
        return;
    }
    let pick = |row: &Option<adw::ComboRow>, from: &[List]| {
        row.as_ref()
            .and_then(|r| from.get(r.selected() as usize))
            .map(|l| l.href.clone())
    };
    let (events_to, tasks_to) = (pick(&calendar, &calendars), pick(&list, &lists));
    let r = gio::spawn_blocking(move || core().import_ics(ics, events_to, tasks_to)).await;
    match r {
        Ok(Ok(n)) => {
            let n = n.events + n.tasks;
            crate::window::toast(
                &ngettext("{n} item imported", "{n} items imported", n.into())
                    .replace("{n}", &n.to_string()),
            );
            crate::sync_now();
        }
        Ok(Err(e)) => crate::window::toast(
            &gettext("Not imported: {error}").replace("{error}", &e.to_string()),
        ),
        Err(_) => crate::window::toast(&gettext("Not imported")),
    }
}
