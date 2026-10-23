//! Preferences dialog: General (style, what opens at start), Accounts (account, server
//! addresses with connection tests, applied on close; sync; local lists), Tasks (order and
//! display, applied at once), Calendar and Extension.

use crate::connection::Connection;
use crate::i18n::{gettext, gettext_noop};
use crate::window::{get, toast, url_text, view};
use crate::{core, request_sync};
use adw::prelude::*;
use gtk::{gio, glib};
use std::rc::Rc;
use todav_core::Sort;

type UrlRow = (gtk::DropDown, adw::EntryRow);

/// Setting values for the "Sort" rows, in row order.
const SORTS: [(&str, Sort); 5] = [
    ("manual", Sort::Manual),
    ("newest", Sort::Newest),
    ("oldest", Sort::Oldest),
    ("due_soonest", Sort::DueSoonest),
    ("due_latest", Sort::DueLatest),
];

fn sort_index(key: &str) -> u32 {
    let v = core().setting(key.into()).unwrap_or_default();
    SORTS.iter().position(|(s, _)| *s == v).unwrap_or(0) as u32
}

/// Where tasks show up and where new ones go; date sorting decides the latter.
pub struct Order {
    pub tasks: Sort,
    pub subtasks: Sort,
    pub task_start: bool,
    pub sub_start: bool,
}

/// How completed tasks are shown, from the "Completed tasks" / "Completed subtasks" rows.
pub fn done_view() -> todav_core::DoneView {
    let tasks = core().setting("done_tasks".into()).unwrap_or_default();
    let subtasks = core().setting("done_subtasks".into()).unwrap_or_default();
    todav_core::DoneView {
        in_place: tasks == "in_place" || tasks == "last",
        tasks_last: tasks == "last",
        hide_subtasks: subtasks == "hide",
        subtasks_last: subtasks == "last",
    }
}

/// Light or dark as the device-only `color_scheme` says: "light", "dark", else the system's.
pub fn apply_color_scheme() {
    let scheme = match core().setting("color_scheme".into()).as_deref() {
        Some("light") => adw::ColorScheme::ForceLight,
        Some("dark") => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    };
    adw::StyleManager::default().set_color_scheme(scheme);
}

/// Whether the sidebar month marks days with dots (`mini_dots`; on unless turned off).
pub fn mini_dots() -> bool {
    core().setting("mini_dots".into()).as_deref() != Some("false")
}

/// Whether a double click on a calendar item edits it (`double_click_edit`; on unless turned
/// off). Off, a click shows its details at once, not after the double-click time.
pub fn double_click_edit() -> bool {
    core().setting("double_click_edit".into()).as_deref() != Some("false")
}

/// The GNOME extension's options (Preferences → Extension, synced like the rest) with their
/// defaults; it reads them over D-Bus. `ext_list` is the list it shows (empty: the first).
pub const EXTENSION: [(&str, &str); 10] = [
    ("ext_columns", "double"),
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

/// Lengths offered for `duration`, in minutes, as the rows list them.
const DURATIONS: [i64; 7] = [15, 30, 45, 60, 90, 120, 180];

/// How long, in seconds, a task's or event's time lasts when one is added ("task_duration" or
/// "event_duration", minutes; an hour by default).
pub fn duration(key: &str) -> i64 {
    core()
        .setting(key.into())
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|m| *m > 0)
        .unwrap_or(60)
        * 60
}

