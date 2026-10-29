//! Main window: login page, list sidebar, grouped task page.

use crate::i18n::{gettext, gettext_noop, ngettext, pgettext};
use crate::{Event, core, dates, request_sync};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use todav_app::prefs;
use todav_app::tasks::{self, Page, Slot};
use todav_core::{AccountKind, CategoryGroup, Due, Task, TaskPatch};

/// Libadwaita only styles plain buttons as pills; round the ends of the sign-in split button to match.
/// The category section header turns its arrow like an expander row, without a "pressed" look.
/// Completed tasks shown among open ones are struck through (only their own title, not their
/// subtasks'); drop targets show a line where the task will land
/// instead of GTK's frame around the target, and "+" buttons taking the dragged task are outlined,
/// as is the sidebar category it is over. A "warning" banner is tinted amber (libadwaita's are
/// grey, from `--banner-color`). A task's star takes its priority's colour, as in Nextcloud Tasks;
/// a pin is accented while pinned.
const CSS: &str = "
menubutton.priority-high > button { color: @error_color; }
menubutton.priority-medium > button { color: @yellow_5; }
menubutton.priority-low > button { color: @blue_3; }
button.pinned { color: @accent_color; }
splitbutton.pill { border-radius: 9999px; }
splitbutton.pill > button { padding: 10px 20px 10px 32px; border-radius: 9999px 0 0 9999px; }
splitbutton.pill > menubutton > button { padding: 10px 16px 10px 12px; border-radius: 0 9999px 9999px 0; }
.cat-toggle image { transition: -gtk-icon-transform 200ms ease; }
.cat-toggle:not(:checked) image { -gtk-icon-transform: rotate(-90deg); }
.cat-toggle:checked:not(:hover) { background: none; }
row.task-done > box.header > box.title > label.title,
row.expander.task-done > box > list > row.header > box.header > box.title > label.title { text-decoration-line: line-through; opacity: 0.55; }
.tag { padding: 2px 10px; border-radius: 9999px; font-size: smaller; color: @accent_color; background: alpha(@accent_bg_color, 0.15); }
.tag.date, .tag.count { color: @window_fg_color; background: alpha(@window_fg_color, 0.08); }
button.tag.event { min-height: 0; padding: 2px 10px; font-weight: normal; }
.tag.overdue { color: @error_color; background: alpha(@error_bg_color, 0.15); }
:drop(active) { box-shadow: none; outline: none; }
button.drop-hint { outline: 2px solid @accent_bg_color; outline-offset: -2px; }
button.drop-hint:drop(active) { background: alpha(@accent_bg_color, 0.2); }
list:not(.reordering) > row.cat-drop:drop(active) { outline: 2px solid @accent_bg_color; outline-offset: -2px; background: alpha(@accent_bg_color, 0.2); }
row.dragged { opacity: 0.4; }
/* A sidebar row a list moves by: the gap under it (libadwaita's margin) as part of it, so a drop there lands. */
.navigation-sidebar > row.reorderable { margin-bottom: 0; border-bottom: 2px solid transparent; background-clip: padding-box; }
banner.warning { --banner-color: var(--warning-bg-color); }
.drag-card { margin: 8px; padding: 12px 18px; border-radius: 12px; background: @card_bg_color; color: @card_fg_color; box-shadow: 0 2px 8px alpha(black, 0.25); }
row.drop-above { box-shadow: inset 0 2px 0 0 @accent_bg_color; }
row.drop-below { box-shadow: inset 0 -2px 0 0 @accent_bg_color; }
@keyframes flash { from { background-color: transparent; } 50% { background-color: alpha(@accent_bg_color, 0.3); } to { background-color: transparent; } }
row.flash { animation: flash 900ms ease-in-out 2; }
";

/// Left margin per subtask level, in pixels.
const INDENT: i32 = 24;

/// Header and sidebar name for tasks without a category.
fn other() -> String {
    pgettext("category", "Other")
}

/// Sidebar entry for the Upcoming page (tasks with a due date in every list), kept among the
/// list hrefs.
const UPCOMING: &str = "upcoming";
/// Sidebar entry for the Important page (high-priority tasks of every list).
const IMPORTANT: &str = "important";
/// The pages above the lists, in sidebar order.
const PAGES: [&str; 2] = [UPCOMING, IMPORTANT];

/// Completed tasks shown in the "Completed" expander, newest first.
const DONE_SHOWN: usize = 50;

/// Where to support development, listed in About (and in the README).
const DONATIONS: [(&str, &str); 3] = [
    (
        gettext_noop("Support on Ko-fi"),
        "https://ko-fi.com/N5E229GJPZ",
    ),
    (
        gettext_noop("Sponsor on GitHub"),
        "https://github.com/sponsors/SelfRef",
    ),
    (
        gettext_noop("Donate on Liberapay"),
        "https://liberapay.com/SelfRef/donate",
    ),
];

struct Ui {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    split: adw::OverlaySplitView,
    sidebar: gtk::ListBox,
    /// The search, `folded`: while set, the page shows matching tasks of every list.
    query: RefCell<String>,
    search: gtk::SearchBar,
    search_entry: gtk::SearchEntry,
    cat_section: gtk::Box,
    cat_list: gtk::ListBox,
    /// Category of each `cat_list` row; None is "Other" (no category).
    cat_rows: RefCell<Vec<Option<String>>>,
    /// Category the task page is narrowed to, with the list it belongs to.
    filter: RefCell<Option<(String, Option<String>)>>,
    /// Categories created in the picker per list, shown until a task or the config has them.
    new_cats: RefCell<Vec<(String, String)>>,
    /// Set while refresh_tasks rewrites the picker, so "New Category…" handling ignores it.
    cat_busy: Rc<Cell<bool>>,
    hrefs: RefCell<Vec<String>>,
    /// Name and colour of each list's row, to notice when they change.
    looks: RefCell<Vec<(String, Option<String>)>>,
    current: RefCell<Option<String>>,
    title: adw::WindowTitle,
    sync_state: gtk::Stack,
    entry: gtk::Entry,
    cats: gtk::StringList,
    cat: gtk::DropDown,
    groups: gtk::Box,
    /// Root tasks whose subtasks are shown; kept across rebuilds of the task page.
    expanded: RefCell<HashSet<String>>,
    /// Task given an "Add subtask" entry by its "+": a root task shown as an expander for its
    /// first subtask, before it has any; a subtask until Escape or another "+".
    adding: RefCell<Option<String>>,
    /// Rows show their category as a tag (the "Show as tags" category view).
    tags: Cell<bool>,
    /// (open, done) subtasks of each task on the page, for the "done/all" tag.
    counts: RefCell<HashMap<String, (usize, usize)>>,
    /// Tasks are dragged by a handle (touchscreens) instead of the whole row.
    handles: Cell<bool>,
    /// A click on a task's title edits it in place (`inline_edit`).
    inline: Cell<bool>,
    /// A title is being edited in place: rebuilds wait until it is done (`stale`: one waits).
    renaming: Cell<bool>,
    stale: Cell<bool>,
    /// The task being dragged and where it came from.
    dragging: RefCell<Option<(Task, Slot)>>,
    /// "+" buttons taking dropped tasks as subtasks of their task; for a tree row's drag-only
    /// "+", the row it shows on while collapsed.
    adopters: RefCell<Vec<(gtk::Widget, Task, Option<adw::ExpanderRow>)>>,
    /// Drop line: (row under the pointer, row showing the line, its class).
    drop_mark: RefCell<Option<(gtk::Widget, gtk::Widget, &'static str)>>,
    /// Task whose "Add subtask" entry takes focus after the next rebuild.
    focus_sub: RefCell<Option<String>>,
    /// Task whose row pulses (opened from the extension), until when: rebuilds in between, as
    /// after the sync that showing the window starts, pulse the new row.
    flash: RefCell<Option<(String, std::time::Instant)>>,
    /// Welcome page: adding the first account (`add_account`), made anew each time it shows.
    welcome: adw::Bin,
    add_account: RefCell<Option<Rc<crate::add_account::AddAccount>>>,
    last_error: RefCell<String>,
}

thread_local! {
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
}

fn ui() -> Option<Rc<Ui>> {
    UI.with(|u| u.borrow().clone())
}

pub fn is_active() -> bool {
    ui().is_some_and(|u| u.window.is_visible() && u.window.is_active())
}

pub fn setup_actions(app: &adw::Application) {
    let sync = gio::SimpleAction::new("sync", None);
    sync.connect_activate(|_, _| crate::sync_now());
    let quit = gio::SimpleAction::new("quit", None);
    quit.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| app.quit()
    ));
    // From reminder notifications.
    let show = gio::SimpleAction::new("show-list", Some(glib::VariantTy::STRING));
    show.connect_activate(|_, href| {
        if let Some(href) = href.and_then(|h| h.str()) {
            show_list(href);
        }
    });
    // From event reminders: the calendar on that day.
    let day = gio::SimpleAction::new("show-day", Some(glib::VariantTy::INT64));
    day.connect_activate(|_, t| {
        if let Some(t) = t.and_then(|t| t.get::<i64>()) {
            if let Some(app) = gio::Application::default().and_downcast::<adw::Application>() {
                present(&app);
            }
            crate::calendar::show_day(t);
        }
    });
    app.add_action(&day);
    // From the extension's settings button (Extensions app): Preferences on that page.
    let prefs = gio::SimpleAction::new("show-preferences", Some(glib::VariantTy::STRING));
    prefs.connect_activate(|_, page| {
        if let Some(app) = gio::Application::default().and_downcast::<adw::Application>() {
            present(&app);
        }
        if let Some(ui) = ui().filter(|_| crate::ready()) {
            crate::settings::present(&ui.window, page.and_then(|p| p.str()));
        }
    });
    app.add_action(&prefs);
    let complete = gio::SimpleAction::new("complete", Some(glib::VariantTy::STRING));
    complete.connect_activate(|_, uid| {
        // From a notification nobody can be asked: subtasks stay open unless set to complete.
        let tree = prefs::subtask_completion(core()) == prefs::SubtaskCompletion::Complete;
        if let Some(uid) = uid.and_then(|u| u.str())
            && tasks::set_done(core(), uid, true, tree, false).is_ok()
        {
            request_sync();
        }
    });
    let restart = gio::SimpleAction::new("restart", None);
    restart.connect_activate(glib::clone!(
        #[weak]
        app,
        move |_, _| crate::update::restart(&app, false)
    ));
    app.add_action(&restart);
    let shortcuts = gio::SimpleAction::new("shortcuts", None);
    shortcuts.connect_activate(|_, _| {
        if let Some(ui) = ui() {
            get::<adw::Dialog>(&view!("shortcuts"), "dialog").present(Some(&ui.window));
        }
    });
    // Filled from the metainfo file (bundled, see icons/icons.gresource.xml).
    let about = gio::SimpleAction::new("about", None);
    about.connect_activate(|_, _| {
        let Some(ui) = ui() else { return };
        let d = adw::AboutDialog::from_appdata(
            "/dev/aperte/Todav/metainfo.xml",
            Some(env!("CARGO_PKG_VERSION")),
        );
        d.set_copyright("© 2026 SelfRef");
        about_support(&d);
        about_update(&d);
        d.present(Some(&ui.window));
    });
    app.add_action(&about);
    app.add_action(&sync);
    app.add_action(&quit);
    app.add_action(&show);
    app.add_action(&complete);
    app.add_action(&shortcuts);
    // Listed in ui/shortcuts.blp, with the pages' own.
    app.set_accels_for_action("app.sync", &["F5"]);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
    app.set_accels_for_action("app.shortcuts", &["<Control>question"]);
    app.set_accels_for_action("window.close", &["<Control>w"]);
    app.set_accels_for_action("win.preferences", &["<Control>comma"]);
    app.set_accels_for_action("win.undo", &["<Control>z"]);
    app.set_accels_for_action("win.page::tasks", &["<Alt>1"]);
    app.set_accels_for_action("win.page::calendar", &["<Alt>2"]);
    app.set_accels_for_action("win.toggle-sidebar", &["F9"]);
}

/// Turns About's Details page into Support: a note and the donation links instead of the
/// description, with the website left on the main page. AdwAboutDialog has no API for that, so
/// this finds its rows (the same in libadwaita 1.8 to 1.10); if they move, About keeps a Details
/// page with the note and the links.
fn about_support(d: &adw::AboutDialog) {
    d.set_comments(&glib::markup_escape_text(&gettext(
        "Todav is developed in my free time. If it keeps your days in order, you can support its \
         development.",
    )));
    let titles: Vec<String> = DONATIONS.iter().map(|(t, _)| gettext(t)).collect();
    for ((_, uri), title) in DONATIONS.iter().zip(&titles) {
        d.add_link(title, uri);
    }
    // Its content: the dialog itself has no children until presented.
    let Some(content) = d.child() else { return };
    let widgets = descendants(&content);
    // The row that opens the page, followed by the website's own (hidden with a Details page).
    let Some(row) = widgets
        .iter()
        .filter_map(|w| w.downcast_ref::<adw::ActionRow>())
        .find(|r| {
            r.action_name().as_deref() == Some("navigation.push")
                && r.action_target_value()
                    .is_some_and(|v| v.str() == Some("details"))
        })
    else {
        return;
    };
    let Some(page) = widgets
        .iter()
        .find_map(|w| w.downcast_ref::<adw::NavigationView>())
        .and_then(|v| v.find_page("details"))
    else {
        return;
    };
    // Translators: the About row that opens the page with the donation links.
    row.set_title(&gettext("_Support"));
    // Translators: title of About's page with the donation links.
    page.set_title(&gettext("Support"));
    if let Some(website) = row.next_sibling() {
        website.set_visible(true);
    }
    for r in descendants(page.upcast_ref()) {
        if let Some(r) = r.downcast_ref::<adw::PreferencesRow>()
            && !titles.iter().any(|t| *t == r.title())
        {
            r.set_visible(false);
        }
    }
}

