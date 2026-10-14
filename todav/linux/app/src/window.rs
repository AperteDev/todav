//! Main window: login page, list sidebar, grouped task page.

use crate::{Event, core, dates, request_sync};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use todav_core::{CategoryGroup, Due, Sort, Task, TaskPatch};

/// Libadwaita only styles plain buttons as pills; round the ends of the sign-in split button to match.
/// The category section header turns its arrow like an expander row, without a "pressed" look.
/// Completed tasks shown among open ones are struck through (only their own title, not their
/// subtasks'); drop targets show a line where the task will land
/// instead of GTK's frame around the target, and "+" buttons taking the dragged task are outlined.
const CSS: &str = "
splitbutton.pill { border-radius: 9999px; }
splitbutton.pill > button { padding: 10px 20px 10px 32px; border-radius: 9999px 0 0 9999px; }
splitbutton.pill > menubutton > button { padding: 10px 16px 10px 12px; border-radius: 0 9999px 9999px 0; }
.cat-toggle image { transition: -gtk-icon-transform 200ms ease; }
.cat-toggle:not(:checked) image { -gtk-icon-transform: rotate(-90deg); }
.cat-toggle:checked:not(:hover) { background: none; }
row.task-done > box.header > box.title > label.title,
row.expander.task-done > box > list > row.header > box.header > box.title > label.title { text-decoration-line: line-through; opacity: 0.55; }
.tag { padding: 2px 10px; border-radius: 9999px; font-size: smaller; color: @accent_color; background: alpha(@accent_bg_color, 0.15); }
.tag.date { color: @window_fg_color; background: alpha(@window_fg_color, 0.08); }
.tag.overdue { color: @error_color; background: alpha(@error_bg_color, 0.15); }
:drop(active) { box-shadow: none; outline: none; }
button.drop-hint { outline: 2px solid @accent_bg_color; outline-offset: -2px; }
button.drop-hint:drop(active) { background: alpha(@accent_bg_color, 0.2); }
row.dragged { opacity: 0.4; }
.drag-card { margin: 8px; padding: 12px 18px; border-radius: 12px; background: @card_bg_color; color: @card_fg_color; box-shadow: 0 2px 8px alpha(black, 0.25); }
row.drop-above { box-shadow: inset 0 2px 0 0 @accent_bg_color; }
row.drop-below { box-shadow: inset 0 -2px 0 0 @accent_bg_color; }
";

/// Left margin per subtask level, in pixels.
const INDENT: i32 = 24;

/// Header and sidebar name for tasks without a category.
const OTHER: &str = "Other";

/// Sidebar entry for the Upcoming page (tasks with a due date in every list), kept among the
/// list hrefs.
const UPCOMING: &str = "upcoming";

/// Completed tasks shown in the "Completed" expander, newest first.
const DONE_SHOWN: usize = 50;