fn connect_duration(b: &gtk::Builder, key: &'static str) {
    let row: adw::ComboRow = get(b, key);
    let current = duration(key) / 60;
    let hour = DURATIONS.iter().position(|m| *m == 60).unwrap_or(0);
    row.set_selected(DURATIONS.iter().position(|m| *m == current).unwrap_or(hour) as u32);
    row.connect_selected_notify(move |r| {
        let _ = core().set_setting(key.into(), DURATIONS[r.selected() as usize].to_string());
    });
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

pub fn order() -> Order {
    let sort = |key| SORTS[sort_index(key) as usize].1;
    let start = |sort, key: &str| match sort {
        Sort::Newest => true,
        Sort::Oldest => false,
        // New tasks have no due date: they go among the undated ones, as in manual order.
        _ => core().setting(key.into()).as_deref() == Some("start"),
    };
    let (tasks, subtasks) = (sort("task_sort"), sort("sub_sort"));
    Order {
        tasks,
        subtasks,
        task_start: start(tasks, "task_new"),
        sub_start: start(subtasks, "sub_new"),
    }
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
    sort.set_selected(sort_index(&sort_key));
    show();
    sort.connect_selected_notify(move |s| {
        let _ = core().set_setting(sort_key.clone(), SORTS[s.selected() as usize].0.into());
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
    for (label, _) in crate::dates::TIMED {
        model.append(&gettext(label));
    }
    let current = crate::dates::default_reminder(false);
    let pos = crate::dates::TIMED.iter().position(|(_, o)| *o == current);
    default.set_selected(pos.unwrap_or(0) as u32);
    default.connect_selected_notify(|r| {
        let offset = crate::dates::TIMED[r.selected() as usize].1;
        let _ = core().set_setting("reminder_default".into(), offset.to_string());
    });

    let (hour, minute): (gtk::SpinButton, gtk::SpinButton) =
        (get(b, "reminder_hour"), get(b, "reminder_minute"));
    let t = crate::dates::all_day_time();
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

pub fn present(parent: &impl IsA<gtk::Widget>) {
    let b = view!("settings");
    let dialog: adw::PreferencesDialog = get(&b, "dialog");
    let ntfy: UrlRow = (get(&b, "ntfy_scheme"), get(&b, "ntfy"));
    let conn = connect_account(&b, &dialog);
    let server = conn.server.clone();

    let acc = core().account();
    let old_server = acc.as_ref().map(|a| a.url.clone()).unwrap_or_default();
    let user = acc.map(|a| a.user).unwrap_or_default();
    let manual = crate::goa_id().is_none();
    let password = crate::password_load(&old_server, &user).unwrap_or_default();
    let old_ntfy = core().setting("ntfy_url".into()).unwrap_or_default();
    for key in ["sync_settings", "sync_ntfy"] {
        let sync: adw::SwitchRow = get(&b, key);
        sync.set_active(core().setting(key.into()).as_deref() != Some("false"));
        sync.connect_active_notify(move |s| {
            let _ = core().set_setting(key.into(), s.is_active().to_string());
            request_sync(); // turning it on pulls the server's settings right away
        });
    }
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
        &["last", "month", "week", "agenda"],
    );
    connect_choice(&b, "category_view", &["group", "tags", "none"]);
    connect_choice(&b, "done_tasks", &["group", "in_place", "last"]);
    connect_choice(&b, "done_subtasks", &["show", "hide", "last"]);
    connect_duration(&b, "task_duration");
    connect_duration(&b, "event_duration");
    let dots: adw::SwitchRow = get(&b, "mini_dots");
    dots.set_active(mini_dots());
    dots.connect_active_notify(|d| {
        let _ = core().set_setting("mini_dots".into(), d.is_active().to_string());
        crate::calendar::refresh();
    });
    let double: adw::SwitchRow = get(&b, "double_click_edit");
    double.set_active(double_click_edit());
    double.connect_active_notify(|d| {
        let _ = core().set_setting("double_click_edit".into(), d.is_active().to_string());
    });
    // The extension: its switches carry the setting's name as their id.
    connect_choice(&b, "ext_columns", &["double", "single"]);
    for (key, _) in EXTENSION.iter().filter(|(_, d)| *d == "true") {
        let row: adw::SwitchRow = get(&b, key);
        row.set_active(core().setting((*key).into()).as_deref() != Some("false"));
        row.connect_active_notify(move |r| {
            let _ = core().set_setting((*key).into(), r.is_active().to_string());
        });
    }
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
    connect_local(&b, &dialog);
    set_url(&server, &old_server);
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
        // Still signed in here and the address edited: same account, moved server.
        let new_server = url_text(&server);
        let still_manual = manual && crate::goa_id().is_none() && !conn.goa.is_active();
        if still_manual && !new_server.is_empty() && new_server != old_server {
            move_account(
                old_server.clone(),
                user.clone(),
                password.clone(),
                new_server,
            );
        }
    });
    dialog.present(Some(parent));
}

/// The "Account" group: a status row, then the shared GOA / server rows, applied at once.
/// Turning GOA on (or picking another account) connects through it; turning it off goes back
/// to the last browser sign-in, or disconnects without one. Signing in with the browser
/// replaces the connection (or renews that sign-in).
fn connect_account(b: &gtk::Builder, dialog: &adw::PreferencesDialog) -> Rc<Connection> {
    let goa = crate::goa_id();
    let conn = Connection::new(goa.is_some(), goa.as_deref());
    conn.attach(&get(b, "account"));
    let status: adw::ActionRow = get(b, "status");
    let icon: gtk::Image = get(b, "status_icon");
    let sign_in: adw::SplitButton = get(b, "sign_in");
    let sign_out: adw::SplitButton = get(b, "sign_out");
    // Push and sync are about the account: only with one (local mode runs without).
    let server_groups: [adw::PreferencesGroup; 2] = [get(b, "push"), get(b, "sync")];
    // Everything here shows the actual connection, the switch included.
    let describe = {
        let (conn, sign_in, sign_out) = (conn.clone(), sign_in.clone(), sign_out.clone());
        move || {
            let goa = crate::goa_id().is_some();
            let acc = core().account();
            let host = acc.as_ref().map_or(String::new(), |a| {
                a.url
                    .split_once("://")
                    .map_or(a.url.clone(), |(_, h)| h.to_string())
            });
            match &acc {
                Some(a) if goa => {
                    status.set_title(&gettext("Connected as {name}").replace("{name}", &a.user));
                    status.set_subtitle(&format!("{host} · {}", gettext("GNOME Online Accounts")));
                }
                Some(a) => {
                    status.set_title(&gettext("Signed in as {name}").replace("{name}", &a.user));
                    status.set_subtitle(&host);
                }
                None => {
                    status.set_title(&gettext("Not signed in"));
                    status.set_subtitle("");
                }
            }
            icon.set_visible(acc.is_some());
            for g in &server_groups {
                g.set_sensitive(acc.is_some());
            }
            sign_out.set_visible(acc.is_some() && !goa);
            sign_in.set_visible(acc.is_none());
            conn.goa.set_active(goa); // its handler sees nothing to change
            conn.goa.set_sensitive(conn.has_accounts());
        }
    };
    describe();

    let (d, c, show) = (dialog.clone(), conn.clone(), describe.clone());
    conn.connect_changed(move || {
        let current = crate::goa_id();
        let chosen = c.chosen();
        if chosen.as_ref().map(|a| &a.id) == current.as_ref() {
            return; // already so
        }
        c.goa.set_sensitive(false);
        let (d, show) = (d.clone(), show.clone());
        glib::spawn_future_local(async move {
            let r = match chosen {
                Some(a) => {
                    let name = a.name.clone();
                    crate::switch_account(crate::Target::Goa(a))
                        .await
                        .map(|()| gettext("Connected as {name}").replace("{name}", &name))
                }
                None => match crate::remembered_sign_in() {
                    Some(c) => {
                        let who = c.login_name.clone();
                        crate::switch_account(crate::Target::Manual(c))
                            .await
                            .map(|()| gettext("Signed in as {name}").replace("{name}", &who))
                    }
                    None => crate::disconnect(false)
                        .await
                        .map(|()| gettext("Disconnected")),
                },
            };
            if r.is_ok() && core().account().is_some() {
                crate::window::account_changed();
            }
            show(); // also puts the switch back when that failed
            d.add_toast(crate::window::note(&r.unwrap_or_else(|e| e)));
        });
    });

    // Back from "Waiting for …" once a sign-in ends or is cancelled.
    let cancel: gtk::Button = get(b, "cancel_sign_in");
    let reset = {
        let (sign_in, sign_out, cancel) = (sign_in.clone(), sign_out.clone(), cancel.clone());
        let labels = (sign_in.label(), sign_out.label());
        let describe = describe.clone();
        move || {
            for (button, label) in [(&sign_in, &labels.0), (&sign_out, &labels.1)] {
                button.set_sensitive(true);
                button.set_label(label.as_deref().unwrap_or_default());
            }
            cancel.set_visible(false);
            describe();
        }
    };
    let r = reset.clone();
    cancel.connect_clicked(move |_| {
        crate::window::cancel_login();
        r();
    });

    // Sign in with the browser (or by a copied link): renews the account signed in to here, or
    // replaces the GOA one.
    let start = {
        let (d, c, sign_in, sign_out) = (
            dialog.clone(),
            conn.clone(),
            sign_in.clone(),
            sign_out.clone(),
        );
        move |copy: bool| {
            let server = url_text(&c.server);
            if server.is_empty() {
                d.add_toast(crate::window::note(&crate::window::no_server()));
                return;
            }
            let button = if sign_in.is_visible() {
                &sign_in
            } else {
                &sign_out
            };
            button.set_sensitive(false);
            button.set_label(&if copy {
                gettext("Waiting for Sign-In…")
            } else {
                gettext("Waiting for Browser…")
            });
            cancel.set_visible(true);
            let (d, reset) = (d.clone(), reset.clone());
            glib::spawn_future_local(async move {
                let msg = match crate::window::browser_login(server, copy).await {
                    Ok(Some(c)) => use_sign_in(c).await,
                    Ok(None) => return, // cancelled: Cancel or closing the dialog reset it
                    Err(e) => e,
                };
                reset();
                d.add_toast(crate::window::note(&msg));
            });
        }
    };
    let actions = gio::SimpleActionGroup::new();
    for (name, copy) in [("sign-in", false), ("sign-in-copy", true)] {
        let action = gio::SimpleAction::new(name, None);
        let start = start.clone();
        action.connect_activate(move |_, _| start(copy));
        actions.add_action(&action);
    }
    // Or with a login name and app password typed in.
    let password = gio::SimpleAction::new("sign-in-password", None);
    let (d, c, describe_) = (dialog.clone(), conn.clone(), describe.clone());
    password.connect_activate(move |_, _| {
        let server = url_text(&c.server);
        if server.is_empty() {
            d.add_toast(crate::window::note(&crate::window::no_server()));
            return;
        }
        let (dd, show) = (d.clone(), describe_.clone());
        crate::window::password_login(&d, server, move |c| {
            let (d, show) = (dd.clone(), show.clone());
            glib::spawn_future_local(async move {
                let msg = use_sign_in(c).await;
                show();
                d.add_toast(crate::window::note(&msg));
            });
        });
    });
    actions.add_action(&password);
    dialog.insert_action_group("account", Some(&actions));
    sign_in.connect_clicked(move |_| start(false));
    // Only offered for a browser sign-in; a GOA account is managed in GNOME Settings.
    // The welcome page at once; pending changes still go up first, in the background.
    let d = dialog.clone();
    sign_out.connect_clicked(move |_| {
        crate::forget_sign_in();
        d.close();
        crate::window::show_welcome();
        glib::spawn_future_local(async {
            if let Err(e) = crate::disconnect(true).await {
                crate::window::toast(&e);
            }
        });
    });
    // A sign-in still waiting for the browser stops with the dialog.
    dialog.connect_closed(|_| crate::window::cancel_login());
    conn
}

/// The "Local lists and calendars" switch. Off hides those lists, nothing is deleted: when
/// there are some it asks first; with no account it goes back to the welcome page.
fn connect_local(b: &gtk::Builder, dialog: &adw::PreferencesDialog) {
    let row: adw::SwitchRow = get(b, "local_mode");
    row.set_active(crate::local_mode());
    let d = dialog.clone();
    row.connect_active_notify(move |r| {
        let on = r.is_active();
        if on == crate::local_mode() {
            return; // put back after Cancel
        }
        let account = core().account().is_some();
        let mut all = core().lists();
        all.extend(core().calendars());
        let dialog = d.clone();
        let apply = move |on: bool| {
            let _ = core().set_setting("local_mode".into(), on.to_string());
            if account {
                crate::window::account_changed();
                crate::calendar::refresh();
            } else {
                dialog.close();
                crate::window::show_welcome();
            }
        };
        if on || (account && !all.iter().any(|l| l.local)) {
            return apply(on);
        }
        let (d, r) = (d.clone(), r.clone());
        glib::spawn_future_local(async move {
            let alert = adw::AlertDialog::new(
                Some(&gettext("Turn Off Local Lists?")),
                Some(&gettext(
                    "Local lists and calendars will be hidden until you turn them on again.",
                )),
            );
            alert.add_responses(&[
                ("cancel", &gettext("Cancel")),
                ("off", &gettext("Turn Off")),
            ]);
            alert.set_response_appearance("off", adw::ResponseAppearance::Destructive);
            alert.set_default_response(Some("cancel"));
            alert.set_close_response("cancel");
            if alert.choose_future(Some(&d)).await == "off" {
                apply(false);
            } else {
                r.set_active(true);
            }
        });
    });
}

/// Connect with credentials from a sign-in here; the message for a toast.
async fn use_sign_in(c: todav_core::Credentials) -> String {
    let who = c.login_name.clone();
    match crate::switch_account(crate::Target::Manual(c)).await {
        Ok(()) => {
            crate::window::account_changed();
            gettext("Signed in as {name}").replace("{name}", &who)
        }
        Err(e) => e,
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

/// Point the account at a new server address, keeping the user and app password.
fn move_account(old: String, user: String, password: String, new: String) {
    glib::spawn_future_local(async move {
        let r = gio::spawn_blocking(move || -> Result<(), String> {
            // Switching servers drops the local copy, so pending edits must reach the old one first.
            core().sync().map_err(|e| {
                gettext("Server not changed, cannot upload pending changes: {error}")
                    .replace("{error}", &e.to_string())
            })?;
            let _ = core().push_unregister();
            core()
                .set_account(new.clone(), user.clone(), password.clone())
                .map_err(|e| e.to_string())?;
            crate::password_store(&new, &user, &password).map_err(|e| {
                gettext("Cannot save password: {error}").replace("{error}", &e.to_string())
            })?;
            crate::password_clear(&old, &user);
            Ok(())
        })
        .await;
        match r {
            Ok(Ok(())) => {
                toast(&gettext("Nextcloud server changed"));
                request_sync();
                crate::restart_push(); // register push on the new server now, not within the hour
            }
            Ok(Err(e)) => toast(&e),
            Err(_) => toast(&gettext("Changing the server failed")),
        }
    });
}
