//! A new event's popover, as GNOME Calendar's: a click on a day of the month, or a drag across
//! days, offers one with its title, colour and calendar, laid out as an event's details; Edit…
//! opens the whole dialog with them.

use super::*;

/// Offer a new event at `time` from a popover on `parent`; `closed` runs once it is gone. Waits
/// out the colour dialog its colour box opens over the window.
pub(super) fn open(
    c: &Rc<Cal>,
    parent: &impl IsA<gtk::Widget>,
    time: EventTime,
    closed: impl Fn() + 'static,
) {
    // Calendars we may write to, as the dialog offers them; the one used last picked.
    let writable = todav_app::lists::by_place(
        core(),
        c.lists
            .borrow()
            .iter()
            .filter(|l| l.holds_events && l.writable)
            .cloned()
            .collect(),
    );
    if writable.is_empty() {
        crate::window::toast(&gettext("No calendar to save to"));
        return closed();
    }
    let last = core().setting("last_calendar".into());
    let first = writable
        .iter()
        .position(|l| Some(&l.href) == last.as_ref())
        .unwrap_or(0);

    let b = details_box();
    let color: Rc<RefCell<Option<String>>> = Rc::default();
    let away: Rc<Cell<bool>> = Rc::default();
    let c2 = color.clone();
    let (color_box, set_list) = dialog::color_button(
        None,
        writable[first].href.clone(),
        Some(away.clone()),
        move |c| *c2.borrow_mut() = c,
    );
    let title = gtk::Entry::builder()
        .placeholder_text(gettext("Title"))
        .hexpand(true)
        .build();
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    head.append(&color_box);
    head.append(&title);
    b.append(&head);
    b.append(&line("x-office-calendar-symbolic", &span(&time)));
    let calendar = picker(&writable, first);
    calendar.connect_selected_notify(move |d| {
        if let Some(l) = href(d) {
            set_list(&l);
        }
    });
    b.append(&calendar);

    let row = gtk::Box::builder().spacing(6).margin_top(4).build();
    let edit = gtk::Button::with_label(&gettext("Edit…"));
    let add = gtk::Button::builder()
        .label(gettext("Add"))
        .hexpand(true)
        .halign(gtk::Align::End)
        .sensitive(false)
        .build();
    add.add_css_class("suggested-action");
    row.append(&edit);
    row.append(&add);
    b.append(&row);
    let a = add.clone();
    title.connect_changed(move |t| a.set_sensitive(!t.text().trim().is_empty()));

    let pop = gtk::Popover::builder().child(&b).build();
    pop.add_css_class("quick-add");
    pop.set_parent(parent);
    // The title to type, again after the colour dialog.
    let t = title.clone();
    pop.connect_show(move |_| {
        t.grab_focus_without_selecting();
    });
    c.holding.set(true);
    let a = away.clone();
    pop.connect_closed(move |p| {
        if a.get() {
            return;
        }
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
        closed();
        if let Some(c) = cal() {
            c.holding.set(false);
            if c.pending.replace(false) {
                glib::idle_add_local_once(refresh);
            }
        }
    });

    // What it has: (title, calendar, colour).
    let read = {
        let (title, calendar, color) = (title.clone(), calendar.clone(), color.clone());
        move || {
            Some((
                title.text().trim().to_string(),
                href(&calendar)?,
                color.borrow().clone(),
            ))
        }
    };
    let save = {
        let (read, p) = (read.clone(), pop.downgrade());
        move || {
            let Some((title, list, color)) = read().filter(|r| !r.0.is_empty()) else {
                return;
            };
            let _ = core().set_setting("last_calendar".into(), list.clone());
            let added = core().together(|| {
                let uid = core().add_event(list.clone(), title, time, Vec::new())?;
                if color.is_some() {
                    let patch = EventPatch {
                        color: Some(color),
                        ..Default::default()
                    };
                    core().update_event(list, uid, None, Scope::All, patch)?;
                }
                Ok::<_, todav_core::Error>(())
            });
            match added {
                Ok(()) => request_sync(),
                Err(err) => crate::window::toast(&err.to_string()),
            }
            if let Some(p) = p.upgrade() {
                close_for_change(&p);
            }
        }
    };
    let s = save.clone();
    title.connect_activate(move |_| s());
    add.connect_clicked(move |_| save());
    let p = pop.downgrade();
    edit.connect_clicked(move |_| {
        let (Some(c), Some((title, list, color))) = (cal(), read()) else {
            return;
        };
        if let Some(p) = p.upgrade() {
            close_for_change(&p);
        }
        dialog::open_draft(&c, time, dialog::Draft { title, list, color });
    });
    pop.popup();
}

/// The calendar picked in a `picker`.
fn href(d: &gtk::DropDown) -> Option<String> {
    d.selected_item()
        .and_downcast::<gtk::StringObject>()
        .map(|s| s.string().to_string())
}

/// The calendars `lists` to pick from, `first` picked: each its dot and name, as the line of an
/// event's details says which, under the place it is kept when there are several.
fn picker(lists: &[List], first: usize) -> gtk::DropDown {
    let headings = crate::lists::headings(lists);
    let sections = gio::ListStore::new::<gtk::StringList>();
    let mut hrefs = gtk::StringList::new(&[]);
    for (l, heading) in lists.iter().zip(&headings) {
        if heading.is_some() || sections.n_items() == 0 {
            hrefs = gtk::StringList::new(&[]);
            sections.append(&hrefs);
        }
        hrefs.append(&l.href);
    }
    let names: std::collections::HashMap<String, String> = lists
        .iter()
        .map(|l| (l.href.clone(), l.display_name.clone()))
        .collect();
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(&gtk::Box::builder().valign(gtk::Align::Center).build());
        row.append(&gtk::Label::builder().xalign(0.0).build());
        item.set_child(Some(&row));
    });
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let (Some(href), Some(row)) = (
            item.item().and_downcast::<gtk::StringObject>(),
            item.child(),
        ) else {
            return;
        };
        let href = href.string();
        if let Some(dot) = row.first_child() {
            dot.set_css_classes(&["dot", &class(&href)]);
            if let Some(label) = dot.next_sibling().and_downcast::<gtk::Label>() {
                label.set_label(names.get(href.as_str()).map_or("", String::as_str));
            }
        }
    });
    let d = gtk::DropDown::builder()
        .model(&gtk::FlattenListModel::new(Some(sections)))
        .factory(&factory)
        .selected(first as u32)
        .tooltip_text(gettext("Calendar"))
        .halign(gtk::Align::Start)
        .build();
    d.add_css_class("flat");
    if headings.iter().any(Option::is_some) {
        let header = gtk::SignalListItemFactory::new();
        // As the dialog's calendar row has them (lists::picker), whole: libadwaita pads list
        // headers already. Lined up with the dots under them.
        header.connect_setup(|_, h| {
            if let Some(h) = h.downcast_ref::<gtk::ListHeader>() {
                let label = crate::lists::heading_label("");
                label.set_margin_top(0);
                label.set_margin_bottom(0);
                label.set_margin_start(15);
                label.set_ellipsize(gtk::pango::EllipsizeMode::None);
                h.set_child(Some(&label));
            }
        });
        header.connect_bind(move |_, h| {
            let Some(h) = h.downcast_ref::<gtk::ListHeader>() else {
                return;
            };
            let text = headings.get(h.start() as usize).cloned().flatten();
            if let Some(label) = h.child().and_downcast::<gtk::Label>() {
                label.set_label(text.as_deref().unwrap_or_default());
            }
        });
        d.set_header_factory(Some(&header));
    }
    d
}