/// Checks GitHub for a newer stable release while About is open; if there is one, a first
/// section links to its page. Its place is found like `about_support`'s rows: the box holding
/// the group of What's New.
fn about_update(d: &adw::AboutDialog) {
    let Some(content) = d.child() else { return };
    let Some(groups) = descendants(&content)
        .into_iter()
        .filter_map(|w| w.downcast::<adw::ActionRow>().ok())
        .find(|r| {
            r.action_name().as_deref() == Some("navigation.push")
                && r.action_target_value()
                    .is_some_and(|v| v.str() == Some("whatsnew"))
        })
        .and_then(|r| r.ancestor(adw::PreferencesGroup::static_type()))
        .and_then(|g| g.parent())
        .and_downcast::<gtk::Box>()
    else {
        return;
    };
    let groups = groups.downgrade();
    glib::spawn_future_local(async move {
        let Ok(Some((version, url))) =
            gio::spawn_blocking(|| todav_core::release::newer(env!("CARGO_PKG_VERSION"))).await
        else {
            return;
        };
        let Some(groups) = groups.upgrade() else {
            return;
        };
        let row = adw::ActionRow::builder()
            // Translators: the About row linking to a newer release's page.
            .title(gettext("Version {version} Available").replace("{version}", &version))
            .tooltip_text(&url)
            .activatable(true)
            .build();
        row.add_suffix(
            &gtk::Image::builder()
                .icon_name("adw-external-link-symbolic")
                .accessible_role(gtk::AccessibleRole::Presentation)
                .build(),
        );
        row.connect_activated(move |r| {
            gtk::UriLauncher::new(&url).launch(
                r.root().and_downcast_ref::<gtk::Window>(),
                gio::Cancellable::NONE,
                |_| {},
            );
        });
        let group = adw::PreferencesGroup::new();
        group.add(&row);
        groups.prepend(&group);
    });
}

/// `w` and every widget inside it.
fn descendants(w: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut all = vec![w.clone()];
    let mut i = 0;
    while let Some(w) = all.get(i).cloned() {
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            all.push(c);
        }
        i += 1;
    }
    all
}

pub fn present(app: &adw::Application) {
    let ui = ui().unwrap_or_else(|| {
        let ui = build(app);
        UI.with(|u| *u.borrow_mut() = Some(ui.clone()));
        ui
    });
    if crate::ready() {
        ui.stack.set_visible_child_name("main");
        refresh_lists(&ui);
    } else if ui.add_account.borrow().is_none() {
        welcome(&ui);
    }
    ui.window.present();
}

/// The welcome page, made anew (GNOME Online Accounts may have changed); the window shows the
/// first account's lists once it is added.
fn welcome(ui: &Rc<Ui>) {
    let add = crate::add_account::new(true, |id| {
        account_changed();
        // Google's calendars hold no tasks: the calendar, not an empty task list.
        if core()
            .account(id)
            .is_some_and(|a| a.kind == AccountKind::Google)
        {
            crate::calendar::show_day(core().local_now());
        }
    });
    ui.welcome.set_child(Some(&add.root));
    ui.add_account.replace(Some(add));
    ui.current.take();
    ui.stack.set_visible_child_name("login");
    refresh_lists(ui); // empties the sidebar
}

/// No account left: back to the welcome page.
pub fn show_welcome() {
    if let Some(ui) = ui() {
        welcome(&ui);
    }
}

/// An account came, went, or was switched on or off: show the lists there are now.
pub fn account_changed() {
    if let Some(ui) = ui() {
        ui.current.take();
        ui.add_account.take();
        ui.stack.set_visible_child_name("main");
        refresh_lists(&ui);
    }
}

/// The lists' grouping or order changed (accounts reordered or renamed): the sidebar anew, on
/// the same list.
pub fn lists_changed() {
    if let Some(ui) = ui() {
        refresh_lists(&ui);
    }
}

thread_local! {
    /// The banners on the Tasks and Calendar pages saying which accounts are not connected,
    /// and those offering to restart into a newer version.
    static BANNERS: RefCell<Vec<adw::Banner>> = const { RefCell::new(Vec::new()) };
    static UPDATE_BANNERS: RefCell<Vec<adw::Banner>> = const { RefCell::new(Vec::new()) };
}

/// A page's banners: `banner` for accounts not connected, `update` for a newer version.
pub fn add_banner(banner: adw::Banner, update: adw::Banner) {
    BANNERS.with_borrow_mut(|b| b.push(banner));
    UPDATE_BANNERS.with_borrow_mut(|b| b.push(update));
    update_banners();
}

/// Switched-on Nextcloud accounts without a connection (no password in the keyring, the GNOME
/// Online Accounts account gone): their lists are here, nothing syncs.
pub fn disconnected() -> Vec<todav_core::Account> {
    let mut v = core().accounts();
    v.retain(|a| a.enabled && a.kind != AccountKind::Local && !core().connected(a.id));
    v
}

pub fn update_banners() {
    let off = disconnected();
    let title = match off.as_slice() {
        [] => String::new(),
        [a] => gettext("{name} is not connected").replace("{name}", &crate::account_name(a)),
        _ => ngettext(
            "{n} account is not connected",
            "{n} accounts are not connected",
            off.len() as u64,
        )
        .replace("{n}", &off.len().to_string()),
    };
    BANNERS.with_borrow(|banners| {
        for b in banners {
            b.set_title(&title);
            b.set_revealed(!off.is_empty());
        }
    });
    let updated = crate::update::updated();
    UPDATE_BANNERS.with_borrow(|banners| {
        for b in banners {
            b.set_title(&gettext(
                "Todav was updated: restart it to use the new version",
            ));
            b.set_revealed(updated);
        }
    });
}

/// A sync button's icon, or its spinner while syncing.
pub fn set_syncing(state: &gtk::Stack, on: bool) {
    state.set_visible_child_name(if on { "syncing" } else { "idle" });
}

/// Rebuild the sidebar (when its pages change) and the task page.
pub fn refresh_all() {
    if let Some(ui) = ui() {
        refresh_sidebar_counts(&ui);
        refresh_tasks(&ui);
    }
}

/// Rebuild the task page, e.g. after the task order changed.
pub fn refresh() {
    if let Some(ui) = ui() {
        refresh_tasks(&ui);
    }
}

/// The main window, once built.
pub fn window() -> Option<adw::ApplicationWindow> {
    ui().map(|ui| ui.window.clone())
}

pub fn toast(msg: &str) {
    if let Some(ui) = ui() {
        ui.toasts.add_toast(note(msg));
    }
}

/// A toast showing `text` as it is: a toast title is markup otherwise, and errors carry `<`.
pub fn note(text: &str) -> adw::Toast {
    let t = adw::Toast::new(text);
    t.set_use_markup(false);
    t
}

pub fn show_list(href: &str) {
    if let Some(app) = gio::Application::default().and_downcast::<adw::Application>() {
        present(&app);
    }
    if let Some(ui) = ui() {
        *ui.current.borrow_mut() = Some(href.to_string());
        refresh_lists(&ui);
        show_content(&ui.split);
    }
}

/// Show or hide the subtasks of `uid`, also after a restart.
fn expand(ui: &Ui, uid: &str, open: bool) {
    if open {
        ui.expanded.borrow_mut().insert(uid.into());
    } else {
        ui.expanded.borrow_mut().remove(uid);
    }
    prefs::set_expanded(core(), uid, open);
}

/// The Tasks page on task `uid`'s list, its row in view and pulsing once (from the extension):
/// its parents open, a category filter or search that hides it gone.
pub fn show_task(uid: &str) {
    if let Some(app) = gio::Application::default().and_downcast::<adw::Application>() {
        present(&app); // builds the window when running in the background
    }
    let (Some(t), Some(ui)) = (core().task(uid.into()), ui()) else {
        return;
    };
    let mut parent = t.parent_uid.clone();
    while let Some(p) = parent {
        expand(&ui, &p, true);
        parent = core().task(p).and_then(|p| p.parent_uid);
    }
    ui.filter.take();
    ui.search.set_search_mode(false); // clears the query too
    ui.query.borrow_mut().clear();
    *ui.flash.borrow_mut() = Some((t.uid, std::time::Instant::now() + FLASH));
    let _ = WidgetExt::activate_action(&ui.window, "win.page", Some(&"tasks".to_variant()));
    show_list(&t.list_href);
}

/// How long a row `show_task` asked for pulses (the `flash` animation, twice).
pub const FLASH: std::time::Duration = std::time::Duration::from_millis(1800);

/// Pulse `row` if it is the task `show_task` asked for, once it is laid out, and scroll to it.
fn flash_if(ui: &Ui, row: &impl IsA<gtk::Widget>, uid: &str) {
    let left = match &*ui.flash.borrow() {
        Some((u, until)) if u == uid => until.saturating_duration_since(std::time::Instant::now()),
        _ => return,
    };
    if left.is_zero() {
        ui.flash.take();
        return;
    }
    let row = row.clone().upcast::<gtk::Widget>();
    let scroll = ui
        .groups
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>();
    glib::idle_add_local_once(move || {
        if let Some(s) = scroll
            && let Some(child) = s.child()
            && let Some(at) = row.compute_point(&child, &gtk::graphene::Point::new(0.0, 0.0))
        {
            let adj = s.vadjustment();
            adj.set_value(at.y() as f64 - adj.page_size() / 3.0);
        }
        row.add_css_class("flash");
        glib::timeout_add_local_once(left, move || row.remove_css_class("flash"));
    });
}

pub fn on_event(ev: &Event) {
    let Some(ui) = ui() else { return };
    match ev {
        Event::Changed(list) => {
            let list = list.clone();
            // Rebuild outside the signal handler that caused the change.
            glib::idle_add_local_once(move || {
                refresh_sidebar_counts(&ui);
                let current = ui.current.borrow().clone();
                let page = current.as_deref().is_some_and(|c| PAGES.contains(&c));
                if current.as_deref() == Some(&list) || page || !ui.query.borrow().is_empty() {
                    refresh_tasks(&ui);
                }
                crate::calendar::refresh();
            });
        }
        Event::SyncState(s) => {
            set_syncing(&ui.sync_state, s == "syncing");
            crate::calendar::set_syncing(s == "syncing");
            if let Some(err) = s.strip_prefix("error:") {
                if *ui.last_error.borrow() != err {
                    ui.toasts.add_toast(note(
                        &gettext("Sync failed: {error}").replace("{error}", err),
                    ));
                    *ui.last_error.borrow_mut() = err.to_string();
                }
            } else if s == "idle" {
                ui.last_error.borrow_mut().clear();
                if ui.stack.visible_child_name().as_deref() == Some("main")
                    && ui.hrefs.borrow().is_empty()
                {
                    refresh_lists(&ui);
                }
            }
        }
        // Each refused item by name; a task opens to be fixed (its dialog says why).
        Event::Refused(items) => {
            for r in items {
                let t = note(
                    &if r.deleted {
                        gettext("“{title}” could not be deleted on the server")
                    } else if r.task {
                        gettext("“{title}” was not saved on the server")
                    } else {
                        gettext("“{title}” was not saved on the server: {reason}")
                            .replace("{reason}", server_reason(&r.error))
                    }
                    .replace("{title}", &r.summary),
                );
                t.set_priority(adw::ToastPriority::High);
                if r.task && !r.deleted {
                    t.set_button_label(Some(&gettext("Edit")));
                    let uid = r.uid.clone();
                    t.connect_button_clicked(move |_| {
                        if let Some(task) = core().task(uid.clone()) {
                            edit_task(&task);
                        }
                    });
                }
                ui.toasts.add_toast(t);
                on_event(&Event::Changed(r.list_href.clone()));
            }
        }
    }
}

/// What the server said is wrong, without the "HTTP 415: " before it.
pub fn server_reason(error: &str) -> &str {
    error
        .split_once(": ")
        .filter(|(code, _)| code.starts_with("HTTP "))
        .map_or(error, |(_, why)| why)
}

/// Loads a view compiled from `ui/<name>.blp` by build.rs.
macro_rules! view {
    ($name:literal) => {
        gtk::Builder::from_string(include_str!(concat!(env!("OUT_DIR"), "/", $name, ".ui")))
    };
}
pub(crate) use view;

pub fn get<T: IsA<glib::Object>>(b: &gtk::Builder, id: &str) -> T {
    b.object(id).unwrap_or_else(|| panic!("no `{id}` in view"))
}

/// Restores the last window size and saves it when the window is hidden or the app quits
/// (Ctrl+Q skips close-request). GTK keeps the unmaximized size in default-width/height.
fn restore_size(window: &adw::ApplicationWindow, app: &adw::Application) {
    let size = core().setting("window_size".into()).unwrap_or_default();
    if let Some((w, h)) = size.split_once('x')
        && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
    {
        window.set_default_size(w, h);
    }
    window.set_maximized(core().setting("window_maximized".into()).as_deref() == Some("true"));
    fn save(window: &adw::ApplicationWindow) {
        let (w, h) = window.default_size();
        let _ = core().set_setting("window_size".into(), format!("{w}x{h}"));
        let _ = core().set_setting("window_maximized".into(), window.is_maximized().to_string());
    }
    window.connect_close_request(glib::clone!(
        #[weak]
        app,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |w| {
            save(w);
            // Once it is hidden: an update waiting restarts the app now.
            glib::idle_add_local_once(move || crate::update::window_closed(&app));
            glib::Propagation::Proceed
        }
    ));
    app.connect_shutdown(glib::clone!(
        #[weak]
        window,
        move |_| save(&window)
    ));
}

fn build(app: &adw::Application) -> Rc<Ui> {
    let b = view!("window");
    let window: adw::ApplicationWindow = get(&b, "window");
    window.set_application(Some(app));
    if crate::sandbox() {
        window.set_title(Some("Todav (sandbox)"));
    }
    restore_size(&window, app);

    let css = gtk::CssProvider::new();
    css.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &WidgetExt::display(&window),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let ui = Rc::new(Ui {
        window,
        toasts: get(&b, "toasts"),
        stack: get(&b, "stack"),
        split: get(&b, "split"),
        sidebar: get(&b, "sidebar"),
        cat_section: get(&b, "cat_section"),
        cat_list: get(&b, "cat_list"),
        cat_rows: RefCell::new(Vec::new()),
        filter: RefCell::new(None),
        new_cats: RefCell::new(Vec::new()),
        cat_busy: Rc::new(Cell::new(false)),
        hrefs: RefCell::new(Vec::new()),
        looks: RefCell::new(Vec::new()),
        current: RefCell::new(None),
        query: RefCell::new(String::new()),
        search: get(&b, "task_search"),
        search_entry: get(&b, "task_query"),
        title: get(&b, "title"),
        sync_state: get(&b, "sync_state"),
        entry: get(&b, "entry"),
        cats: get(&b, "cats"),
        cat: get(&b, "cat"),
        groups: get(&b, "groups"),
        expanded: RefCell::new(prefs::expanded(core())),
        adding: RefCell::new(None),
        tags: Cell::new(false),
        counts: RefCell::default(),
        handles: Cell::new(false),
        inline: Cell::new(false),
        renaming: Cell::new(false),
        stale: Cell::new(false),
        dragging: RefCell::new(None),
        drop_mark: RefCell::new(None),
        adopters: RefCell::new(Vec::new()),
        focus_sub: RefCell::new(None),
        flash: RefCell::new(None),
        welcome: get(&b, "welcome"),
        add_account: RefCell::new(None),
        last_error: RefCell::new(String::new()),
    });
    add_banner(get(&b, "banner"), get(&b, "update_banner"));
    connect(&ui, &b);
    let views = get(&b, "views");
    connect_search(&ui, &get(&b, "task_search"), &get(&b, "task_query"), &views);
    crate::calendar::build(&views, &ui.split);
    ui
}

