//! Task lists and calendars: the colour each is shown in, on both pages, and the dialog that
//! adds one or renames, recolours or deletes it.

use crate::core;
use crate::i18n::{gettext, ngettext};
use crate::window::{get, view};
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use todav_app::lists::{PRESETS, color};
use todav_core::{AccountKind, List};

/// Swatches per row in the dialog.
const COLUMNS: i32 = 7;

const CSS: &str = "
.swatch { min-width: 32px; min-height: 32px; padding: 0; color: white; }
.swatch:hover { filter: brightness(1.1); }
.swatch image { opacity: 0; }
.swatch.chosen image { opacity: 1; }
/* The custom colour: a smaller swatch in a rainbow ring, the size of the others in all. */
.custom-ring { padding: 4px; border-radius: 9999px; background-image: conic-gradient(#e01b24, #f6d32d, #33d17a, #3584e4, #9141ac, #e01b24); }
.custom-ring > button { min-width: 24px; min-height: 24px; padding: 0; box-shadow: 0 0 0 2px @dialog_bg_color; background-color: @dialog_bg_color; }
.custom-ring > button.custom { color: white; }
.custom-ring > button:hover { filter: brightness(1.1); }
";

thread_local! {
    /// The colour classes, and the CSS they were last loaded from.
    static COLORS: RefCell<Option<(gtk::CssProvider, String)>> = const { RefCell::new(None) };
}

/// The CSS class showing list `href` in its colour (see `paint`).
pub fn class(href: &str) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    href.hash(&mut h);
    format!("cal-{:x}", h.finish())
}

/// Load every list's colour class as it is now; both pages call this before drawing.
pub fn paint() {
    let mut all = core().calendars();
    all.extend(core().lists());
    let mut css: String = all
        .iter()
        .map(|l| {
            let (cls, col) = (class(&l.href), color(l));
            format!(
                ".dot.{cls}, .chip.all-day.{cls} {{ background-color: {col}; }}
                 .ring.{cls} {{ color: {col}; }}
                 .chip.all-day.{cls} {{ color: white; }}
                 .chip.{cls}:not(.all-day) {{ background-color: alpha({col}, 0.18); }}\n"
            )
        })
        .collect();
    css += CSS;
    for (i, p) in PRESETS.iter().enumerate() {
        css += &format!(".swatch.preset-{i} {{ background-color: {p}; }}\n");
    }
    let Some(display) = gdk::Display::default() else {
        return;
    };
    COLORS.with(|c| {
        let mut c = c.borrow_mut();
        let (provider, loaded) = c.get_or_insert_with(|| {
            let p = gtk::CssProvider::new();
            gtk::style_context_add_provider_for_display(
                &display,
                &p,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            (p, String::new())
        });
        if *loaded != css {
            provider.load_from_string(&css);
            *loaded = css;
        }
    });
}

/// A small round mark in list `href`'s colour.
pub fn dot(href: &str) -> gtk::Box {
    let dot = gtk::Box::builder().valign(gtk::Align::Center).build();
    dot.add_css_class("dot");
    dot.add_css_class(&class(href));
    dot
}

/// The window's actions: `win.new-list`, `win.new-calendar` and `win.edit-list` (with the
/// list's href).
pub fn setup(window: &adw::ApplicationWindow) {
    for (name, events) in [("new-list", false), ("new-calendar", true)] {
        let a = gio::SimpleAction::new(name, None);
        let w = window.downgrade();
        a.connect_activate(move |_, _| {
            if let Some(w) = w.upgrade() {
                open(w.upcast_ref(), None, events);
            }
        });
        window.add_action(&a);
    }
    let edit = gio::SimpleAction::new("edit-list", Some(glib::VariantTy::STRING));
    let w = window.downgrade();
    edit.connect_activate(move |_, href| {
        let (Some(w), Some(href)) = (w.upgrade(), href.and_then(|h| h.str().map(str::to_string)))
        else {
            return;
        };
        let mut all = core().calendars();
        all.extend(core().lists());
        if let Some(l) = all.into_iter().find(|l| l.href == href) {
            open(w.upcast_ref(), Some(l), false);
        }
    });
    window.add_action(&edit);
}

/// Right-click or a long press on a row of `rows` offers to edit what `target_at` gives for it
/// (None: a row with nothing to edit), with `action` (a list's "win.edit-list", a task's
/// "win.edit-task"), and to delete it with `delete` when given.
pub fn context_menu(
    rows: &gtk::ListBox,
    action: &'static str,
    delete: Option<&'static str>,
    target_at: impl Fn(&gtk::ListBoxRow) -> Option<String> + 'static,
) {
    let r = rows.clone();
    let show = Rc::new(move |x: f64, y: f64| {
        let Some(row) = r.row_at_y(y as i32) else {
            return false;
        };
        let Some(target) = target_at(&row) else {
            return false;
        };
        let m = gio::Menu::new();
        let target = target.to_variant();
        let edit = gio::Action::print_detailed_name(action, Some(&target));
        m.append(Some(&gettext("Edit…")), Some(&edit));
        if let Some(delete) = delete {
            let delete = gio::Action::print_detailed_name(delete, Some(&target));
            m.append(Some(&gettext("Delete")), Some(&delete));
        }
        // On the row (a list box takes no other children), gone once closed.
        let menu = gtk::PopoverMenu::from_model(Some(&m));
        menu.set_parent(&row);
        menu.set_has_arrow(false);
        menu.set_halign(gtk::Align::Start);
        let at = r
            .compute_point(&row, &gtk::graphene::Point::new(x as f32, y as f32))
            .unwrap_or_else(|| gtk::graphene::Point::new(x as f32, 0.0));
        menu.set_pointing_to(Some(&gdk::Rectangle::new(
            at.x() as i32,
            at.y() as i32,
            1,
            1,
        )));
        menu.connect_closed(|m| {
            let m = m.clone();
            glib::idle_add_local_once(move || m.unparent());
        });
        menu.popup();
        true
    });
    let click = gtk::GestureClick::builder()
        .button(gdk::BUTTON_SECONDARY)
        .build();
    let s = show.clone();
    click.connect_pressed(move |g, _, x, y| {
        if s(x, y) {
            g.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    rows.add_controller(click);
    let hold = gtk::GestureLongPress::builder().touch_only(true).build();
    hold.connect_pressed(move |g, x, y| {
        if show(x, y) {
            g.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    rows.add_controller(hold);
}

/// Sidebar row `row` of list `l` moves by dragging onto another of `siblings` (the hrefs of its
/// account's lists in that sidebar section, in order): above or below that one (its upper or
/// lower half). The new order syncs as Nextcloud's own, and stays here for lists on this device.
pub fn reorderable(row: &gtk::ListBoxRow, l: &List, siblings: Vec<String>) {
    // A variant, not a string: text fields would take a string and type it.
    let drag = gtk::DragSource::new();
    drag.set_actions(gdk::DragAction::MOVE);
    let value = l.href.to_variant().to_value();
    drag.set_content(Some(&gdk::ContentProvider::for_value(&value)));
    // Its list box shows where the row lands, not the highlight a task dropped on a row gets;
    // kept from now, as the drop makes the rows anew, maybe before the drag ends.
    let rows = row.parent();
    let (r, name, href, b) = (
        row.clone(),
        l.display_name.clone(),
        l.href.clone(),
        rows.clone(),
    );
    drag.connect_drag_begin(move |_, drag| {
        DRAGGED.replace(Some(href.clone()));
        // The list's name on a card; the row stays in place, faded.
        let card = gtk::Label::new(Some(&name));
        card.add_css_class("drag-card");
        if let Ok(icon) = gtk::DragIcon::for_drag(drag).downcast::<gtk::DragIcon>() {
            icon.set_child(Some(&card));
        }
        r.add_css_class("dragged");
        if let Some(b) = &b {
            b.add_css_class("reordering");
        }
    });
    let r = row.clone();
    drag.connect_drag_end(move |_, _, _| {
        DRAGGED.take();
        r.remove_css_class("dragged");
        if let Some(b) = &rows {
            b.remove_css_class("reordering");
        }
        clear_mark();
    });
    row.add_controller(drag);

    let below =
        |dt: &gtk::DropTarget, y: f64| dt.widget().is_some_and(|w| y > f64::from(w.height()) / 2.0);
    let drop = gtk::DropTarget::new(glib::Variant::static_type(), gdk::DragAction::MOVE);
    let (own, s) = (l.href.clone(), siblings.clone());
    drop.connect_accept(move |_, _| {
        DRAGGED.with_borrow(|d| d.as_ref().is_some_and(|d| *d != own && s.contains(d)))
    });
    // Where the dragged list lands next to this one, or None where it already is.
    let own = l.href.clone();
    let order = Rc::new(move |below: bool| {
        let moved = DRAGGED.with_borrow(Clone::clone)?;
        let mut order = siblings.clone();
        order.retain(|h| *h != moved);
        let at = order.iter().position(|h| *h == own).unwrap_or(order.len());
        order.insert(at + usize::from(below), moved);
        (order != siblings).then_some(order)
    });
    let o = order.clone();
    drop.connect_motion(move |dt, _, y| {
        if let Some(w) = dt.widget() {
            if o(below(dt, y)).is_some() {
                mark(&w, below(dt, y));
            } else {
                unmark(&w);
            }
        }
        gdk::DragAction::MOVE
    });
    drop.connect_leave(|dt| {
        if let Some(w) = dt.widget() {
            unmark(&w);
        }
    });
    drop.connect_drop(move |dt, _, _, y| {
        if let Some(w) = dt.widget() {
            unmark(&w);
        }
        let Some(order) = order(below(dt, y)) else {
            return false;
        };
        if let Err(e) = core().reorder_lists(order) {
            crate::window::toast(&e.to_string());
            return true;
        }
        // After the drop is done: the rows are made anew.
        glib::idle_add_local_once(|| {
            crate::window::lists_changed();
            crate::calendar::refresh();
            crate::request_sync();
        });
        true
    });
    row.add_controller(drop);
    row.add_css_class("reorderable");
}

thread_local! {
    /// The list being dragged by its sidebar row.
    static DRAGGED: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The drop line between rows: (row dragged over, row showing it, its class).
    static DROP_MARK: RefCell<Option<(gtk::Widget, gtk::Widget, &'static str)>> =
        const { RefCell::new(None) };
}

/// Show where a drop on `owner` lands. Below it is drawn as the line above the next row when
/// there is one under no heading of its own, so the gap between two rows has a single line (as
/// the task rows do). A list box keeps its headings as children after the rows: the last row's
/// next sibling may be one.
pub fn mark(owner: &gtk::Widget, below: bool) {
    let next = owner.next_sibling().filter(|n| {
        below
            && n.downcast_ref::<gtk::ListBoxRow>()
                .is_some_and(|r| r.header().is_none())
    });
    let (w, class) = match next {
        Some(next) => (next, "drop-above"),
        None => (
            owner.clone(),
            if below { "drop-below" } else { "drop-above" },
        ),
    };
    DROP_MARK.with_borrow_mut(|m| {
        if let Some((_, old, c)) = m.take() {
            old.remove_css_class(c);
        }
        w.add_css_class(class);
        *m = Some((owner.clone(), w, class));
    });
}

/// Remove the line `owner` drew; one the row now entered drew stays.
pub fn unmark(owner: &gtk::Widget) {
    DROP_MARK.with_borrow_mut(|m| {
        if m.as_ref().is_some_and(|(o, _, _)| o == owner) {
            let (_, w, c) = m.take().unwrap();
            w.remove_css_class(c);
        }
    });
}

/// Remove any drop line, at a drag's end: cancelled drags get no leave.
pub fn clear_mark() {
    if let Some((_, w, c)) = DROP_MARK.take() {
        w.remove_css_class(c);
    }
}

/// Where a list is kept, as its group is headed: its account's name.
pub fn place(account: i64) -> String {
    core()
        .account(account)
        .map(|a| crate::account_name(&a))
        .unwrap_or_default()
}

/// Where new task lists, or calendars (`events`), can go, as (heading, account), in the user's
/// order of the accounts.
pub fn places(events: bool) -> Vec<(String, i64)> {
    todav_app::lists::places(core(), events)
        .into_iter()
        .map(|id| (place(id), id))
        .collect()
}

/// The heading over each of `lists` (as `todav_app::lists::by_place` orders them): its
/// account on the first of a group, when they are from more than one.
pub fn headings(lists: &[List]) -> Vec<Option<String>> {
    todav_app::lists::headings(lists)
        .into_iter()
        .map(|h| h.map(place))
        .collect()
}

/// A heading above sidebar rows, as libadwaita's sidebars head their sections.
pub fn heading_label(text: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(6)
        .build();
    label.add_css_class("caption-heading");
    label.add_css_class("dim-label");
    label
}

/// Head the rows of `rows` with `headings`, one per row from row `skip` on.
pub fn set_headings(rows: &gtk::ListBox, skip: usize, headings: Vec<Option<String>>) {
    rows.set_header_func(move |row, _| {
        let text = (row.index() as usize)
            .checked_sub(skip)
            .and_then(|i| headings.get(i).cloned().flatten());
        row.set_header(text.map(|t| heading_label(&t)).as_ref());
    });
}

/// Fill `row` with the names of `lists` (as `by_place` orders them), a section per place under
/// its heading when they are in more than one: the task and event dialogs' list picker.
pub fn picker(row: &adw::ComboRow, lists: &[List]) {
    let headings = headings(lists);
    let sections = gio::ListStore::new::<gtk::StringList>();
    let mut names = gtk::StringList::new(&[]);
    for (l, heading) in lists.iter().zip(&headings) {
        if heading.is_some() || sections.n_items() == 0 {
            names = gtk::StringList::new(&[]);
            sections.append(&names);
        }
        names.append(&l.display_name);
    }
    row.set_model(Some(&gtk::FlattenListModel::new(Some(sections))));
    if headings.iter().all(Option::is_none) {
        return;
    }
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, h| {
        if let Some(h) = h.downcast_ref::<gtk::ListHeader>() {
            h.set_child(Some(&heading_label("")));
        }
    });
    factory.connect_bind(move |_, h| {
        let Some(h) = h.downcast_ref::<gtk::ListHeader>() else {
            return;
        };
        let text = headings.get(h.start() as usize).cloned().flatten();
        if let Some(label) = h.child().and_downcast::<gtk::Label>() {
            label.set_label(&text.unwrap_or_default());
        }
    });
    row.set_header_factory(Some(&factory));
}

/// "#rrggbb" (or with alpha) as "#RRGGBB", to compare with the presets and to save.
fn normal(c: &str) -> String {
    c.get(..7).unwrap_or(c).to_ascii_uppercase()
}

/// Add a task list, or a calendar of events (`events`); or edit `list` (its kind decides).
pub fn open(parent: &gtk::Widget, list: Option<List>, events: bool) {
    paint(); // the swatches' colours
    let b = view!("list-dialog");
    let dialog: adw::Dialog = get(&b, "dialog");
    let (name, save, error): (adw::EntryRow, gtk::Button, adw::Banner) =
        (get(&b, "name"), get(&b, "save"), get(&b, "error"));
    let events = list.as_ref().map_or(events, |l| l.holds_events);
    dialog.set_title(&match (events, list.is_some()) {
        (true, true) => gettext("Edit Calendar"),
        (true, false) => gettext("New Calendar"),
        (false, true) => gettext("Edit List"),
        (false, false) => gettext("New List"),
    });
    let save_label = if list.is_some() {
        gettext("Save")
    } else {
        gettext("Create")
    };
    save.set_label(&save_label);

    // A new one gets the first preset no other list has.
    let mut all = core().calendars();
    all.extend(core().lists());
    let start = match &list {
        Some(l) => normal(&color(l)),
        None => PRESETS
            .iter()
            .find(|p| !all.iter().any(|l| normal(&color(l)) == **p))
            .unwrap_or(&PRESETS[0])
            .to_string(),
    };
    let chosen = Rc::new(RefCell::new(start));

    // The custom swatch's colour, while the dialog is open.
    let custom_css = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(
        &dialog.display(),
        &custom_css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let css = custom_css.clone();
    dialog.connect_closed(move |d| {
        gtk::style_context_remove_provider_for_display(&d.display(), &css);
    });

    let grid: gtk::Grid = get(&b, "colors");
    let swatch = |tip: &str| {
        let b = gtk::Button::builder()
            .child(&gtk::Image::from_icon_name("object-select-symbolic"))
            .tooltip_text(tip)
            .valign(gtk::Align::Center)
            .halign(gtk::Align::Center)
            .build();
        b.add_css_class("circular");
        b
    };
    let presets: Vec<gtk::Button> = (0..PRESETS.len())
        .map(|i| {
            let b = swatch(PRESETS[i]);
            b.add_css_class("swatch");
            b.add_css_class(&format!("preset-{i}"));
            grid.attach(&b, i as i32 % COLUMNS, i as i32 / COLUMNS, 1, 1);
            b
        })
        .collect();
    // In a rainbow ring: any colour, set apart from the presets.
    let custom = swatch(&gettext("Custom Color…"));
    let ring = gtk::Box::builder()
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .css_classes(["custom-ring"])
        .build();
    ring.append(&custom);
    let n = PRESETS.len() as i32;
    grid.attach(&ring, n % COLUMNS, n / COLUMNS, 1, 1);

    // The chosen swatch has a check; a colour that is no preset shows in the last one.
    let render = {
        let (chosen, presets, custom) = (chosen.clone(), presets.clone(), custom.clone());
        Rc::new(move || {
            let c = chosen.borrow();
            let at = PRESETS.iter().position(|p| *p == *c);
            for (i, b) in presets.iter().enumerate() {
                if at == Some(i) {
                    b.add_css_class("chosen");
                } else {
                    b.remove_css_class("chosen");
                }
            }
            let icon = if at.is_none() {
                custom_css.load_from_string(&format!(
                    ".custom-ring > button.custom {{ background-color: {c}; }}"
                ));
                custom.set_css_classes(&["circular", "custom"]);
                "object-select-symbolic"
            } else {
                custom.set_css_classes(&["circular"]);
                "view-more-horizontal-symbolic"
            };
            custom.set_child(Some(&gtk::Image::from_icon_name(icon)));
        })
    };
    render();
    for (i, b) in presets.iter().enumerate() {
        let (chosen, render) = (chosen.clone(), render.clone());
        b.connect_clicked(move |_| {
            *chosen.borrow_mut() = PRESETS[i].to_string();
            render();
        });
    }
    {
        let (chosen, render, d) = (chosen.clone(), render.clone(), dialog.clone());
        custom.connect_clicked(move |_| {
            let (chosen, render) = (chosen.clone(), render.clone());
            let window = d.root().and_downcast::<gtk::Window>();
            let initial = gdk::RGBA::parse(chosen.borrow().as_str()).ok();
            glib::spawn_future_local(async move {
                let picker = gtk::ColorDialog::builder()
                    .title(gettext("Custom Color"))
                    .with_alpha(false)
                    .build();
                let Ok(c) = picker
                    .choose_rgba_future(window.as_ref(), initial.as_ref())
                    .await
                else {
                    return; // cancelled
                };
                let hex = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                *chosen.borrow_mut() = format!(
                    "#{:02X}{:02X}{:02X}",
                    hex(c.red()),
                    hex(c.green()),
                    hex(c.blue())
                );
                render();
            });
        });
    }

    // A new one: in which account, picked when there is a choice.
    let places = places(events);
    let place: adw::ComboRow = get(&b, "place");
    let names: gtk::StringList = get(&b, "places");
    for (heading, _) in &places {
        names.append(heading);
    }
    place.set_visible(list.is_none() && places.len() > 1);

    let s = save.clone();
    name.connect_changed(move |e| s.set_sensitive(!e.text().trim().is_empty()));
    if let Some(l) = &list {
        name.set_text(&l.display_name);
    }
    let d = dialog.clone();
    get::<gtk::Button>(&b, "cancel").connect_clicked(move |_| {
        d.close();
    });

    // Busy while the server answers; a failure stays in the dialog.
    let busy = {
        let (save, dialog) = (save.clone(), dialog.clone());
        move |on: bool| {
            dialog.set_can_close(!on);
            save.set_sensitive(!on);
            if on {
                save.set_child(Some(&adw::Spinner::new()));
            } else {
                save.set_label(&save_label);
            }
        }
    };
    let fail = {
        let (busy, error) = (busy.clone(), error.clone());
        move |msg: String| {
            busy(false);
            error.set_title(&msg);
            error.set_revealed(true);
        }
    };

    let href = list.as_ref().map(|l| l.href.clone());
    let (d, busy2, fail2) = (dialog.clone(), busy.clone(), fail.clone());
    save.connect_clicked(move |_| {
        let n = name.text().trim().to_string();
        if n.is_empty() {
            return;
        }
        let (c, href) = (chosen.borrow().clone(), href.clone());
        let account = places
            .get(place.selected() as usize)
            .map_or(0, |(_, id)| *id);
        busy2(true);
        let (d, fail) = (d.clone(), fail2.clone());
        glib::spawn_future_local(async move {
            let r = gio::spawn_blocking(move || match href {
                Some(h) => core().edit_list(h, n, c).map(|_| None),
                None => core().create_list(account, n, c, events).map(Some),
            })
            .await;
            match r {
                Ok(Ok(created)) => {
                    d.set_can_close(true);
                    d.close();
                    // A new task list opens on the Tasks page.
                    if let Some(h) = created.filter(|_| !events) {
                        crate::window::show_list(&h);
                    }
                }
                Ok(Err(e)) => {
                    fail(gettext("Not saved: {error}").replace("{error}", &e.to_string()))
                }
                Err(_) => fail(gettext("Not saved")),
            }
        });
    });

    let delete: adw::ButtonRow = get(&b, "delete");
    delete.set_title(&if events {
        gettext("Delete Calendar")
    } else {
        gettext("Delete List")
    });
    let delete_box: gtk::ListBox = get(&b, "delete_box");
    // Google's calendars are deleted in Google Calendar (its task lists here).
    let google = |l: &List| {
        l.holds_events
            && core()
                .account(l.account)
                .is_some_and(|a| a.kind == AccountKind::Google)
    };
    match list {
        Some(l) if l.writable && !google(&l) => {
            let d = dialog.clone();
            delete.connect_activated(move |_| {
                let (d, l, busy, fail) = (d.clone(), l.clone(), busy.clone(), fail.clone());
                glib::spawn_future_local(async move {
                    let alert = adw::AlertDialog::new(
                        Some(&if events {
                            gettext("Delete Calendar?")
                        } else {
                            gettext("Delete List?")
                        }),
                        Some(&delete_question(&l)),
                    );
                    alert.add_responses(&[
                        ("cancel", &gettext("Cancel")),
                        ("delete", &gettext("Delete")),
                    ]);
                    alert.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
                    alert.set_default_response(Some("cancel"));
                    alert.set_close_response("cancel");
                    if alert.choose_future(Some(&d)).await != "delete" {
                        return;
                    }
                    busy(true);
                    let href = l.href.clone();
                    match gio::spawn_blocking(move || core().delete_list(href)).await {
                        Ok(Ok(())) => {
                            d.set_can_close(true);
                            d.close();
                        }
                        Ok(Err(e)) => {
                            fail(gettext("Not deleted: {error}").replace("{error}", &e.to_string()))
                        }
                        Err(_) => fail(gettext("Not deleted")),
                    }
                });
            });
        }
        _ => delete_box.set_visible(false),
    }
    dialog.present(Some(parent));
}

/// What deleting list `l` deletes with it (its events, its tasks), as whole sentences for
/// translation; and where it can be got back.
fn delete_question(l: &List) -> String {
    let n = core().tasks(l.href.clone(), true).len() as u64;
    let what = match (l.holds_events, n) {
        (true, 0) => gettext("“{name}” will be deleted with its events."),
        (true, n) => ngettext(
            "“{name}” will be deleted with its events and {n} task.",
            "“{name}” will be deleted with its events and {n} tasks.",
            n,
        ),
        (false, 0) => gettext("“{name}” will be deleted."),
        (false, n) => ngettext(
            "“{name}” will be deleted with its {n} task.",
            "“{name}” will be deleted with its {n} tasks.",
            n,
        ),
    };
    let what = what
        .replace("{name}", &l.display_name)
        .replace("{n}", &n.to_string());
    let back = if l.local {
        gettext("It is on this device only and cannot be restored.")
    } else {
        gettext("It can be restored from the trash bin in Nextcloud Calendar.")
    };
    format!("{what} {back}")
}