struct Ui {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    split: adw::NavigationSplitView,
    sidebar: gtk::ListBox,
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
    current: RefCell<Option<String>>,
    title: adw::WindowTitle,
    spinner: adw::Spinner,
    entry: gtk::Entry,
    cats: gtk::StringList,
    cat: gtk::DropDown,
    groups: gtk::Box,
    /// Root tasks whose subtasks are shown; kept across rebuilds of the task page.
    expanded: RefCell<HashSet<String>>,
    /// Root task shown as an expander for its first subtask, before it has any.
    adding: RefCell<Option<String>>,
    /// Rows show their category as a tag (the "Show as tags" category view).
    tags: Cell<bool>,
    /// Tasks are dragged by a handle (touchscreens) instead of the whole row.
    handles: Cell<bool>,
    /// The task being dragged and where it came from.
    dragging: RefCell<Option<(Task, Slot)>>,
    /// "+" buttons taking dropped tasks as subtasks of their task; for a tree row's drag-only
    /// "+", the row it shows on while collapsed.
    adopters: RefCell<Vec<(gtk::Widget, Task, Option<adw::ExpanderRow>)>>,
    /// Drop line: (row under the pointer, row showing the line, its class).
    drop_mark: RefCell<Option<(gtk::Widget, gtk::Widget, &'static str)>>,
    /// Root task whose "Add subtask" entry takes focus after the next rebuild.
    focus_sub: RefCell<Option<String>>,
    /// Welcome page: GNOME Online Accounts or a server to sign in to.
    conn: Rc<crate::connection::Connection>,
    ntfy: (gtk::DropDown, adw::EntryRow),
    connect_goa: gtk::Button,
    login: adw::SplitButton,
    cancel_login: gtk::Button,
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
    sync.connect_activate(|_, _| request_sync());
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
    let complete = gio::SimpleAction::new("complete", Some(glib::VariantTy::STRING));
    complete.connect_activate(|_, uid| {
        if let Some(uid) = uid.and_then(|u| u.str())
            && core().set_done(uid.into(), true).is_ok()
        {
            request_sync();
        }
    });
    app.add_action(&sync);
    app.add_action(&quit);
    app.add_action(&show);
    app.add_action(&complete);
    app.set_accels_for_action("app.sync", &["F5"]);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
    app.set_accels_for_action("window.close", &["<Control>w"]);
    app.set_accels_for_action("win.preferences", &["<Control>comma"]);
}

pub fn present(app: &adw::Application) {
    let ui = ui().unwrap_or_else(|| {
        let ui = build(app);
        UI.with(|u| *u.borrow_mut() = Some(ui.clone()));
        ui
    });
    let logged_in = crate::logged_in();
    ui.stack
        .set_visible_child_name(if logged_in { "main" } else { "login" });
    if logged_in {
        refresh_lists(&ui);
    }
    ui.window.present();
}

/// Disconnected: back to the welcome page.
pub fn show_welcome() {
    if let Some(ui) = ui() {
        ui.current.take();
        ui.stack.set_visible_child_name("login");
        refresh_lists(&ui); // empties the sidebar
    }
}

/// The account was replaced (or one connected after a disconnect): show its lists once they
/// arrive.
pub fn account_changed() {
    if let Some(ui) = ui() {
        ui.current.take();
        ui.stack.set_visible_child_name("main");
        refresh_lists(&ui);
    }
}

/// Rebuild the task page, e.g. after the task order changed.
pub fn refresh() {
    if let Some(ui) = ui() {
        refresh_tasks(&ui);
    }
}

pub fn toast(msg: &str) {
    if let Some(ui) = ui() {
        ui.toasts.add_toast(adw::Toast::new(msg));
    }
}

pub fn show_list(href: &str) {
    if let Some(app) = gio::Application::default().and_downcast::<adw::Application>() {
        present(&app);
    }
    if let Some(ui) = ui() {
        *ui.current.borrow_mut() = Some(href.to_string());
        refresh_lists(&ui);
        ui.split.set_show_content(true);
    }
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
                if current.as_deref() == Some(&list) || current.as_deref() == Some(UPCOMING) {
                    refresh_tasks(&ui);
                }
            });
        }
        Event::SyncState(s) => {
            ui.spinner.set_visible(s == "syncing");
            if let Some(err) = s.strip_prefix("error:") {
                if *ui.last_error.borrow() != err {
                    ui.toasts
                        .add_toast(adw::Toast::new(&format!("Sync failed: {err}")));
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
    }
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

fn build(app: &adw::Application) -> Rc<Ui> {
    let b = view!("window");
    let window: adw::ApplicationWindow = get(&b, "window");
    window.set_application(Some(app));
    if crate::sandbox() {
        window.set_title(Some("Todav (sandbox)"));
    }

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
        current: RefCell::new(None),
        title: get(&b, "title"),
        spinner: get(&b, "spinner"),
        entry: get(&b, "entry"),
        cats: get(&b, "cats"),
        cat: get(&b, "cat"),
        groups: get(&b, "groups"),
        expanded: RefCell::new(HashSet::new()),
        adding: RefCell::new(None),
        tags: Cell::new(false),
        handles: Cell::new(false),
        dragging: RefCell::new(None),
        drop_mark: RefCell::new(None),
        adopters: RefCell::new(Vec::new()),
        focus_sub: RefCell::new(None),
        conn: crate::connection::Connection::new(true, None),
        ntfy: (get(&b, "ntfy_scheme"), get(&b, "ntfy")),
        connect_goa: get(&b, "connect_goa"),
        login: get(&b, "login"),
        cancel_login: get(&b, "cancel_login"),
        last_error: RefCell::new(String::new()),
    });
    ui.conn.attach(&get(&b, "login_group"));
    connect(&ui);
    ui
}