/// The page's content after picking something in its sidebar: an overlaid (collapsed) sidebar
/// closes.
pub fn show_content(split: &adw::OverlaySplitView) {
    if split.is_collapsed() {
        split.set_show_sidebar(false);
    }
}

/// A page's sidebar holds the Tasks / Calendar switcher; `switcher`, in the content's header, stands
/// in for it while the sidebar is hidden on a wide window (collapsed, the sidebar is a toggle away).
pub fn sidebar_switcher(split: &adw::OverlaySplitView, switcher: &adw::InlineViewSwitcher) {
    let update = {
        let switcher = switcher.clone();
        move |s: &adw::OverlaySplitView| {
            switcher.set_visible(!s.shows_sidebar() && !s.is_collapsed())
        }
    };
    update(split);
    split.connect_show_sidebar_notify(update.clone());
    split.connect_collapsed_notify(update);
}

/// Typing on the Tasks page searches the tasks of every list (GNOME's type to search).
fn connect_search(
    ui: &Rc<Ui>,
    bar: &gtk::SearchBar,
    entry: &gtk::SearchEntry,
    views: &adw::ViewStack,
) {
    search_bar(bar, entry, views, "tasks");
    // Picking a list ends the search.
    let b = bar.clone();
    ui.sidebar
        .connect_row_activated(move |_, _| b.set_search_mode(false));
    let weak = Rc::downgrade(ui);
    entry.connect_search_changed(move |e| {
        let Some(ui) = weak.upgrade() else { return };
        *ui.query.borrow_mut() = folded(e.text().trim());
        refresh_tasks(&ui);
    });
}

/// The Tasks page searching for `text` (from the Activities overview's search).
pub fn search(text: &str) {
    if let Some(app) = gio::Application::default().and_downcast::<adw::Application>() {
        present(&app);
    }
    let Some(ui) = ui() else { return };
    let _ = WidgetExt::activate_action(&ui.window, "win.page", Some(&"tasks".to_variant()));
    ui.search.set_search_mode(true);
    ui.search_entry.set_text(text);
    ui.search_entry.set_position(-1);
}

/// Text as search compares it: lower case, without diacritics ("Żarówki" → "zarowki").
pub fn folded(s: &str) -> String {
    glib::normalize(s, glib::NormalizeMode::All)
        .chars()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c)) // the marks split off above
        .map(|c| match c {
            'ł' => 'l', // a letter of its own, not split
            'Ł' => 'L',
            c => c,
        })
        .collect::<String>()
        .to_lowercase()
}

/// Set up the search bar of the `views` page `page`, its `entry` not being its direct child:
/// typing anywhere in the window opens it while that page is shown (unless an entry has focus).
pub fn search_bar(
    bar: &gtk::SearchBar,
    entry: &gtk::SearchEntry,
    views: &adw::ViewStack,
    page: &'static str,
) {
    bar.connect_entry(entry);
    let capture = {
        let bar = bar.clone();
        move |v: &adw::ViewStack| {
            let shown = v.visible_child_name().as_deref() == Some(page);
            let window = v.root().filter(|_| shown).and_upcast::<gtk::Widget>();
            bar.set_key_capture_widget(window.as_ref());
        }
    };
    capture(views);
    views.connect_visible_child_name_notify(capture);
    // Closed: the search ends, and the focus goes back to the page rather than into another
    // entry (where typing would not search).
    let entry = entry.clone();
    bar.connect_search_mode_enabled_notify(move |b| {
        if !b.is_search_mode() {
            entry.set_text("");
            if let Some(root) = b.root() {
                root.set_focus(None::<&gtk::Widget>);
            }
        }
    });
}

/// Open a task's edit dialog (from the calendar).
pub fn edit_task(t: &Task) {
    if let Some(ui) = ui() {
        edit_dialog(&ui, t, t.parent_uid.is_some());
    }
}

fn connect(ui: &Rc<Ui>, b: &gtk::Builder) {
    sidebar_switcher(&ui.split, &get(b, "task_switcher"));
    let weak = Rc::downgrade(ui);
    ui.sidebar.connect_row_selected(move |_, row| {
        let (Some(ui), Some(row)) = (weak.upgrade(), row) else {
            return;
        };
        let href = ui.hrefs.borrow().get(row.index() as usize).cloned();
        if href.is_some() && *ui.current.borrow() != href {
            *ui.current.borrow_mut() = href.clone();
            let _ = core().set_setting("last_list".into(), href.unwrap_or_default());
            refresh_tasks(&ui);
        }
    });
    // Picked, even the list already shown.
    let split = ui.split.clone();
    ui.sidebar
        .connect_row_activated(move |_, _| show_content(&split));
    crate::lists::setup(&ui.window);
    let weak = Rc::downgrade(ui);
    crate::lists::context_menu(&ui.sidebar, "win.edit-list", None, move |row| {
        let ui = weak.upgrade()?;
        let href = ui.hrefs.borrow().get(row.index() as usize).cloned();
        href.filter(|h| !PAGES.contains(&h.as_str()))
    });

    // Clicking the selected category clears the filter.
    let weak = Rc::downgrade(ui);
    ui.cat_list.connect_row_activated(move |list, row| {
        let Some(ui) = weak.upgrade() else { return };
        let (Some(href), Some(cat)) = (
            ui.current.borrow().clone(),
            ui.cat_rows.borrow().get(row.index() as usize).cloned(),
        ) else {
            return;
        };
        let pick = (href, cat);
        let again = ui.filter.borrow().as_ref() == Some(&pick);
        if again {
            list.unselect_all();
        }
        *ui.filter.borrow_mut() = (!again).then_some(pick);
        show_content(&ui.split);
        glib::idle_add_local_once(move || refresh_tasks(&ui));
    });

    connect_new_category(ui, &ui.cat, &ui.cats, ui.cat_busy.clone(), |ui, name| {
        let href = ui.current.borrow().clone();
        if let Some(href) = href {
            ui.new_cats.borrow_mut().push((href, name.clone()));
        }
        refresh_tasks(ui);
        select_name(&ui.cat, &ui.cats, &name);
    });

    // "+": accented and usable once something is typed.
    let add: gtk::Button = get(b, "add_task");
    let entry = ui.entry.clone();
    add.connect_clicked(move |_| entry.emit_activate());
    ui.entry.connect_changed(move |e| {
        let typed = !e.text().trim().is_empty();
        add.set_sensitive(typed);
        if typed {
            add.add_css_class("suggested-action");
        } else {
            add.remove_css_class("suggested-action");
        }
    });
    ui.entry.add_controller(task_drop(ui, |_| false, false)); // tasks are not text to type

    let weak = Rc::downgrade(ui);
    ui.entry.connect_activate(move |entry| {
        let Some(ui) = weak.upgrade() else { return };
        let text = entry.text().trim().to_string();
        let Some(list) = ui.current.borrow().clone() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        // With a category filter the picker is hidden and the filter decides.
        let cat = match ui.filter.borrow().clone() {
            Some((_, c)) => c,
            None => (ui.cat.selected() > 0)
                .then(|| ui.cats.string(ui.cat.selected()).map(|s| s.to_string()))
                .flatten(),
        };
        let start = prefs::order(core()).task_start;
        match core().add_task(list, text, cat, None, start) {
            Ok(_) => {
                entry.set_text("");
                request_sync();
            }
            Err(e) => ui.toasts.add_toast(note(&e.to_string())),
        }
    });

    // The banner's "Sign In": the first account not connected, in Preferences.
    let weak = Rc::downgrade(ui);
    let fix = gio::SimpleAction::new("fix-account", None);
    fix.connect_activate(move |_, _| {
        let Some(ui) = weak.upgrade() else { return };
        if let Some(a) = disconnected().first() {
            crate::settings::present_account(&ui.window, a.id);
        }
    });
    ui.window.add_action(&fix);

    let weak = Rc::downgrade(ui);
    let prefs = gio::SimpleAction::new("preferences", None);
    prefs.connect_activate(move |_, _| {
        if let Some(ui) = weak.upgrade().filter(|_| crate::ready()) {
            crate::settings::present(&ui.window, None);
        }
    });
    ui.window.add_action(&prefs);

    // Undo the latest change; in a text field, the typing there.
    let window = ui.window.downgrade();
    let undo_last = gio::SimpleAction::new("undo", None);
    undo_last.connect_activate(move |_, _| {
        let Some(w) = window.upgrade() else { return };
        match gtk::prelude::RootExt::focus(&w)
            .filter(|f| f.is::<gtk::Text>() || f.is::<gtk::TextView>())
        {
            Some(text) => {
                let _ = text.activate_action("text.undo", None);
            }
            // Not from under a dialog, whose own changes are not saved yet.
            None if w.visible_dialog().is_none() => undo(None),
            None => {}
        }
    });
    ui.window.add_action(&undo_last);
    let views: adw::ViewStack = get(b, "views");
    let page = gio::SimpleAction::new("page", Some(glib::VariantTy::STRING));
    page.connect_activate(move |_, name| {
        if let Some(name) = name.and_then(|n| n.str()) {
            views.set_visible_child_name(name);
        }
    });
    ui.window.add_action(&page);
    // Both pages' sidebars, bound together (src/calendar.rs).
    let toggle = gio::SimpleAction::new("toggle-sidebar", None);
    let split = ui.split.clone();
    toggle.connect_activate(move |_, _| split.set_show_sidebar(!split.shows_sidebar()));
    ui.window.add_action(&toggle);
    // The edit dialog of the task with that UID (a row's context menu, as "To Plan"'s).
    let edit = gio::SimpleAction::new("edit-task", Some(glib::VariantTy::STRING));
    edit.connect_activate(|_, uid| {
        if let Some(t) = uid
            .and_then(|u| u.str())
            .and_then(|u| core().task(u.to_string()))
        {
            edit_task(&t);
        }
    });
    ui.window.add_action(&edit);
    // Delete the task with that UID, with an Undo toast (a row's context menu, as "To Plan"'s).
    let delete = gio::SimpleAction::new("delete-task", Some(glib::VariantTy::STRING));
    delete.connect_activate(|_, uid| {
        if let Some(t) = uid
            .and_then(|u| u.str())
            .and_then(|u| core().task(u.to_string()))
        {
            delete_task(&t);
        }
    });
    ui.window.add_action(&delete);
    // Ctrl+N on the Tasks page goes to the entry adding tasks (the Calendar page's opens a new
    // event).
    let keys = gtk::ShortcutController::new();
    keys.set_scope(gtk::ShortcutScope::Managed);
    let entry = ui.entry.clone();
    keys.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("<Control>n"),
        Some(gtk::CallbackAction::new(move |_, _| {
            entry.grab_focus();
            glib::Propagation::Stop
        })),
    ));
    ui.split.add_controller(keys);
}

/// What every way of signing in says when the server field is empty.
pub fn no_server() -> String {
    gettext("Enter the Nextcloud server first")
}

