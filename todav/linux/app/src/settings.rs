//! Preferences dialog: General (style, what opens at start), Accounts (a page per account:
//! switched on, name, connection, server status; the ntfy server with a connection test,
//! applied on close; settings sync), Tasks (order and display, applied at once), Calendar and
//! Extension.

use crate::core;
use crate::i18n::{gettext, gettext_noop, pgettext};
use crate::window::{get, note, url_text, view};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use todav_app::prefs::{self, DURATIONS};
use todav_core::{Account, AccountKind};

type UrlRow = (gtk::DropDown, adw::EntryRow);

/// Light or dark as the device-only `color_scheme` says: "light", "dark", else the system's.
pub fn apply_color_scheme() {
    let scheme = match core().setting("color_scheme".into()).as_deref() {
        Some("light") => adw::ColorScheme::ForceLight,
        Some("dark") => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    };
    adw::StyleManager::default().set_color_scheme(scheme);
}

/// The GNOME extension's options (Preferences → Extension, synced like the rest) with their
/// defaults; it reads them over D-Bus. `ext_list` is the list it shows (empty: the first). A part
/// (`EXTENSION_PARTS`) is in its own menu (`true`), the clock menu (`clock`) or not shown.
pub const EXTENSION: [(&str, &str); 12] = [
    ("ext_columns", "double"),
    ("ext_order", "tasks"),
    ("ext_icon", "center"),
    ("ext_tasks", "true"),
    ("ext_subtasks", "true"),
    ("ext_add_task", "true"),
    ("ext_month", "true"),
    ("ext_month_dots", "true"),
    ("ext_agenda", "true"),
    ("ext_add_event", "true"),
    ("ext_open", "true"),
    ("ext_list", ""),
];
const EXTENSION_PARTS: [&str; 5] = [
    "ext_tasks",
    "ext_add_task",
    "ext_month",
    "ext_agenda",
    "ext_add_event",
];

/// `EXTENSION` with this device's values.
pub fn extension() -> Vec<(String, String)> {
    EXTENSION
        .iter()
        .map(|(k, default)| {
            let v = core().setting((*k).into());
            (k.to_string(), v.unwrap_or_else(|| default.to_string()))
        })
        .collect()
}

fn connect_duration(b: &gtk::Builder, key: &'static str) {
    let row: adw::ComboRow = get(b, key);
    let current = core()
        .setting(key.into())
        .and_then(|v| v.parse::<i64>().ok());
    let current = current.filter(|m| *m > 0).unwrap_or(60);
    let hour = DURATIONS.iter().position(|m| *m == 60).unwrap_or(0);
    row.set_selected(DURATIONS.iter().position(|m| *m == current).unwrap_or(hour) as u32);
    row.connect_selected_notify(move |r| {
        let _ = core().set_setting(key.into(), DURATIONS[r.selected() as usize].to_string());
    });
}

/// Preferences → Extension's choice of where its parts go: all in its own menu (`panel`), all
/// in the clock menu (`clock`), or each where its row says (`custom`: the rows shown, as they
/// were). Stored in `ext_preset`; unset, it is what the parts say.
fn connect_ext_preset(b: &gtk::Builder) {
    let parts: Vec<adw::ComboRow> = EXTENSION_PARTS.iter().map(|k| get(b, k)).collect();
    let current = core().setting("ext_preset".into()).unwrap_or_else(|| {
        let first = parts[0].selected();
        match first {
            0 | 1 if parts.iter().all(|p| p.selected() == first) => {
                ["panel", "clock"][first as usize]
            }
            _ => "custom",
        }
        .into()
    });
    let panel: adw::PreferencesGroup = get(b, "ext_panel_group");
    // The groups' notes on the clock menu, only where the rows choosing it are shown.
    let notes: Vec<(adw::PreferencesGroup, Option<glib::GString>)> =
        ["ext_tasks_group", "ext_calendar_group"]
            .iter()
            .map(|id| {
                let g: adw::PreferencesGroup = get(b, id);
                let d = g.description();
                (g, d)
            })
            .collect();
    let show = Rc::new({
        let parts = parts.clone();
        move |preset: &str| {
            let custom = preset == "custom";
            parts.iter().for_each(|p| p.set_visible(custom));
            panel.set_visible(preset != "clock");
            for (group, note) in &notes {
                group.set_description(note.as_deref().filter(|_| custom));
            }
        }
    });
    show(&current);
    for (name, place) in [("panel", Some(0)), ("clock", Some(1)), ("custom", None)] {
        let check: gtk::CheckButton = get(b, &format!("ext_preset_{name}"));
        check.set_active(name == current);
        let (parts, show) = (parts.clone(), show.clone());
        check.connect_toggled(move |c| {
            if !c.is_active() {
                return;
            }
            let _ = core().set_setting("ext_preset".into(), name.into());
            if let Some(i) = place {
                parts.iter().for_each(|p| p.set_selected(i));
            }
            show(name);
        });
        if place.is_some() {
            let picture: gtk::Button = get(b, &format!("ext_preset_{name}_picture"));
            picture.connect_clicked(move |_| check.set_active(true));
        }
    }
}