fn connect(ui: &Rc<Ui>) {
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
        ui.split.set_show_content(true);
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
        ui.split.set_show_content(true);
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
        let start = crate::settings::order().task_start;
        match core().add_task(list, text, cat, None, start) {
            Ok(_) => {
                entry.set_text("");
                request_sync();
            }
            Err(e) => ui.toasts.add_toast(adw::Toast::new(&e.to_string())),
        }
    });

    let weak = Rc::downgrade(ui);
    ui.login.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            start_login(&ui, false);
        }
    });
    let weak = Rc::downgrade(ui);
    ui.connect_goa.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            connect_goa(&ui);
        }
    });
    let weak = Rc::downgrade(ui);
    let show_buttons = move || {
        if let Some(ui) = weak.upgrade() {
            let goa = ui.conn.chosen().is_some();
            ui.connect_goa.set_visible(goa);
            ui.login.set_visible(!goa);
        }
    };
    show_buttons();
    ui.conn.connect_changed(show_buttons);
    let weak = Rc::downgrade(ui);
    let copy = gio::SimpleAction::new("copy-login", None);
    copy.connect_activate(move |_, _| {
        if let Some(ui) = weak.upgrade() {
            start_login(&ui, true);
        }
    });
    ui.window.add_action(&copy);

    let weak = Rc::downgrade(ui);
    let prefs = gio::SimpleAction::new("preferences", None);
    prefs.connect_activate(move |_, _| {
        if let Some(ui) = weak.upgrade().filter(|_| crate::logged_in()) {
            crate::settings::present(&ui.window);
        }
    });
    ui.window.add_action(&prefs);

    let weak = Rc::downgrade(ui);
    ui.cancel_login.connect_clicked(move |_| {
        cancel_login(); // the running sign-in sees it is stale and stops
        if let Some(ui) = weak.upgrade() {
            reset_login(&ui);
        }
    });
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

/// Sign in from the welcome page with the browser (Login Flow v2); `copy` puts the login link on
/// the clipboard instead of opening the browser.
fn start_login(ui: &Rc<Ui>, copy: bool) {
    let server = url_text(&ui.conn.server);
    ui.login.set_sensitive(false);
    ui.login.set_label(if copy {
        "Waiting for Sign-In…"
    } else {
        "Waiting for Browser…"
    });
    ui.cancel_login.set_visible(true);
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        match browser_login(server, copy).await {
            Ok(Some(c)) => connect_with(&ui, crate::Target::Manual(c)).await,
            Ok(None) => {} // cancelled or superseded
            Err(e) => ui.toasts.add_toast(adw::Toast::new(&e)),
        }
        reset_login(&ui);
    });
}

/// Connect from the welcome page with the chosen GNOME Online Accounts account.
fn connect_goa(ui: &Rc<Ui>) {
    let Some(account) = ui.conn.chosen() else {
        return;
    };
    ui.connect_goa.set_sensitive(false);
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        connect_with(&ui, crate::Target::Goa(account)).await;
        ui.connect_goa.set_sensitive(true);
    });
}

async fn connect_with(ui: &Rc<Ui>, target: crate::Target) {
    let _ = core().set_setting("ntfy_url".into(), url_text(&ui.ntfy));
    match crate::switch_account(target).await {
        Ok(()) => {
            ui.stack.set_visible_child_name("main");
            refresh_lists(ui);
        }
        Err(e) => ui.toasts.add_toast(adw::Toast::new(&e)),
    }
}

/// Bumped on every sign-in and on cancel; a sign-in only goes on while its number is current.
static LOGIN_ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// Stop a running `browser_login`; it then returns `Ok(None)`.
pub fn cancel_login() {
    LOGIN_ATTEMPT.fetch_add(1, Ordering::SeqCst);
}

fn reset_login(ui: &Ui) {
    ui.login.set_sensitive(true);
    ui.login.set_label("Sign In with Browser");
    ui.cancel_login.set_visible(false);
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
        .map_err(|_| "Cannot start login".to_string())?
        .map_err(|e| format!("Cannot start login: {e}"))?;
    if !current() {
        return Ok(None);
    }
    if copy {
        if let Some(d) = gtk::gdk::Display::default() {
            d.clipboard().set_text(&flow.login_url);
        }
        toast("Login link copied to clipboard");
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
            Ok(Err(e)) => return Err(format!("Login failed: {e}")),
            Err(_) => return Err("Login failed".into()),
        }
    }
    Err("Login timed out".into())
}