/// Full URL from a scheme picker and host entry; empty stays empty, a typed scheme wins over the picker.
pub fn url_text((scheme, row): &(gtk::DropDown, adw::EntryRow)) -> String {
    let host = row.text().trim().to_string();
    if host.is_empty() || host.contains("://") {
        return host;
    }
    let scheme = if scheme.selected() == 0 {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://{host}")
}

/// Bumped on every sign-in and on cancel; a sign-in only goes on while its number is current.
static LOGIN_ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// Stop a running `browser_login`; it then returns `Ok(None)`.
pub fn cancel_login() {
    LOGIN_ATTEMPT.fetch_add(1, Ordering::SeqCst);
}

/// Login Flow v2: open the browser (or copy the link) and wait until the user approves.
/// `Ok(None)`: cancelled, or another sign-in started.
pub async fn browser_login(
    server: String,
    copy: bool,
) -> Result<Option<todav_core::Credentials>, String> {
    let attempt = LOGIN_ATTEMPT.fetch_add(1, Ordering::SeqCst) + 1;
    let current = move || LOGIN_ATTEMPT.load(Ordering::SeqCst) == attempt;
    let flow = gio::spawn_blocking(move || todav_core::login_flow_start(server))
        .await
        .map_err(|_| gettext("Cannot start login"))?
        .map_err(|e| gettext("Cannot start login: {error}").replace("{error}", &e.to_string()))?;
    if !current() {
        return Ok(None);
    }
    if copy {
        if let Some(d) = gtk::gdk::Display::default() {
            d.clipboard().set_text(&flow.login_url);
        }
        toast(&gettext("Login link copied to clipboard"));
    } else {
        let parent = ui().map(|u| u.window.clone());
        gtk::UriLauncher::new(&flow.login_url).launch(
            parent.as_ref(),
            gio::Cancellable::NONE,
            |_| {},
        );
    }
    let flow = std::sync::Arc::new(flow);
    for _ in 0..600 {
        glib::timeout_future_seconds(2).await;
        if !current() {
            return Ok(None);
        }
        let f = flow.clone();
        match gio::spawn_blocking(move || todav_core::login_flow_poll(&f)).await {
            Ok(Ok(None)) => continue,
            Ok(Ok(Some(c))) => return Ok(current().then_some(c)),
            Ok(Err(e)) => {
                return Err(gettext("Login failed: {error}").replace("{error}", &e.to_string()));
            }
            Err(_) => return Err(gettext("Login failed")),
        }
    }
    Err(gettext("Login timed out"))
}

/// Ask for a login name and (app) password for `server` and check them there; `done` gets the
/// credentials once they work.
pub fn password_login(
    parent: &impl IsA<gtk::Widget>,
    server: String,
    login_name: Option<String>,
    done: impl Fn(todav_core::Credentials) + 'static,
) {
    let b = view!("password-login");
    let known = login_name;
    let dialog: adw::Dialog = get(&b, "dialog");
    let user: adw::EntryRow = get(&b, "user");
    let password: adw::PasswordEntryRow = get(&b, "password");
    let sign_in: gtk::Button = get(&b, "sign_in");
    let error: adw::Banner = get(&b, "error");
    // Signing in again keeps the login name.
    if let Some(name) = &known {
        user.set_text(name);
        user.set_sensitive(false);
        dialog.set_property("focus-widget", &password);
    }
    let filled = {
        let (user, password, sign_in) = (user.clone(), password.clone(), sign_in.clone());
        let error = error.clone();
        move || {
            error.set_revealed(false);
            sign_in.set_sensitive(!user.text().trim().is_empty() && !password.text().is_empty())
        }
    };
    let f = filled.clone();
    user.connect_changed(move |_| f());
    password.connect_changed(move |_| filled());
    let d = dialog.clone();
    get::<gtk::Button>(&b, "cancel").connect_clicked(move |_| {
        d.close();
    });
    let (d, done) = (dialog.clone(), Rc::new(done));
    sign_in.connect_clicked(move |button| {
        let (login, pw) = (user.text().trim().to_string(), password.text().to_string());
        let server = server.clone();
        button.set_sensitive(false);
        let (d, done, error, button) = (d.clone(), done.clone(), error.clone(), button.clone());
        glib::spawn_future_local(async move {
            let r =
                gio::spawn_blocking(move || todav_core::password_login(server, login, pw)).await;
            let msg = match r {
                Ok(Ok(c)) => {
                    d.close();
                    return done(c);
                }
                Ok(Err(todav_core::Error::Auth)) => gettext("Wrong login name or password"),
                Ok(Err(e)) => gettext("Sign-in failed: {error}").replace("{error}", &e.to_string()),
                Err(_) => gettext("Sign-in failed"),
            };
            error.set_title(&msg);
            error.set_revealed(true);
            button.set_sensitive(true);
        });
    });
    dialog.present(Some(parent));
}

fn refresh_lists(ui: &Rc<Ui>) {
    // Nothing to sync without a connected account: the sync buttons are greyed out.
    if let Some(sync) = gio::Application::default()
        .and_then(|a| a.lookup_action("sync"))
        .and_downcast::<gio::SimpleAction>()
    {
        sync.set_enabled(crate::can_sync());
    }
    // Nowhere to make lists (e.g. only Google's calendars): no "New List" or "New Calendar".
    let places = !crate::lists::places().is_empty();
    for name in ["new-list", "new-calendar"] {
        if let Some(a) = ui
            .window
            .lookup_action(name)
            .and_downcast::<gio::SimpleAction>()
        {
            a.set_enabled(places);
        }
    }
    update_banners();
    let lists = todav_app::lists::by_place(core(), core().lists());
    let pages = pages();
    let valid = |h: &str| pages.iter().any(|(p, _)| *p == h) || lists.iter().any(|l| l.href == h);
    if ui.current.borrow().as_deref().is_none_or(|c| !valid(c)) {
        let last = core().setting("last_list".into()).filter(|l| valid(l));
        *ui.current.borrow_mut() = last.or_else(|| lists.first().map(|l| l.href.clone()));
    }
    crate::lists::paint();
    ui.sidebar.remove_all();
    *ui.looks.borrow_mut() = looks(&lists);
    *ui.hrefs.borrow_mut() = sidebar_hrefs(&pages, &lists);
    for (page, n) in &pages {
        let (icon, title) = if *page == UPCOMING {
            ("x-office-calendar-symbolic", gettext("Upcoming"))
        } else {
            ("starred-symbolic", gettext("Important"))
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.append(&gtk::Image::from_icon_name(icon));
        let name = gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .hexpand(true)
            .build();
        let count = gtk::Label::new(Some(&n.to_string()));
        count.add_css_class("dim-label");
        row.append(&name);
        row.append(&count);
        ui.sidebar.append(&row);
    }
    for l in &lists {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        // As wide as the Upcoming icon above.
        let dot = crate::lists::dot(&l.href);
        dot.set_margin_start(3);
        dot.set_margin_end(3);
        row.append(&dot);
        let name = gtk::Label::builder()
            .label(&l.display_name)
            .xalign(0.0)
            .hexpand(true)
            .build();
        let count = gtk::Label::new(Some(&l.open_count.to_string()));
        count.add_css_class("dim-label");
        row.append(&name);
        row.append(&count);
        ui.sidebar.append(&row);
        if let Some(r) = row.parent().and_downcast::<gtk::ListBoxRow>() {
            if l.writable {
                list_drop(ui, &r, &l.href);
            }
            let siblings = lists.iter().filter(|s| s.account == l.account);
            crate::lists::reorderable(&r, l, siblings.map(|s| s.href.clone()).collect());
        }
    }
    // Upcoming and Important first, then the lists grouped by where they are kept.
    crate::lists::set_headings(&ui.sidebar, pages.len(), crate::lists::headings(&lists));
    let current = ui.current.borrow().clone();
    if let Some(i) = ui
        .hrefs
        .borrow()
        .iter()
        .position(|h| Some(h) == current.as_ref())
    {
        ui.sidebar
            .select_row(ui.sidebar.row_at_index(i as i32).as_ref());
    }
    refresh_tasks(ui);
}

/// Dropping a task on the sidebar list `href` moves it there with its subtasks; a subtask
/// leaves its parent first.
fn list_drop(ui: &Rc<Ui>, row: &gtk::ListBoxRow, href: &str) {
    let drop = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::MOVE);
    let (weak, h) = (Rc::downgrade(ui), href.to_string());
    drop.connect_accept(move |_, _| {
        weak.upgrade().is_some_and(|ui| {
            let dragging = ui.dragging.borrow();
            dragging.as_ref().is_some_and(|(d, _)| d.list_href != h)
        })
    });
    let (weak, h) = (Rc::downgrade(ui), href.to_string());
    drop.connect_drop(move |_, _, _, _| {
        let Some((d, ds)) = weak.upgrade().and_then(|ui| ui.dragging.take()) else {
            return false;
        };
        let moved = core().together(|| {
            if !ds.root {
                core().promote(d.uid.clone())?;
            }
            core().move_task(d.uid.clone(), h.clone())
        });
        match moved {
            Ok(()) => request_sync(),
            Err(e) => toast(&e.to_string()),
        }
        true
    });
    row.add_css_class("cat-drop");
    row.add_controller(drop);
}

fn refresh_sidebar_counts(ui: &Rc<Ui>) {
    let lists = todav_app::lists::by_place(core(), core().lists());
    let pages = pages();
    // A page appearing or going (its first or last task) rebuilds the sidebar too.
    if sidebar_hrefs(&pages, &lists) != *ui.hrefs.borrow() || looks(&lists) != *ui.looks.borrow() {
        return refresh_lists(ui);
    }
    let counts = pages
        .iter()
        .map(|(_, n)| *n)
        .chain(lists.iter().map(|l| l.open_count as usize));
    for (i, n) in counts.take(ui.hrefs.borrow().len()).enumerate() {
        let count = ui
            .sidebar
            .row_at_index(i as i32)
            .and_then(|r| r.child())
            .and_then(|b| b.last_child())
            .and_downcast::<gtk::Label>();
        if let Some(c) = count {
            c.set_label(&n.to_string());
        }
    }
}

fn looks(lists: &[todav_core::List]) -> Vec<(String, Option<String>)> {
    lists
        .iter()
        .map(|l| (l.display_name.clone(), l.color.clone()))
        .collect()
}

/// The pages shown above the lists, with their sidebar counts.
fn pages() -> Vec<(&'static str, usize)> {
    todav_app::tasks::pages(core())
        .into_iter()
        .map(|(page, n)| match page {
            Page::Upcoming => (UPCOMING, n),
            Page::Important => (IMPORTANT, n),
        })
        .collect()
}

/// The sidebar rows' hrefs: the pages, then the lists.
fn sidebar_hrefs(pages: &[(&str, usize)], lists: &[todav_core::List]) -> Vec<String> {
    let pages = pages.iter().map(|(p, _)| p.to_string());
    pages.chain(lists.iter().map(|l| l.href.clone())).collect()
}

/// Rebuild the task page where it is scrolled to. The rebuild destroys the focused widget when
/// it is a row's (a ticked checkbox): GTK would move focus to a container and scroll that into
/// view, so focus is dropped first.
fn refresh_tasks(ui: &Rc<Ui>) {
    if ui.renaming.get() {
        ui.stale.set(true);
        return;
    }
    if GtkWindowExt::focus(&ui.window).is_some_and(|f| f.is_ancestor(&ui.groups)) {
        GtkWindowExt::set_focus(&ui.window, None::<&gtk::Widget>);
    }
    let adj = ui
        .groups
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
        .map(|s| s.vadjustment());
    let keep = adj.as_ref().map(|a| a.value());
    rebuild_tasks(ui);
    if let (Some(adj), Some(v)) = (adj, keep) {
        adj.set_value(v);
    }
}

fn rebuild_tasks(ui: &Rc<Ui>) {
    while let Some(c) = ui.groups.first_child() {
        ui.groups.remove(&c);
    }
    ui.adopters.borrow_mut().clear();
    ui.inline.set(prefs::inline_edit(core()));
    let Some(href) = ui.current.borrow().clone() else {
        ui.title.set_title("");
        return;
    };
    // Upcoming and search results mix lists: count subtasks in all of them.
    let query = ui.query.borrow().clone();
    let mixed = PAGES.contains(&href.as_str()) || !query.is_empty();
    let all: Vec<Task> = if mixed {
        let lists = core().lists().into_iter();
        lists.flat_map(|l| core().tasks(l.href, true)).collect()
    } else {
        core().tasks(href.clone(), true)
    };
    *ui.counts.borrow_mut() = tasks::subtask_counts(&all);
    // The new-task entry and its category picker.
    let add_box = ui.entry.parent();
    add_box.iter().for_each(|b| b.set_visible(!mixed));
    if !query.is_empty() {
        ui.cat_section.set_visible(false);
        return rebuild_search(ui, &query, all);
    }
    if href == UPCOMING {
        ui.cat_section.set_visible(false);
        return rebuild_upcoming(ui);
    }
    if href == IMPORTANT {
        ui.cat_section.set_visible(false);
        return rebuild_important(ui);
    }
    let list = core().lists().into_iter().find(|l| l.href == href);
    ui.title
        .set_title(list.as_ref().map_or("", |l| &l.display_name));
    ui.title.set_subtitle(
        &list
            .map(|l| open_count(l.open_count as u64))
            .unwrap_or_default(),
    );

    let order = prefs::order(core());
    ui.handles
        .set(core().setting("drag_handles".into()).as_deref() == Some("true"));
    let done_view = prefs::done_view(core());
    let groups = core().grouped_by(href.clone(), order.tasks, order.subtasks, done_view);

    // Category picker: configured categories, then any others in use; keep the selection.
    let selected = ui.cats.string(ui.cat.selected()).map(|s| s.to_string());
    let mut names: Vec<String> = core()
        .categories(href.clone())
        .into_iter()
        .map(|c| c.name)
        .collect();
    let fresh = ui.new_cats.borrow();
    let fresh = fresh.iter().filter(|(h, _)| *h == href).map(|(_, n)| n);
    for n in groups.iter().filter_map(|g| g.name.as_ref()).chain(fresh) {
        if !names.contains(n) {
            names.push(n.clone());
        }
    }
    let (none, new) = (gettext("No category"), gettext("New Category…"));
    let refs: Vec<&str> = std::iter::once(none.as_str())
        .chain(names.iter().map(String::as_str))
        .chain(std::iter::once(new.as_str()))
        .collect();
    let sel = selected
        .and_then(|s| refs[..refs.len() - 1].iter().position(|r| *r == s))
        .unwrap_or(0);
    ui.cat_busy.set(true);
    ui.cats.splice(0, ui.cats.n_items(), &refs);
    ui.cat.set_selected(sel as u32);
    ui.cat_busy.set(false);

    // Sidebar categories plus "Other"; rebuilt only when they change, so the selection holds.
    let rows: Vec<Option<String>> = names
        .iter()
        .cloned()
        .map(Some)
        .chain((!names.is_empty()).then_some(None))
        .collect();
    if *ui.cat_rows.borrow() != rows {
        ui.cat_list.remove_all();
        for (i, r) in rows.iter().enumerate() {
            let label = r.clone().unwrap_or_else(other);
            ui.cat_list
                .append(&gtk::Label::builder().label(label).xalign(0.0).build());
            // Dropping a task here moves it into this category.
            let drop = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::MOVE);
            let weak = Rc::downgrade(ui);
            drop.connect_accept(move |_, _| {
                weak.upgrade()
                    .is_some_and(|ui| ui.dragging.borrow().is_some())
            });
            let (weak, cat) = (Rc::downgrade(ui), r.clone());
            drop.connect_drop(move |_, _, _, _| {
                let Some((d, ds)) = weak.upgrade().and_then(|ui| ui.dragging.take()) else {
                    return false;
                };
                match tasks::drop_on_category(core(), &d, &ds, cat.clone()) {
                    Ok(()) => request_sync(),
                    Err(e) => toast(&e.to_string()),
                }
                true
            });
            if let Some(row) = ui.cat_list.row_at_index(i as i32) {
                row.add_css_class("cat-drop");
                row.add_controller(drop);
            }
        }
        *ui.cat_rows.borrow_mut() = rows;
    }
    ui.cat_section.set_visible(!names.is_empty());

    // The filter only survives while its list is shown and its category is listed.
    let filter = ui
        .filter
        .borrow()
        .clone()
        .filter(|(h, c)| *h == href && ui.cat_rows.borrow().contains(c))
        .map(|(_, c)| c);
    *ui.filter.borrow_mut() = filter.clone().map(|c| (href.clone(), c));
    ui.cat.set_visible(filter.is_none());
    let pos = filter
        .as_ref()
        .and_then(|f| ui.cat_rows.borrow().iter().position(|c| c == f));
    match pos {
        Some(i) => ui
            .cat_list
            .select_row(ui.cat_list.row_at_index(i as i32).as_ref()),
        None => ui.cat_list.unselect_all(),
    }
    let shown = |c: Option<&String>| filter.as_ref().is_none_or(|f| c == f.as_ref());

    // "group": a section per category; "tags" / "none": one list in overall order.
    let view = core().setting("category_view".into()).unwrap_or_default();
    let grouping = view != "tags" && view != "none";
    ui.tags.set(view == "tags");
    let groups = if grouping {
        groups
    } else {
        vec![CategoryGroup {
            name: None,
            icon: None,
            tasks: core().ungrouped_by(href.clone(), order.tasks, order.subtasks, done_view),
        }]
    };

    // Pinned top-level tasks first, in a group of their own, in the list's overall order; the
    // sidebar filter goes by each top-level task's category (a group's name in "group").
    // With pins turned off, pinned tasks show among the others.
    let pins = prefs::pins(core());
    let all_open = core().ungrouped_by(href.clone(), order.tasks, order.subtasks, done_view);
    let pinned: Vec<_> = tasks::trees(&all_open)
        .into_iter()
        .filter(|(root, _)| pins && root.pinned && shown(root.category.as_ref()))
        .collect();
    let mut sections: Vec<(Option<String>, Vec<_>)> = Vec::new();
    for g in &groups {
        let rest: Vec<_> = tasks::trees(&g.tasks)
            .into_iter()
            .filter(|(root, _)| !(pins && root.pinned) && shown(root.category.as_ref()))
            .collect();
        if !rest.is_empty() {
            sections.push((g.name.clone(), rest));
        }
    }
    // A list without categories is one plain list; otherwise every group, "Other" too, has a
    // header. Under "Pinned", the rest of a plain list is headed "Tasks".
    let titled = grouping && !names.is_empty();
    let any = !pinned.is_empty() || !sections.is_empty();
    let add_trees = |group: &adw::PreferencesGroup,
                     trees: &[(&Task, Vec<(usize, &Task)>)],
                     cat: Option<&Option<String>>| {
        for (i, (root, subs)) in trees.iter().enumerate() {
            let slot = Slot {
                // In Pinned (no `cat`), each its own category.
                group: cat.cloned().unwrap_or_else(|| root.category.clone()),
                pinned: cat.is_none(),
                root: true,
                next: trees.get(i + 1).map(|(t, _)| t.uid.clone()),
            };
            let adding = ui.adding.borrow().as_ref() == Some(&root.uid);
            if subs.is_empty() && !adding {
                group.add(&leaf_row(ui, root, slot));
            } else {
                if adding && !subs.is_empty() {
                    ui.adding.take(); // the first subtask exists; from now on subtasks decide
                }
                group.add(&tree_row(ui, root, subs, order.sub_start, slot));
            }
        }
    };
    if !pinned.is_empty() {
        let group = adw::PreferencesGroup::builder()
            .title(gettext("Pinned"))
            .build();
        add_trees(&group, &pinned, None);
        ui.groups.append(&group);
    }
    for (name, trees) in &sections {
        let group = adw::PreferencesGroup::new();
        if titled {
            group.set_title(&glib::markup_escape_text(
                &name.clone().unwrap_or_else(other),
            ));
        } else if !pinned.is_empty() {
            group.set_title(&gettext("Tasks"));
        }
        add_trees(&group, trees, Some(name));
        ui.groups.append(&group);
    }

    let mut done: Vec<Task> = core()
        .finished(href, done_view)
        .into_iter()
        .filter(|t| shown(t.category.as_ref()))
        .collect();
    if !done.is_empty() {
        done.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
        let flash = ui.flash.borrow().clone().map(|(uid, _)| uid);
        let exp = adw::ExpanderRow::builder()
            .title(gettext("Completed ({n})").replace("{n}", &done.len().to_string()))
            .expanded(done.iter().any(|t| Some(&t.uid) == flash.as_ref()))
            .build();
        for t in done.iter().take(DONE_SHOWN) {
            exp.add_row(&task_row(ui, t, 0, None));
        }
        let group = adw::PreferencesGroup::new();
        group.add(&exp);
        ui.groups.append(&group);
    }
    if !any {
        let empty = adw::StatusPage::builder()
            .icon_name("checkbox-checked-symbolic")
            .title(gettext("All Done"))
            .build();
        empty.add_css_class("compact");
        ui.groups.prepend(&empty);
    }
}