/// Bind a ComboRow to the setting `key`, one stored value per row entry (the first is the
/// default); changes apply at once.
fn connect_choice(b: &gtk::Builder, key: &'static str, values: &'static [&'static str]) {
    let row: adw::ComboRow = get(b, key);
    let current = core().setting(key.into()).unwrap_or_default();
    row.set_selected(values.iter().position(|v| *v == current).unwrap_or(0) as u32);
    row.connect_selected_notify(move |r| {
        let _ = core().set_setting(key.into(), values[r.selected() as usize].into());
        crate::window::refresh();
    });
}

/// Tie a "Sort" row to its "Add new …" row: sorting by creation date shows (and locks) where
/// new items go; manual and due date sorting let the user pick (by due date: among the undated
/// ones). `hint` is the "Add new …" subtitle when the user picks.
fn connect_order(b: &gtk::Builder, which: &'static str, hint: Option<&'static str>) {
    let (sort_key, new_key) = (format!("{which}_sort"), format!("{which}_new"));
    let sort: adw::ComboRow = get(b, &sort_key);
    let new: adw::ComboRow = get(b, &new_key);
    let show = {
        let (sort, new, new_key) = (sort.clone(), new.clone(), new_key.clone());
        move || {
            let manual = !matches!(sort.selected(), 1 | 2);
            let start = match sort.selected() {
                1 => true,
                2 => false,
                _ => core().setting(new_key.clone()).as_deref() == Some("start"),
            };
            new.set_sensitive(manual); // first, so the change below is not saved
            new.set_selected(if start { 0 } else { 1 });
            new.set_subtitle(&if manual {
                hint.map(gettext).unwrap_or_default()
            } else {
                gettext("Set by the sort order")
            });
        }
    };
    sort.set_selected(prefs::sort_index(core(), &sort_key) as u32);
    show();
    sort.connect_selected_notify(move |s| {
        let _ = core().set_setting(
            sort_key.clone(),
            prefs::SORTS[s.selected() as usize].0.into(),
        );
        show();
        crate::window::refresh();
    });
    new.connect_selected_notify(move |n| {
        if n.is_sensitive() {
            let v = if n.selected() == 0 { "start" } else { "end" };
            let _ = core().set_setting(new_key.clone(), v.into());
            crate::window::refresh();
        }
    });
}

/// The default for new reminders and the time reminders of all-day tasks go off; both apply to
/// reminders added from now on.
fn connect_reminders(b: &gtk::Builder) {
    let default: adw::ComboRow = get(b, "reminder_default");
    let model: gtk::StringList = get(b, "reminder_defaults");
    for label in crate::dates::TIMED {
        model.append(&gettext(label));
    }
    let current = prefs::default_reminder(core(), false);
    let pos = todav_app::dates::TIMED.iter().position(|o| *o == current);
    default.set_selected(pos.unwrap_or(0) as u32);
    default.connect_selected_notify(|r| {
        let offset = todav_app::dates::TIMED[r.selected() as usize];
        let _ = core().set_setting("reminder_default".into(), offset.to_string());
    });

    let (hour, minute): (gtk::SpinButton, gtk::SpinButton) =
        (get(b, "reminder_hour"), get(b, "reminder_minute"));
    let t = prefs::all_day_time(core());
    hour.set_value((t / 3600) as f64);
    minute.set_value((t / 60 % 60) as f64);
    for spin in [&hour, &minute] {
        crate::window::two_digits(spin);
        crate::window::scroll_past(spin.upcast_ref());
        let (h, m) = (hour.clone(), minute.clone());
        spin.connect_value_changed(move |_| {
            let minutes = h.value() as i64 * 60 + m.value() as i64;
            let _ = core().set_setting("reminder_time".into(), minutes.to_string());
        });
    }
}

/// Open Preferences on `page` (a page's `name` in ui/settings.blp: "general", "accounts",
/// "tasks", "calendar", "extension"; `None` for the first), or turn the open ones to it.
pub fn present(parent: &adw::ApplicationWindow, page: Option<&str>) -> adw::PreferencesDialog {
    if let Some(open) = parent
        .visible_dialog()
        .and_downcast::<adw::PreferencesDialog>()
    {
        if let Some(page) = page {
            open.set_visible_page_name(page);
        }
        return open;
    }
    let b = view!("settings");
    let dialog: adw::PreferencesDialog = get(&b, "dialog");
    if let Some(page) = page {
        dialog.set_visible_page_name(page);
    }
    let ntfy: UrlRow = (get(&b, "ntfy_scheme"), get(&b, "ntfy"));
    connect_accounts(&b, &dialog);
    let old_ntfy = core().setting("ntfy_url".into()).unwrap_or_default();
    let sync_ntfy: adw::SwitchRow = get(&b, "sync_ntfy");
    sync_ntfy.set_active(core().setting("sync_ntfy".into()).as_deref() != Some("false"));
    sync_ntfy.connect_active_notify(move |s| {
        let _ = core().set_setting("sync_ntfy".into(), s.is_active().to_string());
        crate::sync_now(); // turning it on pulls the server's ntfy server right away
    });
    let style: adw::ToggleGroup = get(&b, "color_scheme");
    let current = core().setting("color_scheme".into());
    style.set_active_name(Some(match current.as_deref() {
        Some("light") => "light",
        Some("dark") => "dark",
        _ => "system",
    }));
    style.connect_active_name_notify(|g| {
        let name = g.active_name().map(|n| n.to_string()).unwrap_or_default();
        let _ = core().set_setting("color_scheme".into(), name);
        apply_color_scheme();
    });
    connect_choice(&b, "start_view", &["last", "tasks", "calendar"]);
    connect_choice(
        &b,
        "start_calendar_view",
        &[
            "last",
            "month",
            "week",
            "work_week",
            "three_days",
            "day",
            "agenda",
        ],
    );
    let work: adw::ActionRow = get(&b, "work_days");
    work.set_subtitle(&crate::calendar::work_days_text());
    work.connect_activated(|row| {
        let r = row.downgrade();
        crate::calendar::work_days_dialog(row, move || {
            if let Some(r) = r.upgrade() {
                r.set_subtitle(&crate::calendar::work_days_text());
            }
        });
    });
    connect_choice(&b, "category_view", &["group", "tags", "none"]);
    connect_choice(&b, "done_tasks", &["group", "in_place", "last"]);
    connect_choice(&b, "done_subtasks", &["show", "hide", "last"]);
    connect_choice(&b, "complete_subtasks", &["complete", "keep", "ask"]);
    connect_duration(&b, "task_duration");
    connect_duration(&b, "event_duration");
    let dots: adw::SwitchRow = get(&b, "mini_dots");
    dots.set_active(prefs::mini_dots(core()));
    dots.connect_active_notify(|d| {
        let _ = core().set_setting("mini_dots".into(), d.is_active().to_string());
        crate::calendar::refresh();
    });
    let double: adw::SwitchRow = get(&b, "double_click_edit");
    double.set_active(prefs::double_click_edit(core()));
    double.connect_active_notify(|d| {
        let _ = core().set_setting("double_click_edit".into(), d.is_active().to_string());
    });
    // The extension: its rows carry the setting's name as their id. It hears of a change at
    // once, to put its parts and icon where they now go.
    let ext_changed = || crate::dbus::on_event(&crate::Event::Changed(String::new()));
    connect_choice(&b, "ext_columns", &["double", "single"]);
    connect_choice(&b, "ext_order", &["tasks", "calendar"]);
    connect_choice(&b, "ext_icon", &["center", "right"]);
    for key in EXTENSION_PARTS {
        connect_choice(&b, key, &["true", "clock", "false"]);
        get::<adw::ComboRow>(&b, key).connect_selected_notify(move |_| ext_changed());
    }
    for key in ["ext_subtasks", "ext_month_dots", "ext_open"] {
        let row: adw::SwitchRow = get(&b, key);
        row.set_active(core().setting(key.into()).as_deref() != Some("false"));
        row.connect_active_notify(move |r| {
            let _ = core().set_setting(key.into(), r.is_active().to_string());
        });
    }
    get::<adw::ComboRow>(&b, "ext_icon").connect_selected_notify(move |_| ext_changed());
    // Subtasks and dots only where the list and the month are shown.
    for (part, option) in [
        ("ext_tasks", "ext_subtasks"),
        ("ext_month", "ext_month_dots"),
    ] {
        let (part, option): (adw::ComboRow, adw::SwitchRow) = (get(&b, part), get(&b, option));
        let sync = move |p: &adw::ComboRow| option.set_sensitive(p.selected() != 2);
        sync(&part);
        part.connect_selected_notify(sync);
    }
    connect_ext_preset(&b);
    // Stars and pins, on unless turned off; the sidebar may gain or lose Important.
    for key in ["show_stars", "show_pins"] {
        let row: adw::SwitchRow = get(&b, key);
        row.set_active(core().setting(key.into()).as_deref() != Some("false"));
        row.connect_active_notify(move |r| {
            let _ = core().set_setting(key.into(), r.is_active().to_string());
            crate::window::refresh_all();
        });
    }
    let inline: adw::SwitchRow = get(&b, "inline_edit");
    inline.set_active(prefs::inline_edit(core()));
    inline.connect_active_notify(|r| {
        let _ = core().set_setting("inline_edit".into(), r.is_active().to_string());
        crate::window::refresh();
    });
    let handles: adw::SwitchRow = get(&b, "drag_handles");
    handles.set_active(core().setting("drag_handles".into()).as_deref() == Some("true"));
    handles.connect_active_notify(|h| {
        let _ = core().set_setting("drag_handles".into(), h.is_active().to_string());
        crate::window::refresh();
    });
    connect_order(&b, "task", None);
    connect_order(
        &b,
        "sub",
        Some(gettext_noop("Also places the “Add subtask” field")),
    );
    connect_reminders(&b);
    set_url(&ntfy, &old_ntfy);

    connect_test(
        &dialog,
        &get(&b, "ntfy_test"),
        &ntfy,
        todav_core::ntfy_check,
    );

    dialog.connect_closed(move |_| {
        let new_ntfy = url_text(&ntfy);
        if new_ntfy != old_ntfy {
            let _ = core().set_setting("ntfy_url".into(), new_ntfy.clone());
            if new_ntfy.is_empty() {
                // No listener anymore: stop the server pushing to the old ntfy topic.
                std::thread::spawn(|| core().push_unregister());
            }
            crate::restart_push();
        }
    });
    dialog.present(Some(parent));
    dialog
}

/// Preferences on account `id`'s page.
pub fn present_account(parent: &adw::ApplicationWindow, id: i64) {
    let dialog = present(parent, Some("accounts"));
    dialog.pop_subpage();
    dialog.push_subpage(&account_page(&dialog, id));
}

thread_local! {
    /// Shows the sync and push state on the open account page; set while Preferences are open.
    static STATUS: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
    /// Shows the accounts in the open Preferences anew.
    static ACCOUNTS: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Accounts came, went or changed: the open Preferences show them.
pub fn accounts_changed() {
    if let Some(show) = ACCOUNTS.with_borrow(Clone::clone) {
        show();
    }
}

/// A sync started or ended, or push failed or works again: the open Preferences show it.
pub fn status_changed() {
    if let Some(show) = STATUS.with_borrow(Clone::clone) {
        show();
    }
}

/// The DAV Push app's page in the Nextcloud app store.
const DAV_PUSH_APP: &str = "https://apps.nextcloud.com/apps/dav_push";

/// One row of "Server Status": its icon (none, "success", "info", "warning" or "error") and
/// subtitle.
fn set_status(row: &adw::ActionRow, icon: &gtk::Image, look: Option<&str>, text: &str) {
    row.set_subtitle(text);
    icon.set_visible(look.is_some());
    for class in ["success", "warning", "error", "dimmed"] {
        icon.remove_css_class(class);
    }
    if let Some(look) = look {
        icon.set_icon_name(Some(match look {
            "success" => "object-select-symbolic",
            "error" => "dialog-error-symbolic",
            "warning" => "dialog-warning-symbolic",
            _ => "dialog-information-symbolic",
        }));
        icon.add_css_class(if look == "info" { "dimmed" } else { look });
    }
}

/// The "Server Status" group of account `id`: the server (asked each time the returned closure
/// runs), the last sync and DAV Push, the latter two kept up to date while the page is open.
/// Google's has only the last sync.
fn connect_status(b: &gtk::Builder, id: i64, google: bool) -> Rc<dyn Fn()> {
    let (server, server_icon): (adw::ActionRow, gtk::Image) =
        (get(b, "server_row"), get(b, "server_icon"));
    let (sync, sync_icon): (adw::ActionRow, gtk::Image) = (get(b, "sync_row"), get(b, "sync_icon"));
    let (push, push_icon): (adw::ActionRow, gtk::Image) = (get(b, "push_row"), get(b, "push_icon"));
    server.set_visible(!google);
    push.set_visible(!google);
    let get_push: gtk::Button = get(b, "get_push");
    get_push.connect_clicked(|button| {
        let window = button.root().and_downcast::<gtk::Window>();
        gtk::UriLauncher::new(DAV_PUSH_APP).launch(window.as_ref(), gio::Cancellable::NONE, |_| {});
    });

    let show = Rc::new(move || {
        // The last sync, as the sidebar dates say it ("Today 14:32").
        let now = core().local_now();
        let last = core().last_sync(id).map(|t| {
            let local = t + now - todav_core::now();
            gettext("Last synced: {time}")
                .replace("{time}", &crate::dates::label(local, false, now))
        });
        let state = crate::sync_state();
        match crate::service().sync_error(id) {
            _ if state == "syncing" => set_status(&sync, &sync_icon, None, &gettext("Syncing…")),
            Some(e) if todav_app::accounts::no_access(&e) => set_status(
                &sync,
                &sync_icon,
                Some("error"),
                &crate::no_calendar_access(),
            ),
            Some(e) => set_status(
                &sync,
                &sync_icon,
                Some("error"),
                &gettext("Failed: {error}").replace("{error}", &e),
            ),
            None => match &last {
                Some(l) => set_status(&sync, &sync_icon, Some("success"), l),
                None => set_status(&sync, &sync_icon, None, &gettext("Not synced yet")),
            },
        }

        let p = core().push_status(id);
        let ntfy = !core()
            .setting("ntfy_url".into())
            .unwrap_or_default()
            .is_empty();
        get_push.set_visible(p.calendars > 0 && p.supported == 0);
        let (look, text) = if p.calendars == 0 {
            (None, gettext("Checked at the next sync"))
        } else if p.supported == 0 {
            (
                Some("info"),
                gettext(
                    "Not available on this server, so changes made elsewhere show up at the next \
                     check (every minute while the window is focused). Install the DAV Push app \
                     on Nextcloud and enter an ntfy server below to see them within seconds",
                ),
            )
        } else if !ntfy {
            (
                Some("info"),
                gettext(
                    "Available on this server. Enter an ntfy server below to see changes made \
                     elsewhere within seconds",
                ),
            )
        } else if let Some(e) = crate::service().push_error() {
            (
                Some("error"),
                gettext("Not working: {error}").replace("{error}", &e),
            )
        } else if p.registered == 0 {
            (None, gettext("Registering…"))
        } else {
            (
                Some("success"),
                gettext("On: changes made elsewhere show up within seconds"),
            )
        };
        set_status(&push, &push_icon, look, &text);
    });
    STATUS.set(Some(show.clone()));

    Rc::new(move || {
        show();
        if google {
            return;
        }
        set_status(&server, &server_icon, None, &gettext("Checking…"));
        let (server, icon) = (server.downgrade(), server_icon.downgrade());
        glib::spawn_future_local(async move {
            let r = gio::spawn_blocking(move || core().server_info(id)).await;
            let (Some(server), Some(icon)) = (server.upgrade(), icon.upgrade()) else {
                return;
            };
            match r {
                Ok(Ok(info)) => {
                    let name = format!("{} {}", info.product, info.version);
                    if info.maintenance {
                        let text =
                            gettext("{server}, in maintenance mode").replace("{server}", &name);
                        set_status(&server, &icon, Some("warning"), &text);
                    } else {
                        set_status(&server, &icon, Some("success"), &name);
                    }
                }
                Ok(Err(e)) => set_status(
                    &server,
                    &icon,
                    Some("error"),
                    &gettext("Cannot reach the server: {error}").replace("{error}", &e.to_string()),
                ),
                Err(_) => {}
            }
        });
    })
}

/// The "Accounts" group: a row per account opening its page, and Add; with the settings sync
/// choice, shown anew when accounts change.
fn connect_accounts(b: &gtk::Builder, dialog: &adw::PreferencesDialog) {
    let group: adw::PreferencesGroup = get(b, "accounts");
    let rows: RefCell<Vec<adw::ActionRow>> = RefCell::default();
    let settings_sync = connect_settings_sync(b);
    let d = dialog.downgrade();
    let show: Rc<dyn Fn()> = Rc::new(move || {
        for r in rows.borrow_mut().drain(..) {
            group.remove(&r);
        }
        let accounts = core().accounts();
        for a in &accounts {
            let row = adw::ActionRow::builder()
                .title(crate::account_name(a))
                .subtitle(account_state(a))
                .activatable(true)
                .build();
            if accounts.len() > 1 {
                connect_reorder(&row, a.id, &crate::account_name(a));
            }
            row.add_prefix(&gtk::Image::from_icon_name(account_icon(a)));
            if a.enabled && a.kind != AccountKind::Local && !core().connected(a.id) {
                let warning = gtk::Image::from_icon_name("dialog-warning-symbolic");
                warning.add_css_class("warning");
                row.add_suffix(&warning);
            }
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            let (d, id) = (d.clone(), a.id);
            row.connect_activated(move |_| {
                if let Some(d) = d.upgrade() {
                    d.push_subpage(&account_page(&d, id));
                }
            });
            group.add(&row);
            rows.borrow_mut().push(row);
        }
        settings_sync();
    });
    show();
    ACCOUNTS.set(Some(show));
    dialog.connect_closed(|_| {
        ACCOUNTS.set(None);
        STATUS.set(None);
    });
    let d = dialog.clone();
    get::<gtk::Button>(b, "add_account").connect_clicked(move |_| {
        let d2 = d.clone();
        crate::add_account::dialog(&d, move |id| {
            accounts_changed();
            crate::window::account_changed();
            crate::calendar::refresh();
            if let Some(a) = core().account(id) {
                let name = crate::account_name(&a);
                d2.add_toast(note(&gettext("{name} added").replace("{name}", &name)));
            }
        });
    });
}

/// Account rows move by their handle: a dragged row goes above or below the one it is dropped
/// on (its upper or lower half), and everything that lists accounts follows the new order.
fn connect_reorder(row: &adw::ActionRow, id: i64, title: &str) {
    let handle = gtk::Image::from_icon_name("list-drag-handle-symbolic");
    handle.set_tooltip_text(Some(&gettext("Drag to Move")));
    handle.set_cursor_from_name(Some("grab"));
    let drag = gtk::DragSource::new();
    drag.set_actions(gtk::gdk::DragAction::MOVE);
    drag.set_content(Some(&gtk::gdk::ContentProvider::for_value(&id.to_value())));
    let (r, title) = (row.clone(), title.to_string());
    drag.connect_drag_begin(move |_, drag| {
        // The account's name on a card; the row stays in place, faded.
        let card = gtk::Label::new(Some(&title));
        card.add_css_class("drag-card");
        if let Ok(icon) = gtk::DragIcon::for_drag(drag).downcast::<gtk::DragIcon>() {
            icon.set_child(Some(&card));
        }
        r.add_css_class("dragged");
    });
    let r = row.clone();
    drag.connect_drag_end(move |_, _, _| {
        r.remove_css_class("dragged");
        crate::lists::clear_mark();
    });
    handle.add_controller(drag);
    row.add_prefix(&handle);

    let below =
        |dt: &gtk::DropTarget, y: f64| dt.widget().is_some_and(|w| y > f64::from(w.height()) / 2.0);
    let drop = gtk::DropTarget::new(i64::static_type(), gtk::gdk::DragAction::MOVE);
    drop.connect_motion(move |dt, _, y| {
        if let Some(w) = dt.widget() {
            crate::lists::mark(&w, below(dt, y));
        }
        gtk::gdk::DragAction::MOVE
    });
    drop.connect_leave(|dt| {
        if let Some(w) = dt.widget() {
            crate::lists::unmark(&w);
        }
    });
    drop.connect_drop(move |dt, value, _, y| {
        if let Some(w) = dt.widget() {
            crate::lists::unmark(&w);
        }
        let Ok(moved) = value.get::<i64>() else {
            return false;
        };
        if moved == id {
            return false;
        }
        let mut ids: Vec<i64> = core().accounts().iter().map(|a| a.id).collect();
        ids.retain(|i| *i != moved);
        let at = ids.iter().position(|i| *i == id).unwrap_or(ids.len());
        ids.insert(at + usize::from(below(dt, y)), moved);
        let _ = core().reorder_accounts(ids);
        // After the drop is done: the rows are made anew.
        glib::idle_add_local_once(|| {
            accounts_changed();
            crate::window::lists_changed();
            crate::calendar::refresh();
        });
        true
    });
    row.add_controller(drop);
}

fn account_icon(a: &Account) -> &'static str {
    match a.kind {
        AccountKind::Local | AccountKind::System => "computer-symbolic",
        AccountKind::Nextcloud if a.source.is_empty() => "network-server-symbolic",
        AccountKind::Nextcloud | AccountKind::Google => "goa-panel-symbolic",
    }
}

/// An account row's subtitle: what it is, or why it does not sync.
fn account_state(a: &Account) -> String {
    match a.kind {
        _ if !a.enabled => pgettext("account", "Off"),
        AccountKind::Local => gettext("Lists and calendars that stay on this computer"),
        AccountKind::System => gettext("Calendars of the accounts on this device"),
        _ if !core().connected(a.id) => gettext("Not connected"),
        AccountKind::Google => gettext("Google Calendar"),
        AccountKind::Nextcloud if !a.source.is_empty() => gettext("GNOME Online Accounts"),
        AccountKind::Nextcloud => gettext("Nextcloud server"),
    }
}

/// "Sync settings with": off, or a Nextcloud account; the returned closure fills it anew.
fn connect_settings_sync(b: &gtk::Builder) -> impl Fn() + 'static {
    let row: adw::ComboRow = get(b, "settings_account");
    let names: gtk::StringList = get(b, "settings_accounts");
    let ntfy: adw::SwitchRow = get(b, "sync_ntfy");
    let ids: Rc<RefCell<Vec<Option<i64>>>> = Rc::default();
    let filling = Rc::new(Cell::new(false));
    let (i, f, n) = (ids.clone(), filling.clone(), ntfy.clone());
    row.connect_selected_notify(move |r| {
        if f.get() {
            return;
        }
        let id = i.borrow().get(r.selected() as usize).copied().flatten();
        let _ = core().set_settings_account(id);
        n.set_sensitive(id.is_some());
        crate::sync_now(); // pulls that account's settings right away
    });
    move || {
        filling.set(true);
        let mut v = vec![None];
        let mut labels = vec![pgettext("settings sync", "Off")];
        for a in core().accounts() {
            if a.kind == AccountKind::Nextcloud {
                v.push(Some(a.id));
                labels.push(crate::account_name(&a));
            }
        }
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        names.splice(0, names.n_items(), &labels);
        let current = core().settings_account();
        row.set_selected(v.iter().position(|i| *i == current).unwrap_or(0) as u32);
        ntfy.set_sensitive(current.is_some());
        *ids.borrow_mut() = v;
        filling.set(false);
    }
}