fn refresh_lists(ui: &Rc<Ui>) {
    let lists = core().lists();
    let valid = |h: &str| !lists.is_empty() && (h == UPCOMING || lists.iter().any(|l| l.href == h));
    if ui.current.borrow().as_deref().is_none_or(|c| !valid(c)) {
        let last = core().setting("last_list".into()).filter(|l| valid(l));
        *ui.current.borrow_mut() = last.or_else(|| lists.first().map(|l| l.href.clone()));
    }
    ui.sidebar.remove_all();
    *ui.hrefs.borrow_mut() = (!lists.is_empty())
        .then(|| UPCOMING.to_string())
        .into_iter()
        .chain(lists.iter().map(|l| l.href.clone()))
        .collect();
    if !lists.is_empty() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.append(&gtk::Image::from_icon_name("x-office-calendar-symbolic"));
        let name = gtk::Label::builder()
            .label("Upcoming")
            .xalign(0.0)
            .hexpand(true)
            .build();
        let count = gtk::Label::new(Some(&upcoming_count().to_string()));
        count.add_css_class("dim-label");
        row.append(&name);
        row.append(&count);
        ui.sidebar.append(&row);
    }
    for l in &lists {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
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
    }
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

fn refresh_sidebar_counts(ui: &Rc<Ui>) {
    let lists = core().lists();
    if lists
        .iter()
        .map(|l| &l.href)
        .ne(ui.hrefs.borrow().iter().skip(1))
    {
        return refresh_lists(ui);
    }
    let counts =
        std::iter::once(upcoming_count()).chain(lists.iter().map(|l| l.open_count as usize));
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

/// Open tasks due today or earlier.
fn upcoming_count() -> usize {
    let now = core().local_now();
    core()
        .upcoming()
        .iter()
        .filter(|t| dates::pressing(t, now))
        .count()
}

/// Rebuild the task page where it is scrolled to. The rebuild destroys the focused widget when
/// it is a row's (a ticked checkbox): GTK would move focus to a container and scroll that into
/// view, so focus is dropped first.
fn refresh_tasks(ui: &Rc<Ui>) {
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
    let Some(href) = ui.current.borrow().clone() else {
        ui.title.set_title("");
        return;
    };
    // The new-task entry and its category picker.
    let add_box = ui.entry.parent();
    add_box.iter().for_each(|b| b.set_visible(href != UPCOMING));
    if href == UPCOMING {
        ui.cat_section.set_visible(false);
        return rebuild_upcoming(ui);
    }
    let list = core().lists().into_iter().find(|l| l.href == href);
    ui.title
        .set_title(list.as_ref().map_or("", |l| &l.display_name));
    ui.title.set_subtitle(
        &list
            .map(|l| format!("{} open", l.open_count))
            .unwrap_or_default(),
    );

    let order = crate::settings::order();
    ui.handles
        .set(core().setting("drag_handles".into()).as_deref() == Some("true"));
    let done_view = crate::settings::done_view();
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
    let refs: Vec<&str> = std::iter::once("No category")
        .chain(names.iter().map(String::as_str))
        .chain(std::iter::once("New Category…"))
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
            let label = r.as_deref().unwrap_or(OTHER);
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
                if let Err(e) = drop_on_category(&d, &ds, cat.clone()) {
                    toast(&e.to_string());
                }
                true
            });
            if let Some(row) = ui.cat_list.row_at_index(i as i32) {
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

    // A list without categories is one plain list; otherwise every group, "Other" too, has a header.
    let titled = grouping && !names.is_empty();
    let mut any = false;
    for g in &groups {
        // The sidebar filter goes by each top-level task's category (a group's name in "group").
        let trees: Vec<_> = trees(&g.tasks)
            .into_iter()
            .filter(|(root, _)| shown(root.category.as_ref()))
            .collect();
        if trees.is_empty() {
            continue;
        }
        any = true;
        let group = adw::PreferencesGroup::new();
        if titled {
            group.set_title(&glib::markup_escape_text(
                g.name.as_deref().unwrap_or(OTHER),
            ));
        }
        for (i, (root, subs)) in trees.iter().enumerate() {
            let slot = Slot {
                group: g.name.clone(),
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
        ui.groups.append(&group);
    }

    let mut done: Vec<Task> = core()
        .finished(href, done_view)
        .into_iter()
        .filter(|t| shown(t.category.as_ref()))
        .collect();
    if !done.is_empty() {
        done.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
        let exp = adw::ExpanderRow::builder()
            .title(format!("Completed ({})", done.len()))
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
            .title("All Done")
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
    ui.title.set_title("Upcoming");
    ui.title.set_subtitle(&format!("{} open", tasks.len()));
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
            .title("Nothing Scheduled")
            .description("Tasks with a due date show up here")
            .build();
        empty.add_css_class("compact");
        ui.groups.append(&empty);
    }
}

/// Split a group's depth-first task list into root tasks, each with its (depth, subtask) list.
fn trees(tasks: &[Task]) -> Vec<(&Task, Vec<(usize, &Task)>)> {
    let mut depth = HashMap::new();
    let mut out: Vec<(&Task, Vec<(usize, &Task)>)> = Vec::new();
    for t in tasks {
        match t.parent_uid.as_ref().and_then(|p| depth.get(p).copied()) {
            Some(d) => {
                depth.insert(&t.uid, d + 1);
                out.last_mut().unwrap().1.push((d + 1, t)); // a parent always precedes its subtasks
            }
            None => {
                depth.insert(&t.uid, 0usize);
                out.push((t, Vec::new()));
            }
        }
    }
    out
}

/// `indent` levels of left margin on the whole row, for subtasks; `slot` makes it draggable.
fn task_row(ui: &Rc<Ui>, t: &Task, indent: usize, slot: Option<Slot>) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&t.summary)
        .use_markup(false)
        .activatable(true)
        .build();
    row.set_margin_start(INDENT * indent as i32);
    // Shown among open tasks (a subtask, or kept in place): struck through. Rows in the
    // "Completed" group have no slot and stay plain.
    if t.done && slot.is_some() {
        row.add_css_class("task-done");
    }
    row.add_prefix(&done_check(t));
    if let Some(slot) = slot {
        // ActionRow puts each new prefix to the left of the previous ones (ExpanderRow: right).
        connect_dnd(ui, &row, t, slot, |h| row.add_prefix(h));
    }
    if let Some(d) = subtitle(t) {
        row.set_subtitle(d);
    }
    if let Some(due) = dates::tag(t, core().local_now()) {
        row.add_suffix(&due);
    }
    // Subtasks have no category of their own: the top-level task's decides. The Upcoming page
    // mixes lists, so it leaves categories alone.
    let upcoming = ui.current.borrow().as_deref() == Some(UPCOMING);
    let sub = indent > 0 || upcoming;
    if let Some(tag) = tag(ui, t).filter(|_| indent == 0) {
        row.add_suffix(&tag);
    }
    row.add_suffix(&delete_button(t));
    let (weak, task) = (Rc::downgrade(ui), t.clone());
    row.connect_activated(move |_| {
        if let Some(ui) = weak.upgrade() {
            edit_dialog(&ui, &task, sub);
        }
    });
    row
}