/// Open tasks with a due date in every list, by day: overdue, today, tomorrow, the week ahead,
/// later. Rows show their list instead of their note.
fn rebuild_upcoming(ui: &Rc<Ui>) {
    let tasks = core().upcoming();
    let now = core().local_now();
    let names: HashMap<String, String> = core()
        .lists()
        .into_iter()
        .map(|l| (l.href, l.display_name))
        .collect();
    ui.title.set_title(&gettext("Upcoming"));
    ui.title.set_subtitle(&open_count(tasks.len() as u64));
    // Today's events first: the day's plan next to what is due.
    let events = todav_app::calendar::today_events(core());
    if !events.is_empty() {
        let group = adw::PreferencesGroup::builder()
            .title(gettext("Today’s Events"))
            .build();
        for e in &events {
            let time = if e.all_day {
                gettext("All day")
            } else {
                let at = |t: i64| {
                    dates::fields(t)
                        .format("%H:%M")
                        .map(String::from)
                        .unwrap_or_default()
                };
                format!("{} – {}", at(e.start), at(e.end))
            };
            let row = adw::ActionRow::builder()
                .title(&e.summary)
                .subtitle(&time)
                .use_markup(false)
                .activatable(true)
                .build();
            row.add_prefix(&gtk::Image::from_icon_name("x-office-calendar-symbolic"));
            if now >= e.start && now < e.end && !e.all_day {
                row.add_css_class("accent"); // happening now
            }
            let start = e.start;
            row.connect_activated(move |_| crate::calendar::show_day(start));
            group.add(&row);
        }
        ui.groups.append(&group);
    }
    let mut section: Option<(String, adw::PreferencesGroup)> = None;
    for t in &tasks {
        let s = dates::section(t, now);
        if section.as_ref().is_none_or(|(cur, _)| *cur != s) {
            let group = adw::PreferencesGroup::builder().title(&s).build();
            ui.groups.append(&group);
            section = Some((s, group));
        }
        let row = task_row(ui, t, 0, None);
        row.set_subtitle(names.get(&t.list_href).map_or("", String::as_str));
        section.as_ref().unwrap().1.add(&row);
    }
    if tasks.is_empty() {
        let empty = adw::StatusPage::builder()
            .icon_name("x-office-calendar-symbolic")
            .title(gettext("Nothing Scheduled"))
            .description(gettext("Tasks with a due date show up here"))
            .build();
        empty.add_css_class("compact");
        ui.groups.append(&empty);
    }
}

/// Open tasks of high priority (starred in Nextcloud Tasks) from every list: a group per list,
/// highest first.
fn rebuild_important(ui: &Rc<Ui>) {
    let tasks = core().important();
    ui.title.set_title(&gettext("Important"));
    ui.title.set_subtitle(&open_count(tasks.len() as u64));
    for l in todav_app::lists::by_place(core(), core().lists()) {
        let here: Vec<&Task> = tasks.iter().filter(|t| t.list_href == l.href).collect();
        if here.is_empty() {
            continue;
        }
        let group = adw::PreferencesGroup::builder()
            .title(glib::markup_escape_text(&l.display_name))
            .build();
        for t in here {
            group.add(&task_row(ui, t, 0, None));
        }
        ui.groups.append(&group);
    }
    if tasks.is_empty() {
        let empty = adw::StatusPage::builder()
            .icon_name("starred-symbolic")
            .title(gettext("No Important Tasks"))
            .description(gettext("Tasks of high priority show up here"))
            .build();
        empty.add_css_class("compact");
        ui.groups.append(&empty);
    }
}

/// Tasks of `all` whose title, notes or category hold `query` (`folded`): a group per list,
/// open ones first.
fn rebuild_search(ui: &Rc<Ui>, query: &str, all: Vec<Task>) {
    let hit = |t: &Task| {
        [
            Some(&t.summary),
            t.description.as_ref(),
            t.category.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|s| folded(s).contains(query))
    };
    let found: Vec<Task> = all.into_iter().filter(hit).collect();
    ui.title.set_title(&gettext("Search"));
    let n = found.len() as u64;
    ui.title
        .set_subtitle(&ngettext("{n} task", "{n} tasks", n).replace("{n}", &n.to_string()));
    for l in core().lists() {
        let mut here: Vec<&Task> = found.iter().filter(|t| t.list_href == l.href).collect();
        if here.is_empty() {
            continue;
        }
        here.sort_by_key(|t| t.done); // stable: list order otherwise
        let group = adw::PreferencesGroup::builder()
            .title(glib::markup_escape_text(&l.display_name))
            .build();
        for t in here {
            group.add(&task_row(ui, t, 0, None));
        }
        ui.groups.append(&group);
    }
    if found.is_empty() {
        let empty = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title(gettext("No Results Found"))
            .description(gettext("Try a different search"))
            .build();
        empty.add_css_class("compact");
        ui.groups.append(&empty);
    }
}

/// "3 open": the open tasks of a list or of Upcoming, under the title.
fn open_count(n: u64) -> String {
    ngettext("{n} open", "{n} open", n).replace("{n}", &n.to_string())
}

/// The event a task is for, as a pill opening the calendar on its (next) day.
fn event_tag(t: &Task) -> Option<gtk::Button> {
    let e = core().event(t.event_uid.clone()?)?;
    // Ellipsized in narrow windows, so the full name is in the tooltip too.
    let content = adw::ButtonContent::builder()
        .icon_name("x-office-calendar-symbolic")
        .label(&e.summary)
        .can_shrink(true)
        .build();
    let when = dates::label(e.start, e.all_day, core().local_now());
    let b = gtk::Button::builder()
        .child(&content)
        .can_shrink(true)
        .valign(gtk::Align::Center)
        .tooltip_text(format!("{} · {when}", e.summary))
        .build();
    b.add_css_class("tag");
    b.add_css_class("event");
    b.connect_clicked(move |_| crate::calendar::show_day(e.start));
    Some(b)
}

/// "done/all" pill for a task with subtasks.
fn count_tag(ui: &Ui, t: &Task) -> Option<gtk::Label> {
    let (open, done) = *ui.counts.borrow().get(&t.uid)?;
    let all = open + done;
    let l = gtk::Label::builder()
        .label(format!("{done}/{all}"))
        .tooltip_text(
            ngettext(
                "{done} of {all} subtask done",
                "{done} of {all} subtasks done",
                all as u64,
            )
            .replace("{done}", &done.to_string())
            .replace("{all}", &all.to_string()),
        )
        .valign(gtk::Align::Center)
        .build();
    l.add_css_class("tag");
    l.add_css_class("count");
    l.add_css_class("numeric");
    Some(l)
}

/// A warning on a task the server refused, saying why; it stays until a version goes up.
pub fn sync_warning(t: &Task) -> Option<gtk::Image> {
    let error = t.sync_error.as_deref()?;
    let w = gtk::Image::builder()
        .icon_name("dialog-warning-symbolic")
        .tooltip_text(
            gettext("Not saved on the server: {reason}").replace("{reason}", server_reason(error)),
        )
        .valign(gtk::Align::Center)
        .build();
    w.add_css_class("warning");
    Some(w)
}

/// `indent` levels of left margin on the whole row, for subtasks; `slot` makes it draggable.
fn task_row(ui: &Rc<Ui>, t: &Task, indent: usize, slot: Option<Slot>) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&t.summary)
        .use_markup(false)
        .activatable(true)
        .build();
    row.set_margin_start(INDENT * indent as i32);
    flash_if(ui, &row, &t.uid);
    // Shown among open tasks (a subtask, or kept in place): struck through. Rows in the
    // "Completed" group have no slot and stay plain.
    if t.done && slot.is_some() {
        row.add_css_class("task-done");
    }
    row.add_prefix(&done_check(t));
    // Top-level rows of a list page can be pinned.
    let pinnable = slot.as_ref().is_some_and(|s| s.root) && prefs::pins(core());
    if let Some(slot) = slot {
        // ActionRow puts each new prefix to the left of the previous ones (ExpanderRow: right).
        connect_dnd(ui, &row, t, slot, |h| row.add_prefix(h));
    }
    if let Some(d) = subtitle(t) {
        row.set_subtitle(d);
    }
    if let Some(w) = sync_warning(t) {
        row.add_suffix(&w);
    }
    if let Some(n) = count_tag(ui, t) {
        row.add_suffix(&n);
    }
    if let Some(due) = dates::tag(t, core().local_now()) {
        row.add_suffix(&due);
    }
    if let Some(e) = event_tag(t) {
        row.add_suffix(&e);
    }
    // Subtasks have no category of their own: the top-level task's decides. The Upcoming page
    // and search results mix lists, so they leave categories alone.
    let mixed = ui
        .current
        .borrow()
        .as_deref()
        .is_some_and(|c| PAGES.contains(&c))
        || !ui.query.borrow().is_empty();
    let sub = indent > 0 || mixed;
    if let Some(tag) = tag(ui, t).filter(|_| indent == 0) {
        row.add_suffix(&tag);
    }
    if pinnable {
        row.add_suffix(&pin_button(t));
    }
    if prefs::stars(core()) {
        row.add_suffix(&star_button(t));
    }
    row.add_suffix(&delete_button(t));
    inline_title(ui, row.upcast_ref(), t);
    let (weak, task) = (Rc::downgrade(ui), t.clone());
    row.connect_activated(move |_| {
        if let Some(ui) = weak.upgrade() {
            edit_dialog(&ui, &task, sub);
        }
    });
    row
}

/// With `inline_edit` on, a click on the title text of task `t`'s `row` with a mouse or touchpad
/// edits it in place, a text cursor showing over it; a tap, or a click beside it, still opens the
/// task (or its subtasks). Enter or clicking elsewhere saves, Escape cancels.
fn inline_title(ui: &Rc<Ui>, row: &gtk::Widget, t: &Task) {
    if !ui.inline.get() {
        return;
    }
    // The row's title (the header's, for an expander row: it comes first).
    let Some(label) = descendants(row)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Label>().ok())
        .find(|l| l.has_css_class("title") && l.label() == t.summary)
    else {
        return;
    };
    // Where in the text (x, y in `row`) is: its byte index, or None beside it.
    let at = {
        let (row, label) = (row.clone(), label.clone());
        move |x: f64, y: f64| {
            let p = row.compute_point(&label, &gtk::graphene::Point::new(x as f32, y as f32))?;
            let (ox, oy) = label.layout_offsets();
            let layout = label.layout();
            let (_, r) = layout.pixel_extents();
            let (x, y) = (p.x() as i32 - ox, p.y() as i32 - oy);
            let inside =
                x >= r.x() && x < r.x() + r.width() && y >= r.y() && y < r.y() + r.height();
            let s = gtk::pango::SCALE;
            inside.then(|| layout.xy_to_index(x * s, y * s).1 as usize)
        }
    };
    let pointer = |d: Option<gtk::gdk::Device>| {
        use gtk::gdk::InputSource::{Mouse, Touchpad, Trackpoint};
        d.is_some_and(|d| matches!(d.source(), Mouse | Touchpad | Trackpoint))
    };
    let motion = gtk::EventControllerMotion::new();
    let (l, on) = (label.clone(), at.clone());
    motion.connect_motion(move |m, x, y| {
        let text = pointer(m.current_event_device()) && on(x, y).is_some();
        l.set_cursor_from_name(text.then_some("text"));
    });
    let l = label.clone();
    motion.connect_leave(move |_| l.set_cursor(None));
    row.add_controller(motion);
    // On release, so dragging the row by its title still works; claimed, the row is not opened.
    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (weak, task) = (Rc::downgrade(ui), t.clone());
    click.connect_released(move |g, n, x, y| {
        let Some(ui) = weak.upgrade().filter(|ui| !ui.renaming.get()) else {
            return;
        };
        if n != 1 || !pointer(g.current_event_device()) {
            return;
        }
        if let Some(index) = at(x, y) {
            g.set_state(gtk::EventSequenceState::Claimed);
            edit_title(&ui, &label, &task, index);
        }
    });
    row.add_controller(click);
}

