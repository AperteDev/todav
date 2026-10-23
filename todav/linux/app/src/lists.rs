//! Task lists and calendars: the colour each is shown in, on both pages, and the dialog that
//! adds one or renames, recolours or deletes it.

use crate::core;
use crate::i18n::{gettext, ngettext};
use crate::window::{get, view};
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use todav_core::List;

/// Nextcloud's colour presets (its colour picker's palette), in its order.
const PRESETS: [&str; 12] = [
    "#B6469D", "#BF678B", "#C98879", "#D3A967", "#DDCB55", "#A5B872", "#6EA68F", "#3794AC",
    "#0082C9", "#2D73BE", "#5B64B3", "#8855A8",
];

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

/// A list's colour from the server, if it is one (it goes into CSS).
pub fn color(l: &List) -> String {
    l.color
        .as_deref()
        .filter(|c| {
            c.len() <= 9 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
        })
        .unwrap_or("#3584e4")
        .to_string()
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

/// Right-click or a long press on a row of `rows` offers to edit the list `href_at` gives
/// for it (None: a row that is no list).
pub fn context_menu(
    rows: &gtk::ListBox,
    href_at: impl Fn(&gtk::ListBoxRow) -> Option<String> + 'static,
) {
    let r = rows.clone();
    let show = Rc::new(move |x: f64, y: f64| {
        let Some(row) = r.row_at_y(y as i32) else {
            return false;
        };
        let Some(href) = href_at(&row) else {
            return false;
        };
        let m = gio::Menu::new();
        let action = gio::Action::print_detailed_name("win.edit-list", Some(&href.to_variant()));
        m.append(Some(&gettext("Edit…")), Some(&action));
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
        busy2(true);
        let (d, fail) = (d.clone(), fail2.clone());
        glib::spawn_future_local(async move {
            let r = gio::spawn_blocking(move || match href {
                Some(h) => core().edit_list(h, n, c).map(|_| None),
                None => core().create_list(n, c, events, false).map(Some),
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
    match list {
        Some(l) if l.writable => {
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
    format!(
        "{what} {}",
        gettext("It can be restored from the trash bin in Nextcloud Calendar.")
    )
}