/// A root task without subtasks: its "+" turns it into a `tree_row` ready for the first one.
fn leaf_row(ui: &Rc<Ui>, t: &Task, slot: Slot) -> adw::ActionRow {
    let row = task_row(ui, t, 0, Some(slot));
    let add = icon_button("list-add-symbolic", "Add Subtask");
    let (weak, uid) = (Rc::downgrade(ui), t.uid.clone());
    add.connect_clicked(move |_| {
        let Some(ui) = weak.upgrade() else { return };
        ui.expanded.borrow_mut().insert(uid.clone());
        *ui.adding.borrow_mut() = Some(uid.clone());
        *ui.focus_sub.borrow_mut() = Some(uid.clone());
        // Rebuild outside the handler: refresh_tasks replaces this row.
        glib::idle_add_local_once(move || refresh_tasks(&ui));
    });
    adopt_target(ui, &add, t, None);
    row.add_suffix(&add);
    row
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
    if root.done {
        row.add_css_class("task-done"); // kept in place
    }
    let slot_group = slot.group.clone();
    connect_dnd(ui, &row, root, slot, |h| row.add_prefix(h));
    row.add_prefix(&done_check(root));
    if let Some(d) = subtitle(root) {
        row.set_subtitle(d);
    }
    // Collapsed, the header shows no subtask list to drop on: a "+" appears during drags.
    let plus = icon_button("list-add-symbolic", "Add Subtask");
    plus.set_visible(false);
    row.add_suffix(&plus);
    adopt_target(ui, &plus, root, Some(&row));
    // The header toggles the subtasks, so editing gets its own button.
    let edit = icon_button("document-edit-symbolic", "Edit");
    let (weak, task) = (Rc::downgrade(ui), root.clone());
    edit.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            edit_dialog(&ui, &task, false);
        }
    });
    row.add_suffix(&edit);
    row.add_suffix(&delete_button(root));
    // ExpanderRow puts each new suffix to the left of the previous ones.
    if let Some(tag) = tag(ui, root) {
        row.add_suffix(&tag);
    }
    if let Some(due) = dates::tag(root, core().local_now()) {
        row.add_suffix(&due);
    }
    let (weak, uid) = (Rc::downgrade(ui), root.uid.clone());
    let empty = subs.is_empty();
    row.connect_expanded_notify(move |r| {
        let Some(ui) = weak.upgrade() else { return };
        if r.is_expanded() {
            ui.expanded.borrow_mut().insert(uid.clone());
            return;
        }
        ui.expanded.borrow_mut().remove(&uid);
        // Closed before adding a first subtask: back to a plain row with "+".
        if empty && ui.adding.borrow().as_ref() == Some(&uid) {
            ui.adding.take();
            glib::idle_add_local_once(move || refresh_tasks(&ui));
        }
    });

    let entry = adw::EntryRow::builder().title("Add subtask").build();
    entry.set_margin_start(INDENT);
    entry.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
    if ui.focus_sub.borrow().as_ref() == Some(&root.uid) {
        ui.focus_sub.take();
        let e = entry.clone();
        glib::idle_add_local_once(move || {
            e.grab_focus();
        });
    }
    let (weak, root) = (Rc::downgrade(ui), root.clone());
    entry.connect_entry_activated(move |e| {
        let Some(ui) = weak.upgrade() else { return };
        let text = e.text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let parent = Some(root.uid.clone());
        match core().add_task(root.list_href.clone(), text, None, parent, start) {
            Ok(_) => {
                e.set_text("");
                *ui.focus_sub.borrow_mut() = Some(root.uid.clone()); // keep typing after the rebuild
                request_sync();
            }
            Err(err) => ui.toasts.add_toast(adw::Toast::new(&err.to_string())),
        }
    });
    if start {
        row.add_row(&entry);
    }
    for (i, (depth, t)) in subs.iter().enumerate() {
        let slot = Slot {
            group: slot_group.clone(),
            root: false,
            next: subs[i + 1..]
                .iter()
                .find(|(_, s)| s.parent_uid == t.parent_uid)
                .map(|(_, s)| s.uid.clone()),
        };
        row.add_row(&task_row(ui, t, *depth, Some(slot)));
    }
    if !start {
        row.add_row(&entry);
    }
    row
}