/// Edit task `t`'s title in place of its row's `label`, the cursor at byte `index` of it.
fn edit_title(ui: &Rc<Ui>, label: &gtk::Label, t: &Task, index: usize) {
    let Some(parent) = label.parent().and_downcast::<gtk::Box>() else {
        return;
    };
    let text = gtk::Text::builder().text(&t.summary).hexpand(true).build();
    parent.insert_child_after(&text, Some(label));
    label.set_visible(false);
    ui.renaming.set(true);
    let ended = Rc::new(Cell::new(false));
    let finish = {
        let (ui, label, text, t) = (ui.clone(), label.clone(), text.clone(), t.clone());
        move |save: bool| {
            if ended.replace(true) {
                return;
            }
            let summary = text.text().trim().to_string();
            ui.renaming.set(false);
            // Not from within the text's own signal handler.
            let (parent, label, text, ui2) =
                (parent.clone(), label.clone(), text.clone(), ui.clone());
            glib::idle_add_local_once(move || {
                parent.remove(&text);
                label.set_visible(true);
                if ui2.stale.take() {
                    refresh_tasks(&ui2);
                }
            });
            if !save || summary.is_empty() || summary == t.summary {
                return;
            }
            let patch = TaskPatch {
                summary: Some(summary),
                ..Default::default()
            };
            match core().update_task(t.uid.clone(), patch) {
                Ok(()) => request_sync(),
                Err(e) => ui.toasts.add_toast(note(&e.to_string())),
            }
        }
    };
    let finish = Rc::new(finish);
    let f = finish.clone();
    text.connect_activate(move |_| f(true));
    let keys = gtk::EventControllerKey::new();
    let f = finish.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key != gtk::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        f(false);
        glib::Propagation::Stop
    });
    text.add_controller(keys);
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave(move |_| finish(true));
    text.add_controller(focus);
    text.grab_focus_without_selecting();
    let chars = t.summary.get(..index).map_or(0, |s| s.chars().count());
    text.set_position(chars as i32);
}

/// A root task without subtasks: its "+" turns it into a `tree_row` ready for the first one.
fn leaf_row(ui: &Rc<Ui>, t: &Task, slot: Slot) -> adw::ActionRow {
    let row = task_row(ui, t, 0, Some(slot));
    row.add_suffix(&add_button(ui, t));
    row
}

/// "+" giving task `t` an "Add subtask" entry, and taking tasks dropped on it as its subtasks.
fn add_button(ui: &Rc<Ui>, t: &Task) -> gtk::Button {
    let add = icon_button("list-add-symbolic", &gettext("Add Subtask"));
    let (weak, uid) = (Rc::downgrade(ui), t.uid.clone());
    add.connect_clicked(move |_| {
        let Some(ui) = weak.upgrade() else { return };
        expand(&ui, &uid, true);
        *ui.adding.borrow_mut() = Some(uid.clone());
        *ui.focus_sub.borrow_mut() = Some(uid.clone());
        // Rebuild outside the handler: refresh_tasks replaces this row.
        glib::idle_add_local_once(move || refresh_tasks(&ui));
    });
    adopt_target(ui, &add, t, None);
    add
}

/// A root task with its subtasks and an entry for adding more.
/// `start`: new subtasks go first, so the entry for them comes first too.
fn tree_row(
    ui: &Rc<Ui>,
    root: &Task,
    subs: &[(usize, &Task)],
    start: bool,
    slot: Slot,
) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::builder()
        .title(&root.summary)
        .use_markup(false)
        .expanded(ui.expanded.borrow().contains(&root.uid))
        .build();
    flash_if(ui, &row, &root.uid);
    if root.done {
        row.add_css_class("task-done"); // kept in place
    }
    let (slot_group, slot_pinned) = (slot.group.clone(), slot.pinned);
    connect_dnd(ui, &row, root, slot, |h| row.add_prefix(h));
    row.add_prefix(&done_check(root));
    if let Some(d) = subtitle(root) {
        row.set_subtitle(d);
    }
    // Collapsed, the header shows no subtask list to drop on: a "+" appears during drags.
    let plus = icon_button("list-add-symbolic", &gettext("Add Subtask"));
    plus.set_visible(false);
    row.add_suffix(&plus);
    adopt_target(ui, &plus, root, Some(&row));
    // The header toggles the subtasks, so editing gets its own button.
    let edit = icon_button("document-edit-symbolic", &gettext("Edit"));
    let (weak, task) = (Rc::downgrade(ui), root.clone());
    edit.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            edit_dialog(&ui, &task, false);
        }
    });
    row.add_suffix(&edit);
    row.add_suffix(&delete_button(root));
    // ExpanderRow puts each new suffix to the left of the previous ones.
    if prefs::stars(core()) {
        row.add_suffix(&star_button(root));
    }
    if prefs::pins(core()) {
        row.add_suffix(&pin_button(root));
    }
    if let Some(tag) = tag(ui, root) {
        row.add_suffix(&tag);
    }
    if let Some(due) = dates::tag(root, core().local_now()) {
        row.add_suffix(&due);
    }
    if let Some(n) = count_tag(ui, root) {
        row.add_suffix(&n);
    }
    if let Some(w) = sync_warning(root) {
        row.add_suffix(&w);
    }
    inline_title(ui, row.upcast_ref(), root);
    let (weak, uid) = (Rc::downgrade(ui), root.uid.clone());
    let empty = subs.is_empty();
    row.connect_expanded_notify(move |r| {
        let Some(ui) = weak.upgrade() else { return };
        expand(&ui, &uid, r.is_expanded());
        if r.is_expanded() {
            return;
        }
        // Closed before adding a first subtask: back to a plain row with "+".
        if empty && ui.adding.borrow().as_ref() == Some(&uid) {
            ui.adding.take();
            glib::idle_add_local_once(move || refresh_tasks(&ui));
        }
    });

    if start {
        row.add_row(&sub_entry(ui, root, 0, start));
    }
    // A subtask given an entry by its "+".
    let at = (ui.adding.borrow().as_deref()).and_then(|uid| tasks::entry_at(subs, uid, start));
    let nested_entry = |p: usize| sub_entry(ui, subs[p].1, subs[p].0, start);
    for (i, (depth, t)) in subs.iter().enumerate() {
        if let Some((_, p)) = at.filter(|a| a.0 == i) {
            row.add_row(&nested_entry(p));
        }
        let slot = Slot {
            group: slot_group.clone(),
            pinned: slot_pinned,
            root: false,
            next: subs[i + 1..]
                .iter()
                .find(|(_, s)| s.parent_uid == t.parent_uid)
                .map(|(_, s)| s.uid.clone()),
        };
        let sub = task_row(ui, t, *depth, Some(slot));
        sub.add_suffix(&add_button(ui, t));
        row.add_row(&sub);
    }
    if let Some((_, p)) = at.filter(|a| a.0 == subs.len()) {
        row.add_row(&nested_entry(p));
    }
    if !start {
        row.add_row(&sub_entry(ui, root, 0, start));
    }
    row
}

/// "Add subtask" entry under `parent`, a task `depth` levels deep; new subtasks go first with
/// `start`. Under a subtask, Escape closes it. A task dropped on it goes where a new subtask
/// would.
fn sub_entry(ui: &Rc<Ui>, parent: &Task, depth: usize, start: bool) -> adw::EntryRow {
    let entry = adw::EntryRow::builder()
        .title(gettext("Add subtask"))
        .build();
    entry.set_margin_start(INDENT * (depth as i32 + 1));
    entry.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
    let p = parent.clone();
    let drop = task_drop(ui, move |d| tasks::can_join(core(), d, &p), start);
    let (weak, p) = (Rc::downgrade(ui), parent.clone());
    drop.connect_drop(move |dt, _, _, _| {
        let (Some(ui), Some(w)) = (weak.upgrade(), dt.widget()) else {
            return false;
        };
        unmark(&ui, &w);
        let Some((d, _)) = ui
            .dragging
            .take()
            .filter(|(d, _)| tasks::can_join(core(), d, &p))
        else {
            return false;
        };
        match tasks::adopt(core(), &d, &p) {
            Ok(()) => request_sync(),
            Err(e) => ui.toasts.add_toast(note(&e.to_string())),
        }
        true
    });
    entry.add_controller(drop);
    if ui.focus_sub.borrow().as_ref() == Some(&parent.uid) {
        ui.focus_sub.take();
        let e = entry.clone();
        glib::idle_add_local_once(move || {
            e.grab_focus();
        });
    }
    if depth > 0 {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(ui);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key != gtk::gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            if let Some(ui) = weak.upgrade() {
                ui.adding.take();
                glib::idle_add_local_once(move || refresh_tasks(&ui));
            }
            glib::Propagation::Stop
        });
        entry.add_controller(keys);
    }
    let (weak, parent) = (Rc::downgrade(ui), parent.clone());
    entry.connect_entry_activated(move |e| {
        let Some(ui) = weak.upgrade() else { return };
        let text = e.text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let p = Some(parent.uid.clone());
        match core().add_task(parent.list_href.clone(), text, None, p, start) {
            Ok(_) => {
                e.set_text("");
                *ui.focus_sub.borrow_mut() = Some(parent.uid.clone()); // keep typing after the rebuild
                request_sync();
            }
            Err(err) => ui.toasts.add_toast(note(&err.to_string())),
        }
    });
    entry
}

/// Make `row` draggable (by the whole row, or by a handle passed to `add_handle`) and a drop
/// target for other tasks: above or below it, depending on which half the pointer is over.
fn connect_dnd(
    ui: &Rc<Ui>,
    row: &impl IsA<gtk::Widget>,
    t: &Task,
    slot: Slot,
    add_handle: impl FnOnce(&gtk::Image),
) {
    row.add_css_class("task-dnd");
    let drag = gtk::DragSource::new();
    drag.set_actions(gtk::gdk::DragAction::MOVE);
    let (weak, task) = (Rc::downgrade(ui), t.clone());
    drag.connect_prepare(move |src, x, y| {
        // Without a handle the whole row drags, except from the "Add subtask" entry inside it
        // and from a subtask row inside it: that one starts its own drag, and a second drag
        // from the parent gets cancelled, whose drag-end would clear `dragging`.
        if nested_row(&src.widget()?, x, y) {
            return None;
        }
        Some(gtk::gdk::ContentProvider::for_value(&task.uid.to_value()))
    });
    // Recorded on begin, not prepare: prepare also runs for drags that never start.
    let r = row.clone().upcast::<gtk::Widget>();
    let (task, s) = (t.clone(), slot.clone());
    drag.connect_drag_begin(move |_, drag| {
        if let Some(ui) = weak.upgrade() {
            *ui.dragging.borrow_mut() = Some((task.clone(), s.clone()));
            for (w, parent, tree) in ui.adopters.borrow().iter() {
                if tasks::can_adopt(core(), &task, parent)
                    && tree.as_ref().is_none_or(|t| !t.is_expanded())
                {
                    w.set_visible(true);
                    w.add_css_class("drop-hint");
                }
            }
        }
        // The task's title on a card; the row stays in place, faded.
        let card = gtk::Label::new(Some(&task.summary));
        card.add_css_class("drag-card");
        if let Ok(icon) = gtk::DragIcon::for_drag(drag).downcast::<gtk::DragIcon>() {
            icon.set_child(Some(&card));
        }
        r.add_css_class("dragged");
    });
    let (weak, r) = (Rc::downgrade(ui), row.clone().upcast::<gtk::Widget>());
    drag.connect_drag_end(move |_, _, _| {
        r.remove_css_class("dragged");
        if let Some(ui) = weak.upgrade() {
            ui.dragging.take();
            for (w, _, tree) in ui.adopters.borrow().iter() {
                w.remove_css_class("drop-hint");
                if tree.is_some() {
                    w.set_visible(false);
                }
            }
            if let Some((_, w, class)) = ui.drop_mark.take() {
                w.remove_css_class(class); // cancelled drags get no leave
            }
        }
    });
    if ui.handles.get() {
        let handle = gtk::Image::from_icon_name("list-drag-handle-symbolic");
        handle.set_tooltip_text(Some(&gettext("Drag to Move")));
        handle.set_cursor_from_name(Some("grab"));
        handle.add_controller(drag);
        add_handle(&handle);
    } else {
        row.add_controller(drag);
    }

    let drop = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::MOVE);
    let (weak, target, s) = (Rc::downgrade(ui), t.clone(), slot.clone());
    drop.connect_accept(move |_, _| {
        weak.upgrade().is_some_and(|ui| {
            let dragging = ui.dragging.borrow();
            dragging
                .as_ref()
                .is_some_and(|(d, ds)| tasks::can_drop(core(), d, ds, &target, &s))
        })
    });
    let (weak, target, s) = (Rc::downgrade(ui), t.clone(), slot.clone());
    drop.connect_motion(move |dt, x, y| {
        if let (Some(ui), Some(w)) = (weak.upgrade(), dt.widget()) {
            let above = y < w.height() as f64 / 2.0;
            let stays = ui
                .dragging
                .borrow()
                .as_ref()
                .is_some_and(|(d, ds)| tasks::stays_put(d, ds, &target, &s, above));
            if stays || over_nested(&s, &w, x, y) {
                unmark(&ui, &w);
            } else {
                mark(&ui, &w, !above);
            }
        }
        gtk::gdk::DragAction::MOVE
    });
    let weak = Rc::downgrade(ui);
    drop.connect_leave(move |dt| {
        if let (Some(ui), Some(w)) = (weak.upgrade(), dt.widget()) {
            unmark(&ui, &w);
        }
    });
    let (weak, target) = (Rc::downgrade(ui), t.clone());
    drop.connect_drop(move |dt, _, x, y| {
        let (Some(ui), Some(w)) = (weak.upgrade(), dt.widget()) else {
            return false;
        };
        unmark(&ui, &w);
        if over_nested(&slot, &w, x, y) {
            return false;
        }
        let Some((d, ds)) = ui.dragging.take() else {
            return false;
        };
        let above = y < w.height() as f64 / 2.0;
        match tasks::drop_task(core(), &d, &ds, &target, &slot, above) {
            Ok(()) => request_sync(),
            Err(e) => ui.toasts.add_toast(note(&e.to_string())),
        }
        true
    });
    row.add_controller(drop);
}