/// Account `id`'s page: switched on, its name; for a Nextcloud account how it is connected
/// (signed in here: sign in again, the server's address) and the server status; Remove.
fn account_page(dialog: &adw::PreferencesDialog, id: i64) -> adw::NavigationPage {
    let b = view!("account");
    let page: adw::NavigationPage = get(&b, "page");
    let Some(a) = core().account(id) else {
        return page;
    };
    page.set_title(&crate::account_name(&a));
    let remote = a.kind != AccountKind::Local;
    let google = a.kind == AccountKind::Google;
    let manual = a.kind == AccountKind::Nextcloud && a.source.is_empty();
    get::<gtk::Widget>(&b, "connection").set_visible(remote);
    get::<gtk::Widget>(&b, "google_note").set_visible(google);

    let enabled: adw::SwitchRow = get(&b, "enabled");
    enabled.set_active(a.enabled);
    enabled.connect_active_notify(move |r| {
        let on = r.is_active();
        glib::spawn_future_local(async move {
            crate::enable_account(id, on).await;
            accounts_changed();
            status_changed();
        });
    });

    // Shown with the name it has without one; applying that name gives that back.
    let name: adw::EntryRow = get(&b, "name");
    name.set_text(&crate::account_name(&a));
    let p = page.clone();
    name.connect_apply(move |e| {
        let Some(mut a) = core().account(id) else {
            return;
        };
        a.name = String::new();
        let text = e.text().trim().to_string();
        let given = if text == crate::account_name(&a) {
            String::new()
        } else {
            text
        };
        let _ = core().rename_account(id, given);
        if let Some(a) = core().account(id) {
            p.set_title(&crate::account_name(&a));
            e.set_text(&crate::account_name(&a));
        }
        accounts_changed();
        crate::window::lists_changed();
        crate::calendar::refresh();
    });

    let status: adw::ActionRow = get(&b, "status");
    let icon: gtk::Image = get(&b, "status_icon");
    let sign_in: adw::SplitButton = get(&b, "sign_in");
    let cancel: gtk::Button = get(&b, "cancel_sign_in");
    let server: UrlRow = (get(&b, "server_scheme"), get(&b, "server"));
    let server_status: adw::PreferencesGroup = get(&b, "server_status");
    server_status.set_visible(false); // a synced account's, while connected (`describe`)
    set_url(&server, &a.url);
    server.1.set_visible(manual);
    sign_in.set_visible(manual);
    let check_server: Rc<dyn Fn()> = if remote {
        connect_status(&b, id, google)
    } else {
        Rc::new(|| {})
    };
    let describe: Rc<dyn Fn()> = {
        let (status, icon) = (status.clone(), icon.clone());
        Rc::new(move || {
            let Some(a) = core().account(id) else {
                return;
            };
            let connected = core().connected(id);
            let host = match a.kind {
                AccountKind::Google => "Google",
                _ => a.url.split_once("://").map_or(a.url.as_str(), |(_, h)| h),
            };
            if !connected {
                status.set_title(&gettext("Not connected"));
                status.set_subtitle(&if manual {
                    gettext("Sign in again to sync it")
                } else {
                    gettext("Check the account in Settings → Online Accounts")
                });
            } else if manual {
                status.set_title(&gettext("Signed in as {name}").replace("{name}", &a.user));
                status.set_subtitle(host);
            } else {
                status.set_title(&gettext("Connected as {name}").replace("{name}", &a.user));
                status.set_subtitle(&format!("{host} · {}", gettext("GNOME Online Accounts")));
            }
            icon.set_icon_name(Some(if connected {
                "object-select-symbolic"
            } else {
                "dialog-warning-symbolic"
            }));
            for (class, on) in [("success", connected), ("warning", !connected)] {
                if on {
                    icon.add_css_class(class);
                } else {
                    icon.remove_css_class(class);
                }
            }
            server_status.set_visible(connected && a.enabled);
            if connected && a.enabled {
                check_server();
            }
        })
    };
    if remote {
        describe();
    }

    // A new app password for the account (the same user on its server).
    let reconnect = {
        let (d, describe) = (dialog.downgrade(), describe.clone());
        move |c: todav_core::Credentials| {
            let (d, describe) = (d.clone(), describe.clone());
            glib::spawn_future_local(async move {
                let Some(a) = core().account(id) else { return };
                let msg = if c.login_name != a.user {
                    gettext("Signed in as {name}: add that as another account")
                        .replace("{name}", &c.login_name)
                } else {
                    match crate::reconnect(id, a.url.clone(), c.app_password).await {
                        Ok(()) => gettext("Signed in as {name}").replace("{name}", &a.user),
                        Err(e) => e,
                    }
                };
                describe();
                accounts_changed();
                crate::window::account_changed();
                if let Some(d) = d.upgrade() {
                    d.add_toast(note(&msg));
                }
            });
        }
    };
    let reset = {
        let (sign_in, cancel) = (sign_in.clone(), cancel.clone());
        let label = sign_in.label();
        move || {
            sign_in.set_sensitive(true);
            sign_in.set_label(label.as_deref().unwrap_or_default());
            cancel.set_visible(false);
        }
    };
    let r = reset.clone();
    cancel.connect_clicked(move |_| {
        crate::window::cancel_login();
        r();
    });
    let start = {
        let (sign_in, cancel, d, reconnect) = (
            sign_in.clone(),
            cancel.clone(),
            dialog.downgrade(),
            reconnect.clone(),
        );
        move |copy: bool| {
            sign_in.set_sensitive(false);
            sign_in.set_label(&if copy {
                gettext("Waiting for Sign-In…")
            } else {
                gettext("Waiting for Browser…")
            });
            cancel.set_visible(true);
            let (d, reset, reconnect) = (d.clone(), reset.clone(), reconnect.clone());
            let url = core().account(id).map(|a| a.url).unwrap_or_default();
            glib::spawn_future_local(async move {
                match crate::window::browser_login(url, copy).await {
                    Ok(Some(c)) => reconnect(c),
                    Ok(None) => return, // cancelled: Cancel or closing the dialog reset it
                    Err(e) => {
                        if let Some(d) = d.upgrade() {
                            d.add_toast(note(&e));
                        }
                    }
                }
                reset();
            });
        }
    };
    let actions = gio::SimpleActionGroup::new();
    let copy = gio::SimpleAction::new("sign-in-copy", None);
    let s = start.clone();
    copy.connect_activate(move |_, _| s(true));
    actions.add_action(&copy);
    // Or with the app password typed in.
    let password = gio::SimpleAction::new("sign-in-password", None);
    let d = dialog.downgrade();
    password.connect_activate(move |_, _| {
        let (Some(d), Some(a)) = (d.upgrade(), core().account(id)) else {
            return;
        };
        crate::window::password_login(&d, a.url, Some(a.user), reconnect.clone());
    });
    actions.add_action(&password);
    page.insert_action_group("account", Some(&actions));
    sign_in.connect_clicked(move |_| start(false));

    // Another address: the same account on a moved server.
    let (d, describe_, row) = (dialog.downgrade(), describe.clone(), server.1.clone());
    row.connect_apply(move |_| {
        let Some(a) = core().account(id) else { return };
        let url = url_text(&server);
        if url.is_empty() || url == a.url {
            return;
        }
        let (d, describe) = (d.clone(), describe_.clone());
        glib::spawn_future_local(async move {
            let msg = match crate::password_load(&a.url, &a.user) {
                None => gettext("No saved password: sign in again first"),
                Some(pw) => match crate::reconnect(id, url, pw).await {
                    Ok(()) => gettext("Nextcloud server changed"),
                    Err(e) => e,
                },
            };
            describe();
            accounts_changed();
            crate::window::account_changed();
            crate::calendar::refresh();
            if let Some(d) = d.upgrade() {
                d.add_toast(note(&msg));
            }
        });
    });

    let d = dialog.downgrade();
    get::<gtk::Button>(&b, "remove").connect_clicked(move |_| {
        if let Some(d) = d.upgrade() {
            glib::spawn_future_local(remove_account(d, id));
        }
    });
    page
}