/// Where a row sits, for drag and drop.
#[derive(Clone)]
struct Slot {
    /// Category group the row is shown in; None is "Other".
    group: Option<String>,
    /// A top-level row (otherwise a subtask, movable only among its siblings).
    root: bool,
    /// The row after it that a drop below lands in front of; None means last.
    next: Option<String>,
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
                if can_adopt(&task, parent) && tree.as_ref().is_none_or(|t| !t.is_expanded()) {
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
        handle.set_tooltip_text(Some("Drag to Move"));
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
                .is_some_and(|(d, ds)| can_drop(d, ds, &target, &s))
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
                .is_some_and(|(d, ds)| stays_put(d, ds, &target, &s, above));
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
        if let Err(e) = drop_task(&d, &ds, &target, &slot, above) {
            ui.toasts.add_toast(adw::Toast::new(&e.to_string()));
        }
        true
    });
    row.add_controller(drop);
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
            dragging.as_ref().is_some_and(|(d, _)| can_adopt(d, &p))
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
        match adopt(&d, &p) {
            Ok(()) => {
                ui.expanded.borrow_mut().insert(p.uid.clone()); // show where it went
                request_sync();
            }
            Err(e) => ui.toasts.add_toast(adw::Toast::new(&e.to_string())),
        }
        true
    });
    w.add_controller(drop);
}

/// `d` can become a subtask of `parent` unless it already is one or is `parent`, or has
/// subtasks of its own (nested trees are not supported yet).
fn can_adopt(d: &Task, parent: &Task) -> bool {
    d.uid != parent.uid && d.parent_uid.as_ref() != Some(&parent.uid) && !has_subtasks(d)
}

fn has_subtasks(t: &Task) -> bool {
    core()
        .tasks(t.list_href.clone(), true)
        .iter()
        .any(|s| s.parent_uid.as_ref() == Some(&t.uid))
}

/// Make `d` a subtask of `parent`, first or last among its subtasks as new subtasks would be.
fn adopt(d: &Task, parent: &Task) -> todav_core::Result<()> {
    core().set_parent(d.uid.clone(), parent.uid.clone())?;
    if d.category.is_some() {
        set_category(&d.uid, None)?; // the top-level task's category decides
    }
    let order = crate::settings::order();
    if order.subtasks == Sort::Manual {
        let first = core()
            .tasks(parent.list_href.clone(), true)
            .into_iter()
            .find(|t| t.parent_uid.as_ref() == Some(&parent.uid) && t.uid != d.uid);
        match first {
            Some(f) if order.sub_start => core().reorder(d.uid.clone(), Some(f.uid))?,
            Some(_) => core().reorder(d.uid.clone(), None)?,
            None => {} // the only subtask
        }
    }
    Ok(())
}