/// A drop target for an entry, taking dragged tasks before its text does (which would insert
/// the task's UID): those `ok` takes get a line above the entry, or `below` it; others are refused.
fn task_drop(ui: &Rc<Ui>, ok: impl Fn(&Task) -> bool + 'static, below: bool) -> gtk::DropTarget {
    let drop = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::MOVE);
    drop.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = Rc::downgrade(ui);
    drop.connect_accept(move |_, _| {
        weak.upgrade()
            .is_some_and(|ui| ui.dragging.borrow().is_some())
    });
    let weak = Rc::downgrade(ui);
    drop.connect_motion(move |dt, _, _| {
        let (Some(ui), Some(w)) = (weak.upgrade(), dt.widget()) else {
            return gtk::gdk::DragAction::empty();
        };
        if ui.dragging.borrow().as_ref().is_some_and(|(d, _)| ok(d)) {
            mark(&ui, &w, below);
            gtk::gdk::DragAction::MOVE
        } else {
            unmark(&ui, &w);
            gtk::gdk::DragAction::empty()
        }
    });
    let weak = Rc::downgrade(ui);
    drop.connect_leave(move |dt| {
        if let (Some(ui), Some(w)) = (weak.upgrade(), dt.widget()) {
            unmark(&ui, &w);
        }
    });
    drop
}

/// Is the pointer over something nested in row `w` with its own drag or drop handling: a
/// subtask row, an "Add subtask" entry or a "+" button?
fn nested_row(w: &gtk::Widget, x: f64, y: f64) -> bool {
    let mut p = w.pick(x, y, gtk::PickFlags::DEFAULT);
    while let Some(c) = p.filter(|c| c != w) {
        if c.is::<adw::EntryRow>() || c.has_css_class("task-dnd") || c.has_css_class("adopt") {
            return true;
        }
        p = c.parent();
    }
    false
}

/// A top-level row leaves drops over its subtasks and "+" to those.
fn over_nested(ts: &Slot, w: &gtk::Widget, x: f64, y: f64) -> bool {
    ts.root && nested_row(w, x, y)
}

/// Show where a drop on `owner` lands. Below it is drawn as the line above the next row when
/// there is one, so the gap between two rows has a single line.
fn mark(ui: &Ui, owner: &gtk::Widget, below: bool) {
    let (w, class) = match owner.next_sibling().filter(|_| below) {
        Some(next) => (next, "drop-above"),
        None => (
            owner.clone(),
            if below { "drop-below" } else { "drop-above" },
        ),
    };
    let mut m = ui.drop_mark.borrow_mut();
    if let Some((_, old, c)) = m.take() {
        old.remove_css_class(c);
    }
    w.add_css_class(class);
    *m = Some((owner.clone(), w, class));
}

/// Remove the line `owner` drew; a row already marked by the one now entered stays.
fn unmark(ui: &Ui, owner: &gtk::Widget) {
    let mut m = ui.drop_mark.borrow_mut();
    if m.as_ref().is_some_and(|(o, _, _)| o == owner) {
        let (_, w, c) = m.take().unwrap();
        w.remove_css_class(c);
    }
}

/// Dropping a task on `w` (a "+" button) makes it a subtask of `parent`. With `collapsed`, the
/// button belongs to that tree row and only shows during a drag while the row is collapsed.
fn adopt_target(
    ui: &Rc<Ui>,
    w: &impl IsA<gtk::Widget>,
    parent: &Task,
    collapsed: Option<&adw::ExpanderRow>,
) {
    w.add_css_class("adopt");
    ui.adopters
        .borrow_mut()
        .push((w.clone().upcast(), parent.clone(), collapsed.cloned()));
    let drop = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::MOVE);
    let (weak, p) = (Rc::downgrade(ui), parent.clone());
    drop.connect_accept(move |_, _| {
        weak.upgrade().is_some_and(|ui| {
            let dragging = ui.dragging.borrow();
            dragging
                .as_ref()
                .is_some_and(|(d, _)| tasks::can_adopt(core(), d, &p))
        })
    });
    let (weak, p) = (Rc::downgrade(ui), parent.clone());
    drop.connect_drop(move |_, _, _, _| {
        let Some(ui) = weak.upgrade() else {
            return false;
        };
        let Some((d, _)) = ui.dragging.take() else {
            return false;
        };
        match tasks::adopt(core(), &d, &p) {
            Ok(()) => {
                expand(&ui, &p.uid, true); // show where it went
                request_sync();
            }
            Err(e) => ui.toasts.add_toast(note(&e.to_string())),
        }
        true
    });
    w.add_controller(drop);
}

/// Category pill for the "Show as tags" view.
fn tag(ui: &Ui, t: &Task) -> Option<gtk::Label> {
    let c = t.category.as_deref().filter(|_| ui.tags.get())?;
    let l = gtk::Label::builder()
        .label(c)
        .valign(gtk::Align::Center)
        .build();
    l.add_css_class("tag");
    Some(l)
}

/// First line of the note; rows use plain text (`use_markup(false)`), so no escaping.
fn subtitle(t: &Task) -> Option<&str> {
    Some(t.description.as_deref()?.lines().next().unwrap_or_default())
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .icon_name(icon)
        .valign(gtk::Align::Center)
        .tooltip_text(tooltip)
        .build();
    b.add_css_class("flat");
    b
}

fn done_check(t: &Task) -> gtk::CheckButton {
    let check = gtk::CheckButton::builder()
        .active(t.done)
        .valign(gtk::Align::Center)
        .build();
    let uid = t.uid.clone();
    check.connect_toggled(move |c| tick(c, &uid, || {}));
    check
}

/// Complete or reopen task `uid` as its check `check` now says, then `after`. Completed, its open
/// subtasks are completed too, stay open or the user picks, as Preferences say
/// (`complete_subtasks`); reopened, the user picks whether its completed subtasks reopen too,
/// every time. Cancelled, the check goes back.
pub fn tick(check: &gtk::CheckButton, uid: &str, after: impl Fn() + 'static) {
    use prefs::SubtaskCompletion::{Ask, Complete};
    let done = check.is_active();
    let Some(t) = core().task(uid.into()).filter(|t| t.done != done) else {
        return; // already so: put back after a cancel, or rebuilt
    };
    let mode = prefs::subtask_completion(core());
    // Completing them all, reopening one reopens its completed parents.
    let parents = !done && mode == Complete;
    let finish = move |subtasks: bool| {
        if tasks::set_done(core(), &t.uid, done, subtasks, parents).is_ok() {
            after();
            request_sync();
        }
    };
    // The subtasks left behind otherwise: open ones when completing, completed ones when reopening.
    let n = tasks::subtasks(core(), uid, !done).len();
    if n == 0 {
        return finish(false);
    }
    if done && mode != Ask {
        return finish(mode == Complete);
    }
    let check = check.clone();
    glib::spawn_future_local(async move {
        let (heading, body, all) = if done {
            (
                gettext("Complete Subtasks Too?"),
                ngettext(
                    "“{title}” has {n} open subtask.",
                    "“{title}” has {n} open subtasks.",
                    n as u64,
                ),
                gettext("Complete All"),
            )
        } else {
            (
                gettext("Reopen Subtasks Too?"),
                ngettext(
                    "“{title}” has {n} completed subtask.",
                    "“{title}” has {n} completed subtasks.",
                    n as u64,
                ),
                gettext("Reopen All"),
            )
        };
        let body = body
            .replace("{title}", &t.summary)
            .replace("{n}", &n.to_string());
        let alert = adw::AlertDialog::new(Some(&heading), Some(&body));
        alert.add_responses(&[
            ("cancel", &gettext("Cancel")),
            ("alone", &gettext("Only This Task")),
            ("all", &all),
        ]);
        alert.set_response_appearance("all", adw::ResponseAppearance::Suggested);
        alert.set_default_response(Some("all"));
        alert.set_close_response("cancel");
        match alert.choose_future(Some(&check)).await.as_str() {
            "all" => finish(true),
            "alone" => finish(false),
            _ => check.set_active(!done),
        }
    });
}

/// Priority levels as the star's menu and the edit dialog's picker list them, by action target.
const PRIORITIES: [&str; 4] = ["none", "low", "medium", "high"];

/// A star coloured by the task's priority (high, medium, low as Nextcloud Tasks colours them;
/// outlined without one); its menu sets the priority as the edit dialog does.
fn star_button(t: &Task) -> gtk::MenuButton {
    let level = priority_row(t.priority);
    let b = gtk::MenuButton::builder()
        .icon_name(if level == 0 {
            "star-outline-symbolic"
        } else {
            "starred-symbolic"
        })
        .valign(gtk::Align::Center)
        .tooltip_text(priority_text(t.priority).unwrap_or_else(|| gettext("Priority")))
        .build();
    b.add_css_class("flat");
    // Without a priority, as grey as an unpinned pin.
    b.add_css_class(&match level {
        0 => "dim-label".to_string(),
        _ => format!("priority-{}", PRIORITIES[level as usize]),
    });
    let menu = gio::Menu::new();
    let labels = [
        pgettext("priority", "None"),
        pgettext("priority", "Low"),
        pgettext("priority", "Medium"),
        pgettext("priority", "High"),
    ];
    for (label, target) in labels.iter().zip(PRIORITIES) {
        menu.append(Some(label), Some(&format!("task.priority::{target}")));
    }
    b.set_menu_model(Some(&menu));
    let state = PRIORITIES[level as usize].to_variant();
    let action = gio::SimpleAction::new_stateful("priority", Some(glib::VariantTy::STRING), &state);
    let uid = t.uid.clone();
    action.connect_change_state(move |_, v| {
        let Some(row) = v
            .and_then(|v| v.str())
            .and_then(|s| PRIORITIES.iter().position(|p| *p == s))
        else {
            return;
        };
        if row as u32 == level {
            return;
        }
        let patch = TaskPatch {
            priority: Some(priority_value(row as u32)),
            ..Default::default()
        };
        if core().update_task(uid.clone(), patch).is_ok() {
            request_sync();
        }
    });
    let actions = gio::SimpleActionGroup::new();
    actions.add_action(&action);
    b.insert_action_group("task", Some(&actions));
    b
}

/// Pins a top-level task to the top of its list (X-PINNED, as Nextcloud Tasks pins).
fn pin_button(t: &Task) -> gtk::Button {
    let b = icon_button(
        "view-pin-symbolic",
        &if t.pinned {
            gettext("Unpin")
        } else {
            gettext("Pin")
        },
    );
    // Dimmed until pinned, as the outlined star is until starred.
    b.add_css_class(if t.pinned { "pinned" } else { "dim-label" });
    let (uid, pinned) = (t.uid.clone(), t.pinned);
    b.connect_clicked(move |_| {
        let patch = TaskPatch {
            pinned: Some(!pinned),
            ..Default::default()
        };
        if core().update_task(uid.clone(), patch).is_ok() {
            request_sync();
        }
    });
    b
}

fn delete_button(t: &Task) -> gtk::Button {
    let del = icon_button("user-trash-symbolic", &gettext("Delete"));
    let task = t.clone();
    del.connect_clicked(move |_| delete_task(&task));
    del
}

pub fn delete_task(t: &Task) {
    if core().delete_task(t.uid.clone()).is_ok() {
        request_sync();
        undo_toast(&gettext("“{title}” deleted").replace("{title}", &t.summary));
    }
}

/// Tell that something was deleted, with a button undoing just that (the latest change).
pub fn undo_toast(title: &str) {
    let Some(ui) = ui() else { return };
    let toast = adw::Toast::builder()
        .title(title)
        .button_label(gettext("Undo"))
        .priority(adw::ToastPriority::High)
        .build();
    toast.set_use_markup(false);
    let change = core().last_change();
    toast.connect_button_clicked(move |_| undo(change));
    ui.toasts.add_toast(toast);
}

/// Undo `change`, else the latest one.
fn undo(change: Option<u64>) {
    match core().undo(change) {
        Ok(true) => request_sync(),
        Ok(false) => {
            if let Some(ui) = ui() {
                ui.window.error_bell();
            }
        }
        Err(e) => toast(&e.to_string()),
    }
}

/// Ask for a category name; it joins this list's categories and becomes the picker's choice.
fn new_category_dialog(ui: &Rc<Ui>, created: impl Fn(&Rc<Ui>, String) + 'static) {
    let b = view!("new-category");
    let dialog: adw::AlertDialog = get(&b, "dialog");
    let name: adw::EntryRow = get(&b, "name");
    let d = dialog.clone();
    name.connect_changed(move |e| d.set_response_enabled("create", !e.text().trim().is_empty()));
    let weak = Rc::downgrade(ui);
    dialog.connect_response(Some("create"), move |_, _| {
        if let Some(ui) = weak.upgrade() {
            created(&ui, name.text().trim().to_string());
        }
    });
    dialog.present(Some(&ui.window));
}

/// Make the last item of `model` ("New Category…") ask for a name instead of being picked.
/// `picker` is a DropDown or ComboRow showing `model`; changes made while `busy` are ignored.
fn connect_new_category(
    ui: &Rc<Ui>,
    picker: &impl IsA<glib::Object>,
    model: &gtk::StringList,
    busy: Rc<Cell<bool>>,
    created: impl Fn(&Rc<Ui>, String) + 'static,
) {
    let prev = Cell::new(picker.property::<u32>("selected"));
    let (weak, model, created) = (Rc::downgrade(ui), model.clone(), Rc::new(created));
    picker.connect_notify_local(Some("selected"), move |p, _| {
        let sel = p.property::<u32>("selected");
        if sel + 1 != model.n_items() {
            prev.set(sel);
            return;
        }
        let Some(ui) = weak.upgrade().filter(|_| !busy.get()) else {
            return;
        };
        busy.set(true);
        p.set_property("selected", prev.get());
        busy.set(false);
        let created = created.clone();
        new_category_dialog(&ui, move |ui, name| created(ui, name));
    });
}

fn select_name(picker: &impl IsA<glib::Object>, model: &gtk::StringList, name: &str) {
    if let Some(i) = (0..model.n_items()).find(|&i| model.string(i).as_deref() == Some(name)) {
        picker.set_property("selected", i);
    }
}