/// Remove account `id` after asking; when its changes cannot be uploaded first, asks again
/// whether to lose them.
async fn remove_account(dialog: adw::PreferencesDialog, id: i64) {
    let Some(a) = core().account(id) else { return };
    let name = crate::account_name(&a);
    let body = match a.kind {
        AccountKind::Local => gettext(
            "Its lists and calendars, with everything in them, are deleted from this device. \
             This cannot be undone.",
        ),
        AccountKind::Nextcloud | AccountKind::Google => gettext(
            "Its lists and calendars leave this device; they stay on the server. Switch it \
             off instead to only hide them.",
        ),
        AccountKind::System => gettext(
            "Its calendars leave Todav; they stay on this device, in the accounts they come from.",
        ),
    };
    let ask = |heading: String, body: String, yes: String| {
        let alert = adw::AlertDialog::new(Some(&heading), Some(&body));
        alert.add_responses(&[("cancel", &gettext("Cancel")), ("remove", &yes)]);
        alert.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        alert.set_default_response(Some("cancel"));
        alert.set_close_response("cancel");
        alert
    };
    let alert = ask(
        gettext("Remove {name}?").replace("{name}", &name),
        body,
        gettext("Remove"),
    );
    if alert.choose_future(Some(&dialog)).await != "remove" {
        return;
    }
    let mut r = crate::remove_account(id, false).await;
    if let Err(e) = &r {
        let alert = ask(
            gettext("Remove Without Uploading?"),
            gettext("{error}. Changes made here that the server does not have yet will be lost.")
                .replace("{error}", e),
            gettext("Remove Anyway"),
        );
        if alert.choose_future(Some(&dialog)).await != "remove" {
            return;
        }
        r = crate::remove_account(id, true).await;
    }
    match r {
        Ok(()) if core().accounts().is_empty() => {
            dialog.close();
        }
        Ok(()) => {
            dialog.pop_subpage();
            accounts_changed();
            dialog.add_toast(note(&gettext("{name} removed").replace("{name}", &name)));
        }
        Err(e) => dialog.add_toast(note(&e)),
    }
}