/// Would dropping `d` above or below `t` leave it where it is? Then no line is drawn there.
fn stays_put(d: &Task, ds: &Slot, t: &Task, ts: &Slot, above: bool) -> bool {
    let before = if above {
        Some(&t.uid)
    } else {
        ts.next.as_ref()
    };
    ds.root == ts.root
        && ds.group == ts.group
        && d.parent_uid == t.parent_uid
        && (before == Some(&d.uid) || before == ds.next.as_ref())
}

/// Top-level tasks move within and between groups (changing category); a subtask dropped among
/// them is promoted. A task without subtasks dropped among subtasks joins them under their
/// parent. Reordering only means something in manual order.
fn can_drop(d: &Task, ds: &Slot, t: &Task, ts: &Slot) -> bool {
    let order = crate::settings::order();
    if d.uid == t.uid {
        return false;
    }
    if ts.root {
        return !ds.root || ds.group != ts.group || order.tasks == Sort::Manual;
    }
    if d.parent_uid == t.parent_uid {
        return order.subtasks == Sort::Manual;
    }
    !has_subtasks(d) // nested trees are not supported yet
}

/// Move dragged task `d` above or below target `t`.
fn drop_task(d: &Task, ds: &Slot, t: &Task, ts: &Slot, above: bool) -> todav_core::Result<()> {
    let order = crate::settings::order();
    let promote = !ds.root && ts.root;
    if promote {
        core().promote(d.uid.clone())?;
    }
    if ts.root && (promote || ds.group != ts.group) {
        set_category(&d.uid, ts.group.clone())?;
    }
    if !ts.root && d.parent_uid != t.parent_uid {
        let parent = t.parent_uid.clone().unwrap_or_default();
        core().set_parent(d.uid.clone(), parent)?;
        if d.category.is_some() {
            set_category(&d.uid, None)?; // the new top-level task's category decides
        }
    }
    let sort = if ts.root { order.tasks } else { order.subtasks };
    let before = if above {
        Some(t.uid.clone())
    } else {
        ts.next.clone()
    };
    // Dropping right below the task above it leaves `d` where it is.
    if sort == Sort::Manual && before.as_ref() != Some(&d.uid) {
        core().reorder(d.uid.clone(), before)?;
    }
    request_sync();
    Ok(())
}

fn set_category(uid: &str, category: Option<String>) -> todav_core::Result<()> {
    let patch = TaskPatch {
        category: Some(category.unwrap_or_default()),
        ..Default::default()
    };
    core().update_task(uid.into(), patch)
}

/// A task dropped on a sidebar category: into that category (a subtask becomes top-level),
/// first or last in it as new tasks would be.
fn drop_on_category(d: &Task, ds: &Slot, category: Option<String>) -> todav_core::Result<()> {
    if !ds.root {
        core().promote(d.uid.clone())?;
    }
    set_category(&d.uid, category)?;
    let order = crate::settings::order();
    if order.tasks == Sort::Manual {
        // Before every task of the list puts it first in its category too.
        let first = core()
            .tasks(d.list_href.clone(), true)
            .first()
            .map(|t| t.uid.clone());
        match first {
            _ if !order.task_start => core().reorder(d.uid.clone(), None)?,
            Some(f) if f != d.uid => core().reorder(d.uid.clone(), Some(f))?,
            _ => {} // already first
        }
    }
    request_sync();
    Ok(())
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
    check.connect_toggled(move |c| {
        if core().set_done(uid.clone(), c.is_active()).is_ok() {
            request_sync();
        }
    });
    check
}

fn delete_button(t: &Task) -> gtk::Button {
    let del = icon_button("user-trash-symbolic", "Delete");
    let task = t.clone();
    del.connect_clicked(move |_| {
        let Ok(ics) = core().delete_task(task.uid.clone()) else {
            return;
        };
        request_sync();
        let Some(ui) = ui() else { return };
        let toast = adw::Toast::builder()
            .title(format!("“{}” deleted", task.summary))
            .button_label("Undo")
            .priority(adw::ToastPriority::High)
            .build();
        toast.set_use_markup(false);
        let list = task.list_href.clone();
        toast.connect_button_clicked(move |_| {
            match core().restore_task(list.clone(), ics.clone()) {
                Ok(()) => request_sync(),
                Err(e) => self::toast(&e.to_string()),
            }
        });
        ui.toasts.add_toast(toast);
    });
    del
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
    let dialog: adw::AlertDialog = get(&b, "dialog");
    let summary: adw::EntryRow = get(&b, "summary");
    let category: adw::ComboRow = get(&b, "category");
    let cats: gtk::StringList = get(&b, "cats");
    let description: adw::EntryRow = get(&b, "description");
    summary.set_text(&t.summary);
    description.set_text(t.description.as_deref().unwrap_or_default());

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
    let due_at = due_dialog(&b, t);
    let task = t.clone();
    dialog.connect_response(Some("save"), move |_, _| {
        // Only send what changed: a one-line entry would flatten a multi-line note.
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
            description: changed(description.text().to_string(), task.description.as_deref()),
            ..due_at()
        };
        let uid = task.uid.clone();
        if core().update_task(uid.clone(), patch).is_ok() {
            request_sync();
        }
    });
    dialog.present(Some(&ui.window));
}