/// `sub`: a subtask, which has no category to edit.
fn edit_dialog(ui: &Rc<Ui>, t: &Task, sub: bool) {
    let b = view!("edit-dialog");
    let dialog: adw::Dialog = get(&b, "dialog");
    let summary: adw::EntryRow = get(&b, "summary");
    let category: adw::ComboRow = get(&b, "category");
    let cats: gtk::StringList = get(&b, "cats");
    let description: gtk::TextView = get(&b, "description");
    summary.set_text(&t.summary);
    description
        .buffer()
        .set_text(t.description.as_deref().unwrap_or_default());

    // Same choices as the new-task picker, plus the task's own category if that lacks it.
    let items: Vec<glib::GString> = (0..ui.cats.n_items())
        .filter_map(|i| ui.cats.string(i))
        .collect();
    let mut items: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
    if let Some(c) = t.category.as_deref().filter(|c| !items[1..].contains(c)) {
        items.insert(items.len() - 1, c);
    }
    cats.splice(0, cats.n_items(), &items);
    category.set_visible(!sub);
    category.set_selected(0);
    if let Some(c) = &t.category {
        select_name(&category, &cats, c);
    }
    let busy = Rc::new(Cell::new(false));
    let (combo, list, uid) = (category.downgrade(), cats.clone(), t.uid.clone());
    let b2 = busy.clone();
    connect_new_category(ui, &category, &cats, busy, move |_, name| {
        // A category made from this dialog applies to the task right away.
        let patch = TaskPatch {
            category: Some(name.clone()),
            ..Default::default()
        };
        if core().update_task(uid.clone(), patch).is_ok() {
            request_sync();
        }
        b2.set(true);
        list.splice(list.n_items() - 1, 0, &[name.as_str()]);
        b2.set(false);
        if let Some(c) = combo.upgrade() {
            c.set_selected(list.n_items() - 2);
        }
    });
    if let Some(e) = &t.sync_error {
        get::<adw::ActionRow>(&b, "refused").set_subtitle(server_reason(e));
        get::<adw::PreferencesGroup>(&b, "refused_group").set_visible(true);
    }
    let (due_at, due) = due_dialog(&b, t);
    let old_start = t.start.map(|at| Due {
        at,
        all_day: t.start_all_day,
    });
    let start = date_row(&b, "start", old_start);
    // One time choice for both dates, as the server asks: switching either switches both. Linked
    // after loading, so a task that mixes them opens as it is (the banner says so).
    let (start_time, due_time): (gtk::Switch, gtk::Switch) =
        (get(&b, "start_has_time"), get(&b, "due_has_time"));
    let other = due_time.clone();
    start_time.connect_active_notify(move |s| other.set_active(s.is_active()));
    let other = start_time.clone();
    due_time.connect_active_notify(move |s| other.set_active(s.is_active()));
    // Times added to dates that had none: due the default length after the start (the same day
    // at the latest), rather than at the start itself.
    if t.due.is_none_or(|_| t.due_all_day) {
        let spins: [gtk::SpinButton; 4] =
            ["start_hour", "start_minute", "due_hour", "due_minute"].map(|id| get(&b, id));
        let start_row: adw::ExpanderRow = get(&b, "start");
        due_time.connect_active_notify(move |s| {
            if s.is_active() && start_row.enables_expansion() {
                let from = spins[0].value() as i64 * 3600 + spins[1].value() as i64 * 60;
                let to = (from + prefs::task_duration(core())).min(86400 - 60);
                spins[2].set_value((to / 3600) as f64);
                spins[3].set_value((to / 60 % 60) as f64);
            }
        });
    }
    let priority: adw::ComboRow = get(&b, "priority");
    let old_priority = priority_row(t.priority);
    priority.set_selected(old_priority);

    // The list it is in, among those it can move to, grouped by where they are kept.
    let list_row: adw::ComboRow = get(&b, "list");
    let lists = todav_app::lists::by_place(
        core(),
        core()
            .lists()
            .into_iter()
            .filter(|l| l.writable || l.href == t.list_href)
            .collect(),
    );
    crate::lists::picker(&list_row, &lists);
    let at = lists.iter().position(|l| l.href == t.list_href);
    list_row.set_selected(at.unwrap_or(0) as u32);
    list_row.set_visible(!sub);

    // The event it is for, with a way to unlink it.
    let event_row: adw::ActionRow = get(&b, "event");
    let linked = t.event_uid.clone().and_then(|e| core().event(e));
    if let Some(e) = &linked {
        event_row.set_subtitle(&e.summary);
        event_row.set_visible(true);
    }
    let unlinked = Rc::new(Cell::new(false));
    let (row, u) = (event_row.clone(), unlinked.clone());
    get::<gtk::Button>(&b, "unlink").connect_clicked(move |_| {
        u.set(true);
        row.set_visible(false);
    });

    let save: gtk::Button = get(&b, "save");
    // What iCalendar (and the server, which refuses otherwise) asks of both dates: the same kind,
    // and not due before the start.
    let conflict: adw::Banner = get(&b, "conflict");
    let check: Rc<dyn Fn()> = {
        let (start, due, save) = (start.clone(), due.clone(), save.clone());
        Rc::new(move || {
            let problem = match (start(), due()) {
                (Some(s), Some(d)) if s.all_day != d.all_day => Some(gettext(
                    "Start and due date need a time on both or on neither",
                )),
                (Some(s), Some(d)) if d.at < s.at => {
                    Some(gettext("The task starts after it is due"))
                }
                _ => None,
            };
            save.set_sensitive(problem.is_none());
            save.set_tooltip_text(problem.as_deref());
            if let Some(p) = &problem {
                conflict.set_title(p);
            }
            conflict.set_revealed(problem.is_some());
        })
    };
    check();
    for name in ["start", "due"] {
        on_date_change(&b, name, check.clone());
    }

    let cancel: gtk::Button = get(&b, "cancel");
    let d = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(d) = d.upgrade() {
            d.close();
        }
    });
    let link = todav_app::links::task(&t.uid);
    get::<adw::ButtonRow>(&b, "copy_link")
        .connect_activated(move |row| crate::open::copy_link(row, link.clone()));
    let (task, d) = (t.clone(), dialog.downgrade());
    get::<adw::ButtonRow>(&b, "delete").connect_activated(move |_| {
        if let Some(d) = d.upgrade() {
            d.close();
        }
        delete_task(&task);
    });
    let (task, d) = (t.clone(), dialog.downgrade());
    save.connect_clicked(move |_| {
        // Only send what changed.
        let changed =
            |new: String, old: Option<&str>| (new != old.unwrap_or_default()).then_some(new);
        let patch = TaskPatch {
            summary: changed(summary.text().trim().to_string(), Some(&task.summary))
                .filter(|s| !s.is_empty()),
            category: (!sub)
                .then(|| {
                    changed(
                        (category.selected() > 0)
                            .then(|| cats.string(category.selected()))
                            .flatten()
                            .map(String::from)
                            .unwrap_or_default(),
                        task.category.as_deref(),
                    )
                })
                .flatten(),
            description: changed(
                description.buffer().property::<String>("text"),
                task.description.as_deref(),
            ),
            start: Some(start()).filter(|s| *s != old_start),
            priority: (priority.selected() != old_priority)
                .then(|| priority_value(priority.selected())),
            event: unlinked.get().then_some(None),
            ..due_at()
        };
        let uid = task.uid.clone();
        let to = lists
            .get(list_row.selected() as usize)
            .map(|l| l.href.clone())
            .filter(|h| *h != task.list_href && !sub);
        let saved = core().together(|| {
            core().update_task(uid.clone(), patch)?;
            match to {
                Some(to) => core().move_task(uid.clone(), to),
                None => Ok(()),
            }
        });
        match saved {
            Ok(()) => request_sync(),
            Err(e) => toast(&e.to_string()),
        }
        if let Some(d) = d.upgrade() {
            d.close();
        }
    });
    dialog.present(Some(&ui.window));
}

/// Show a spin button's value as two digits, as in times ("09").
pub fn two_digits(spin: &gtk::SpinButton) {
    spin.connect_output(|s| {
        s.set_text(&format!("{:02}", s.value() as i64));
        glib::Propagation::Stop
    });
}

/// The wheel over `w` scrolls the dialog instead: a calendar would change the month and a spin
/// button its value, and with them the due date, while the user only scrolls past.
pub fn scroll_past(w: &gtk::Widget) {
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    wheel.connect_scroll(|c, _, dy| {
        let sw = c
            .widget()
            .and_then(|w| w.ancestor(gtk::ScrolledWindow::static_type()));
        if let Some(adj) = sw
            .and_downcast::<gtk::ScrolledWindow>()
            .map(|s| s.vadjustment())
        {
            adj.set_value(adj.value() + dy * adj.step_increment().max(40.0));
        }
        glib::Propagation::Stop
    });
    w.add_controller(wheel);
}

/// Call `f` on every change of the dialog's date row `name` (see `date_row`).
fn on_date_change(b: &gtk::Builder, name: &str, f: Rc<dyn Fn()>) {
    let row: adw::ExpanderRow = get(b, name);
    let g = f.clone();
    row.connect_enable_expansion_notify(move |_| g());
    // Day, month or year: scrolling over the calendar changes the month, keeping the day.
    let calendar: gtk::Calendar = get(b, &format!("{name}_calendar"));
    let g = f.clone();
    calendar.connect_notify_local(None, move |_, _| g());
    let has_time: gtk::Switch = get(b, &format!("{name}_has_time"));
    let g = f.clone();
    has_time.connect_active_notify(move |_| g());
    for part in ["hour", "minute"] {
        let spin: gtk::SpinButton = get(b, &format!("{name}_{part}"));
        let g = f.clone();
        spin.connect_value_changed(move |_| g());
    }
}

/// The dialog's date row `name` (a switch, `{name}_calendar` and an optional time in
/// `{name}_time_row`: `_hour`, `_minute`, switched by `_has_time`), set to `at`, else offering
/// today; a time not set offers 9:00. Its subtitle shows the date picked; the returned function
/// reads it.
fn date_row(b: &gtk::Builder, name: &str, at: Option<Due>) -> Rc<dyn Fn() -> Option<Due>> {
    let row: adw::ExpanderRow = get(b, name);
    let calendar: gtk::Calendar = get(b, &format!("{name}_calendar"));
    let has_time: gtk::Switch = get(b, &format!("{name}_has_time"));
    // The row says what its switch is for while there is no time.
    let time_row: adw::ActionRow = get(b, &format!("{name}_time_row"));
    let add_time = gettext("Add a Time");
    let tip = move |on: bool| time_row.set_tooltip_text((!on).then_some(add_time.as_str()));
    has_time.connect_active_notify(move |s| tip(s.is_active()));
    let hour: gtk::SpinButton = get(b, &format!("{name}_hour"));
    let minute: gtk::SpinButton = get(b, &format!("{name}_minute"));
    for spin in [&hour, &minute] {
        two_digits(spin);
    }
    let now = core().local_now();
    let f = dates::fields(at.map_or(now - now.rem_euclid(86400) + 9 * 3600, |d| d.at));
    calendar.select_day(
        &glib::DateTime::from_utc(f.year(), f.month(), f.day_of_month(), 0, 0, 0.0).unwrap(),
    );
    for w in [
        calendar.upcast_ref::<gtk::Widget>(),
        hour.upcast_ref(),
        minute.upcast_ref(),
    ] {
        scroll_past(w);
    }
    row.set_enable_expansion(at.is_some());
    // Open next to the note when set; the one-column layout collapses it again.
    row.set_expanded(at.is_some());
    let timed = at.is_some_and(|d| !d.all_day);
    has_time.set_active(timed);
    has_time.notify("active"); // the tooltip, also when it stays off
    let (h, m) = if timed {
        (f.hour(), f.minute())
    } else {
        (9, 0)
    };
    hour.set_value(h as f64);
    minute.set_value(m as f64);

    let r = row.clone();
    let read: Rc<dyn Fn() -> Option<Due>> = Rc::new(move || {
        r.enables_expansion().then(|| {
            let all_day = !has_time.is_active();
            let time = if all_day {
                0
            } else {
                hour.value() as i64 * 3600 + minute.value() as i64 * 60
            };
            Due {
                at: dates::midnight(&calendar.date()) + time,
                all_day,
            }
        })
    });
    let r = read.clone();
    let show = move || {
        let label = r().map(|d| dates::label(d.at, d.all_day, now));
        row.set_subtitle(label.as_deref().unwrap_or(""));
    };
    show();
    on_date_change(b, name, Rc::new(show));
    read
}

/// Fill the dialog's due date and reminders from `t`: what reads the patch for them, and what
/// reads the due date.
fn due_dialog(
    b: &gtk::Builder,
    t: &Task,
) -> (
    impl Fn() -> TaskPatch + 'static,
    Rc<dyn Fn() -> Option<Due>>,
) {
    let old_due = t.due.map(|at| Due {
        at,
        all_day: t.due_all_day,
    });
    let read = date_row(b, "due", old_due);

    // Seconds after the due time, as the rows show them (whole minutes), so untouched
    // reminders are not rewritten.
    let old: Vec<i64> = t
        .due
        .map(|d| {
            let mut o: Vec<i64> = t
                .reminders
                .iter()
                .map(|r| {
                    let (a, u, before) = dates::span(r - d);
                    dates::offset(a, u, before)
                })
                .collect();
            o.dedup();
            o
        })
        .unwrap_or_default();
    let reminders = crate::reminders::Reminders::new(b, t.due_all_day || t.due.is_none(), &old);
    let has_time: gtk::Switch = get(b, "due_has_time");
    let r = reminders.clone();
    has_time.connect_active_notify(move |h| r.set_all_day(!h.is_active()));

    let due = read.clone();
    let patch = move || {
        let new_due = due();
        TaskPatch {
            due: (new_due != old_due).then_some(new_due),
            reminders: Some(reminders.offsets()).filter(|r| *r != old || new_due != old_due),
            ..Default::default()
        }
    };
    (patch, read)
}

/// Priority rows: None, Low, Medium, High; as stored, `priority_value`.
fn priority_row(p: Option<i64>) -> u32 {
    match p {
        Some(1..=4) => 3,
        Some(5) => 2,
        Some(6..=9) => 1,
        _ => 0,
    }
}

/// The priority in words ("High priority"), as the edit dialog's levels; none when not set.
pub fn priority_text(p: Option<i64>) -> Option<String> {
    match priority_row(p) {
        1 => Some(gettext("Low priority")),
        2 => Some(gettext("Medium priority")),
        3 => Some(gettext("High priority")),
        _ => None,
    }
}

fn priority_value(row: u32) -> Option<i64> {
    [None, Some(9), Some(5), Some(1)][row as usize]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn search_ignores_case_and_diacritics() {
        assert_eq!(folded("Żarówki ŁÓDŹ Crème"), "zarowki lodz creme");
        assert!(folded("Rybałki").contains(&folded("rybal")));
    }

    pub(crate) fn task(uid: &str, parent: Option<&str>) -> Task {
        Task {
            uid: uid.into(),
            list_href: String::new(),
            summary: uid.into(),
            description: None,
            status: String::new(),
            done: false,
            completed_at: None,
            category: None,
            parent_uid: parent.map(Into::into),
            priority: None,
            due: None,
            due_all_day: false,
            reminders: Vec::new(),
            sort_order: None,
            created: None,
            last_modified: None,
            start: None,
            start_all_day: false,
            event_uid: None,
            sync_error: None,
            pinned: false,
        }
    }
}