/// Fill a scheme picker and host entry from a full URL.
fn set_url((scheme, row): &UrlRow, url: &str) {
    match url.strip_prefix("http://") {
        Some(host) => {
            scheme.set_selected(1);
            row.set_text(host);
        }
        None => row.set_text(url.strip_prefix("https://").unwrap_or(url)),
    }
}

/// Run `check` on the entered URL off the main thread and report the result as a toast.
fn connect_test(
    dialog: &adw::PreferencesDialog,
    button: &gtk::Button,
    row: &UrlRow,
    check: impl Fn(String) -> Result<(), todav_core::Error> + Clone + Send + 'static,
) {
    let (dialog, row) = (dialog.clone(), row.clone());
    button.connect_clicked(move |button| {
        let url = url_text(&row);
        if url.is_empty() {
            return;
        }
        button.set_sensitive(false);
        let (dialog, button, check) = (dialog.clone(), button.clone(), check.clone());
        glib::spawn_future_local(async move {
            let msg = match gio::spawn_blocking(move || check(url)).await {
                Ok(Ok(())) => gettext("Connection works"),
                Ok(Err(e)) => gettext("Test failed: {error}").replace("{error}", &e.to_string()),
                Err(_) => gettext("Test failed"),
            };
            button.set_sensitive(true);
            dialog.add_toast(crate::window::note(&msg));
        });
    });
}