/// Fill the dialog's due date rows from `t`; the returned function reads what changed.
fn due_dialog(b: &gtk::Builder, t: &Task) -> impl Fn() -> TaskPatch + 'static {
    let due: adw::ExpanderRow = get(b, "due");
    let calendar: gtk::Calendar = get(b, "calendar");
    let has_time: adw::SwitchRow = get(b, "has_time");
    let (hour, minute): (gtk::SpinButton, gtk::SpinButton) = (get(b, "hour"), get(b, "minute"));
    for spin in [&hour, &minute] {
        spin.connect_output(|s| {
            s.set_text(&format!("{:02}", s.value() as i64));
            glib::Propagation::Stop
        });
    }

    let now = core().local_now();
    let at = t.due.unwrap_or(now - now.rem_euclid(86400) + 9 * 3600); // default: today at 9:00
    let f = dates::fields(at);
    calendar.select_day(
        &glib::DateTime::from_utc(f.year(), f.month(), f.day_of_month(), 0, 0, 0.0).unwrap(),
    );
    // The wheel over the calendar scrolls the dialog: GtkCalendar would change the month, and
    // with it the due date, while the user only scrolls past.
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
    calendar.add_controller(wheel);
    due.set_enable_expansion(t.due.is_some());
    due.set_expanded(false); // enabling expands it; it opens on a click or the switch
    has_time.set_active(t.due.is_some() && !t.due_all_day);
    hour.set_value(f.hour() as f64);
    minute.set_value(f.minute() as f64);

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

    let read = {
        let (due, calendar, has_time) = (due.clone(), calendar.clone(), has_time.clone());
        let (hour, minute) = (hour.clone(), minute.clone());
        move || {
            due.enables_expansion().then(|| {
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
        }
    };
    // The row's subtitle shows the date picked.
    let show = {
        let (due, read) = (due.clone(), read.clone());
        move || {
            let label = read().map(|d| dates::label(d.at, d.all_day, now));
            due.set_subtitle(label.as_deref().unwrap_or(""));
        }
    };
    show();
    let s = show.clone();
    // Day, month or year: scrolling over the calendar changes the month, keeping the day.
    calendar.connect_notify_local(None, move |_, _| s());
    let s = show.clone();
    due.connect_enable_expansion_notify(move |_| s());
    let s = show.clone();
    hour.connect_value_changed(move |_| s());
    let s = show.clone();
    minute.connect_value_changed(move |_| s());
    let r = reminders.clone();
    has_time.connect_active_notify(move |h| {
        show();
        r.set_all_day(!h.is_active());
    });

    let old_due = t.due.map(|at| Due {
        at,
        all_day: t.due_all_day,
    });
    move || {
        let new_due = read();
        TaskPatch {
            due: (new_due != old_due).then_some(new_due),
            reminders: Some(reminders.offsets()).filter(|r| *r != old || new_due != old_due),
            ..Default::default()
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

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
        }
    }

    #[test]
    fn trees_nest_depth_first_lists() {
        let tasks = [
            task("a", None),
            task("a1", Some("a")),
            task("a1x", Some("a1")),
            task("a2", Some("a")),
            task("b", None),
            task("c", Some("gone")), // parent closed or deleted: shown as a root
        ];
        let got: Vec<(&str, Vec<(usize, &str)>)> = trees(&tasks)
            .into_iter()
            .map(|(r, s)| {
                (
                    r.uid.as_str(),
                    s.iter().map(|(d, t)| (*d, t.uid.as_str())).collect(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("a", vec![(1, "a1"), (2, "a1x"), (1, "a2")]),
                ("b", vec![]),
                ("c", vec![]),
            ]
        );
    }
}
