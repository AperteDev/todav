//! The Calendar page: events and dated tasks in a month grid, a week of hours or an agenda,
//! with the calendars (and task lists) to show and the tasks still to plan in the sidebar.
//! Times are local wall-clock seconds, as in the core.

mod week;

use crate::window::{get, view};
use crate::{core, dates, request_sync};
use adw::prelude::*;
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use todav_core::{Due, Event, List, Task, TaskPatch};

const DAY: i64 = 86400;

/// Items a month day shows before "+N more".
const CHIPS: usize = 3;

/// Days the agenda covers, and how far its arrows move.
const AGENDA_DAYS: i64 = 30;
const AGENDA_STEP: i64 = 7;

const CSS: &str = "
.weekdays label { font-size: smaller; opacity: 0.6; padding: 6px 0; }
.month .day { border-top: 1px solid alpha(currentColor, 0.1); padding: 2px; }
.month .day:not(:first-child) { border-left: 1px solid alpha(currentColor, 0.1); }
.day .num { font-feature-settings: 'tnum'; padding: 1px 7px; margin: 2px; }
.day.other .num { opacity: 0.4; }
.day.today .num { background: @accent_bg_color; color: @accent_fg_color; border-radius: 9999px; font-weight: bold; }
.day.selected { background: alpha(@accent_bg_color, 0.08); }
.chip { padding: 0 6px; min-height: 22px; border-radius: 6px; font-size: smaller; }
.chip button, .more button { font-weight: normal; }
.chip.task check { min-width: 14px; min-height: 14px; margin: 0 4px 0 0; padding: 0; }
.chip.task button { padding: 0; min-height: 0; }
.chip.done label { text-decoration-line: line-through; opacity: 0.55; }
.chip.cancelled label { text-decoration-line: line-through; }
.more { padding: 0 6px; min-height: 20px; font-size: smaller; }
.dot { min-width: 10px; min-height: 10px; border-radius: 9999px; }
.day:drop(active), .all-day-cell:drop(active), list.plan:drop(active) { box-shadow: inset 0 0 0 2px @accent_bg_color; background: alpha(@accent_bg_color, 0.12); }
.week .weekdays label.today { color: @accent_color; font-weight: bold; opacity: 1; }
.all-day-row { border-bottom: 1px solid alpha(currentColor, 0.15); }
.all-day-cell { padding: 2px; min-height: 26px; }
.hours label { font-size: smaller; opacity: 0.6; font-feature-settings: 'tnum'; }
.block { border-radius: 6px; padding: 2px 6px; font-size: smaller; }
.block label { font-weight: normal; }
.block.task { border-left: 3px solid alpha(currentColor, 0.5); }
.drag-card { margin: 8px; padding: 8px 14px; border-radius: 10px; background: @card_bg_color; color: @card_fg_color; box-shadow: 0 2px 8px alpha(black, 0.25); }
";

/// Something on the calendar.
#[derive(Clone)]
enum Item {
    Event(Event),
    Task(Task),
}

impl Item {
    fn list(&self) -> &str {
        match self {
            Item::Event(e) => &e.list_href,
            Item::Task(t) => &t.list_href,
        }
    }

    /// When it is, for sorting within a day: all-day things first.
    fn key(&self) -> (bool, i64) {
        match self {
            Item::Event(e) => (!e.all_day, e.start),
            Item::Task(t) => (!when(t).1, when(t).0),
        }
    }
}

/// A task's place on the calendar: its start, else its due date; and whether that is a date.
fn when(t: &Task) -> (i64, bool) {
    match t.start {
        Some(s) => (s, t.start_all_day),
        None => (t.due.unwrap_or(0), t.due_all_day),
    }
}

struct Cal {
    /// The window's Tasks / Calendar stack.
    stack: adw::ViewStack,
    split: adw::NavigationSplitView,
    mini: gtk::Calendar,
    calendars: gtk::ListBox,
    task_heading: gtk::Label,
    task_lists: gtk::ListBox,
    title: adw::WindowTitle,
    spinner: adw::Spinner,
    mode: adw::ToggleGroup,
    views: gtk::Stack,
    weekdays: gtk::Box,
    month: gtk::Grid,
    agenda: gtk::Box,
    plan_heading: gtk::Label,
    plan: gtk::ListBox,
    week_head: gtk::Box,
    week_all_day: gtk::Box,
    week_scroll: gtk::ScrolledWindow,
    week_grid: gtk::Overlay,
    week_lines: gtk::DrawingArea,
    hours: gtk::Box,
    /// The week grid's blocks and where they go.
    blocks: RefCell<Vec<(gtk::Widget, week::Geo)>>,
    /// Week-grid slot a dragged task would land in: (day of the week, minute of the day).
    hover: Cell<Option<(i64, i64)>>,
    /// The week view scrolled to the morning once.
    scrolled: Cell<bool>,
    /// Midnight of the day the views are around (the selected day).
    anchor: Cell<i64>,
    /// Set while the mini month is moved in code, so its handler ignores that.
    busy: Cell<bool>,
    /// Calendar colours, rebuilt with the sidebar.
    colors: gtk::CssProvider,
    /// Index of each collection's colour class (`cal-<i>`), by href.
    lists: RefCell<Vec<List>>,
}

thread_local! {
    static CAL: RefCell<Option<Rc<Cal>>> = const { RefCell::new(None) };
}

fn cal() -> Option<Rc<Cal>> {
    CAL.with(|c| c.borrow().clone())
}

fn today() -> i64 {
    let now = core().local_now();
    now - now.rem_euclid(DAY)
}

/// Add the Calendar page to the window's view stack; it collapses with `tasks` (the Tasks page's
/// split view).
pub fn build(views: &adw::ViewStack, tasks: &adw::NavigationSplitView) {
    let b = view!("calendar");
    let split: adw::NavigationSplitView = get(&b, "split");
    let page = views.add_titled(&split, Some("calendar"), "Calendar");
    page.set_icon_name(Some("x-office-calendar-symbolic"));
    get::<adw::InlineViewSwitcher>(&b, "switcher").set_stack(Some(views));
    tasks
        .bind_property("collapsed", &split, "collapsed")
        .sync_create()
        .build();

    let display = split.display();
    let css = gtk::CssProvider::new();
    css.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let colors = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(
        &display,
        &colors,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let c = Rc::new(Cal {
        stack: views.clone(),
        split,
        mini: get(&b, "mini"),
        calendars: get(&b, "calendars"),
        task_heading: get(&b, "task_heading"),
        task_lists: get(&b, "task_lists"),
        title: get(&b, "title"),
        spinner: get(&b, "spinner"),
        mode: get(&b, "mode"),
        views: get(&b, "views"),
        weekdays: get(&b, "weekdays"),
        month: get(&b, "month"),
        agenda: get(&b, "agenda"),
        plan_heading: get(&b, "plan_heading"),
        plan: get(&b, "plan"),
        week_head: get(&b, "week_head"),
        week_all_day: get(&b, "week_all_day"),
        week_scroll: get(&b, "week_scroll"),
        week_grid: get(&b, "week_grid"),
        week_lines: get(&b, "week_lines"),
        hours: get(&b, "hours"),
        blocks: RefCell::new(Vec::new()),
        hover: Cell::new(None),
        scrolled: Cell::new(false),
        anchor: Cell::new(today()),
        busy: Cell::new(false),
        colors,
        lists: RefCell::new(Vec::new()),
    });
    CAL.with(|cell| *cell.borrow_mut() = Some(c.clone()));

    week::setup(&c);
    plan_drop(&c);
    let mode = core().setting("calendar_mode".into()).unwrap_or_default();
    c.mode.set_active_name(Some(match mode.as_str() {
        "week" | "agenda" => mode.as_str(),
        _ => "month",
    }));
    c.mode.connect_active_name_notify(|m| {
        let name = m.active_name().map(|n| n.to_string()).unwrap_or_default();
        let _ = core().set_setting("calendar_mode".into(), name);
        refresh();
    });
    c.mini.connect_day_selected(|m| {
        let Some(c) = cal().filter(|c| !c.busy.get()) else {
            return;
        };
        c.anchor.set(dates::midnight(&m.date()));
        c.split.set_show_content(true); // narrow window: from the sidebar to the views
        refresh();
    });
    get::<gtk::Button>(&b, "today").connect_clicked(|_| go(|_| today()));
    get::<gtk::Button>(&b, "prev").connect_clicked(|_| step(-1));
    get::<gtk::Button>(&b, "next").connect_clicked(|_| step(1));

    // Shown when it was left open.
    if core().setting("last_view".into()).as_deref() == Some("calendar") {
        views.set_visible_child_name("calendar");
        glib::idle_add_local_once(refresh);
    }
    views.connect_visible_child_name_notify(|v| {
        let name = v
            .visible_child_name()
            .map(|n| n.to_string())
            .unwrap_or_default();
        let _ = core().set_setting("last_view".into(), name.clone());
        if name == "calendar" {
            refresh();
        }
    });
}

#[derive(PartialEq)]
enum Mode {
    Month,
    Week,
    Agenda,
}

fn mode(c: &Cal) -> Mode {
    match c.mode.active_name().as_deref() {
        Some("week") => Mode::Week,
        Some("agenda") => Mode::Agenda,
        _ => Mode::Month,
    }
}

/// Move to the day `f` picks from the current one.
fn go(f: impl FnOnce(i64) -> i64) {
    if let Some(c) = cal() {
        c.anchor.set(f(c.anchor.get()));
        refresh();
    }
}

/// A month, a week or the agenda's step back or forward.
fn step(by: i64) {
    let Some(c) = cal() else { return };
    match mode(&c) {
        Mode::Month => {
            let d = dates::fields(c.anchor.get());
            let first = d
                .add_days(1 - d.day_of_month())
                .and_then(|f| f.add_months(by as i32));
            if let Ok(f) = first {
                go(|_| dates::midnight(&f));
            }
        }
        Mode::Week => go(|a| a + by * 7 * DAY),
        Mode::Agenda => go(|a| a + by * AGENDA_STEP * DAY),
    }
}

/// Switch to the calendar on the day holding `t` (local time).
pub fn show_day(t: i64) {
    let Some(c) = cal() else { return };
    c.stack.set_visible_child_name("calendar");
    c.split.set_show_content(true);
    c.anchor.set(t - t.rem_euclid(DAY));
    refresh();
}

/// Redraw the page if it is built; cheap enough to call on every change.
pub fn refresh() {
    let Some(c) = cal() else { return };
    refresh_calendars(&c);
    let anchor = c.anchor.get();
    // Keep the mini month on the selected day.
    c.busy.set(true);
    c.mini.select_day(&dates::fields(anchor));
    c.busy.set(false);
    let until = match mode(&c) {
        Mode::Month => {
            c.views.set_visible_child_name("month");
            render_month(&c)
        }
        Mode::Week => {
            c.views.set_visible_child_name("week");
            week::render(&c)
        }
        Mode::Agenda => {
            c.views.set_visible_child_name("agenda");
            render_agenda(&c);
            0
        }
    };
    render_plan(&c, until);
}

/// "To Plan": open tasks without a time slot, due before `until` (the end of the shown month or
/// week) or undated, to drag onto the calendar. Not in the agenda (`until` 0).
fn render_plan(c: &Rc<Cal>, until: i64) {
    c.plan.remove_all();
    let hidden = hidden();
    let tasks: Vec<Task> = if until == 0 {
        Vec::new()
    } else {
        core()
            .unscheduled(until)
            .into_iter()
            .filter(|t| !hidden.contains(&t.list_href))
            .collect()
    };
    c.plan_heading.set_visible(until != 0);
    c.plan.set_visible(until != 0);
    // ponytail: capped; a search or a list picker if the cut matters.
    for t in tasks.iter().take(40) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let dot = gtk::Box::builder().valign(gtk::Align::Center).build();
        dot.add_css_class("dot");
        dot.add_css_class(&class(c, &t.list_href));
        row.append(&dot);
        let name = gtk::Label::builder()
            .label(&t.summary)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        row.append(&name);
        if let Some(d) = t.due {
            let due = gtk::Label::new(Some(&fmt(d, "%-d %b")));
            due.add_css_class("dim-label");
            if dates::overdue(t, core().local_now()) {
                due.add_css_class("error");
            }
            row.append(&due);
        }
        drag_task(&row, t);
        c.plan.append(&row);
    }
    if tasks.is_empty() && until != 0 {
        let none = gtk::Label::builder()
            .label("Nothing to plan")
            .xalign(0.0)
            .build();
        none.add_css_class("dim-label");
        c.plan.append(&none);
    }
}

// --- moving tasks by drag and drop -------------------------------------------

thread_local! {
    /// The task being dragged.
    static DRAGGING: RefCell<Option<Task>> = const { RefCell::new(None) };
}

fn dragging() -> Option<Task> {
    DRAGGING.with(|d| d.borrow().clone())
}

/// Make `w` drag task `t`: onto a day, an hour of the week, or back to "To Plan".
fn drag_task(w: &impl IsA<gtk::Widget>, t: &Task) {
    let src = gtk::DragSource::new();
    src.set_actions(gdk::DragAction::MOVE);
    let uid = t.uid.clone();
    src.connect_prepare(move |_, _, _| Some(gdk::ContentProvider::for_value(&uid.to_value())));
    let task = t.clone();
    src.connect_drag_begin(move |_, drag| {
        DRAGGING.with(|d| *d.borrow_mut() = Some(task.clone()));
        let card = gtk::Label::new(Some(&task.summary));
        card.add_css_class("drag-card");
        if let Ok(icon) = gtk::DragIcon::for_drag(drag).downcast::<gtk::DragIcon>() {
            icon.set_child(Some(&card));
        }
    });
    src.connect_drag_end(|_, _, _| {
        DRAGGING.with(|d| d.borrow_mut().take());
        if let Some(c) = cal() {
            c.hover.set(None);
            c.week_lines.queue_draw();
        }
    });
    w.add_controller(src);
}

/// A drop target taking dragged tasks; `dropped` gets the task.
fn task_drop(w: &impl IsA<gtk::Widget>, dropped: impl Fn(&Task) + 'static) -> gtk::DropTarget {
    let drop = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
    drop.connect_accept(|_, _| dragging().is_some());
    drop.connect_drop(move |_, _, _, _| {
        let Some(t) = dragging() else { return false };
        dropped(&t);
        true
    });
    w.add_controller(drop.clone());
    drop
}

fn save(uid: &str, patch: TaskPatch) {
    match core().update_task(uid.into(), patch) {
        Ok(()) => request_sync(),
        Err(e) => crate::window::toast(&e.to_string()),
    }
}

/// Move a task to the day starting at `day`, keeping its times of day (and its length when
/// it is planned into a slot); a task without a time is due that day.
fn move_to_day(t: &Task, day: i64) {
    let at = |t: i64, all_day| {
        Some(Due {
            at: day + t.rem_euclid(DAY),
            all_day,
        })
    };
    let patch = match (t.start, t.due) {
        (Some(s), due) if !t.start_all_day => TaskPatch {
            start: Some(at(s, false)),
            due: due.map(|d| {
                Some(Due {
                    at: d - s + day + s.rem_euclid(DAY),
                    all_day: t.due_all_day,
                })
            }),
            ..Default::default()
        },
        (_, Some(d)) if !t.due_all_day => TaskPatch {
            start: t.start.map(|_| None),
            due: Some(at(d, false)),
            ..Default::default()
        },
        _ => TaskPatch {
            start: t.start.map(|_| None),
            due: Some(Some(Due {
                at: day,
                all_day: true,
            })),
            ..Default::default()
        },
    };
    save(&t.uid, patch);
}

/// Plan a task into the slot starting at `at`: as long as it was planned before, else an hour.
fn plan_at(t: &Task, at: i64) {
    let len = match (t.start, t.due) {
        (Some(s), Some(d)) if !t.start_all_day && !t.due_all_day && d > s => d - s,
        _ => 3600,
    };
    let time = |t| {
        Some(Some(Due {
            at: t,
            all_day: false,
        }))
    };
    save(
        &t.uid,
        TaskPatch {
            start: time(at),
            due: time(at + len),
            ..Default::default()
        },
    );
}

/// Back to "To Plan": no slot, due on the day it was planned for.
fn unplan(t: &Task) {
    if t.start.is_none() && t.due_all_day {
        return; // nothing planned
    }
    let (at, _) = when(t);
    let due = t.due.map(|_| {
        Some(Due {
            at: at - at.rem_euclid(DAY),
            all_day: true,
        })
    });
    save(
        &t.uid,
        TaskPatch {
            start: Some(None),
            due,
            ..Default::default()
        },
    );
}

fn plan_drop(c: &Rc<Cal>) {
    c.plan.add_css_class("plan");
    task_drop(&c.plan, unplan);
}

pub fn set_syncing(on: bool) {
    if let Some(c) = cal() {
        c.spinner.set_visible(on);
    }
}

/// Collections the user hid (synced `hidden_calendars` setting, one href per line).
fn hidden() -> HashSet<String> {
    core()
        .setting("hidden_calendars".into())
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect()
}

fn set_hidden(href: &str, hide: bool) {
    let mut h = hidden();
    if hide {
        h.insert(href.to_string());
    } else {
        h.remove(href);
    }
    let mut v: Vec<String> = h.into_iter().collect();
    v.sort();
    let _ = core().set_setting("hidden_calendars".into(), v.join("\n"));
}

/// A colour from the server, if it is one (it goes into CSS).
fn color(l: &List) -> String {
    l.color
        .as_deref()
        .filter(|c| {
            c.len() <= 9 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
        })
        .unwrap_or("#3584e4")
        .to_string()
}

/// Every collection with its colour class, and a sidebar row each to show or hide it.
fn refresh_calendars(c: &Rc<Cal>) {
    let mut all = core().calendars();
    for l in core().lists() {
        if !all.iter().any(|a| a.href == l.href) {
            all.push(l);
        }
    }
    all.sort_by_key(|l| (!l.holds_events, l.display_name.to_lowercase()));
    if *c.lists.borrow() == all {
        return;
    }
    let css: String = all
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let col = color(l);
            format!(
                ".dot.cal-{i}, .chip.all-day.cal-{i} {{ background-color: {col}; }}
                 .chip.all-day.cal-{i} {{ color: white; }}
                 .chip.cal-{i}:not(.all-day) {{ background-color: alpha({col}, 0.18); }}\n"
            )
        })
        .collect();
    c.colors.load_from_string(&css);
    c.calendars.remove_all();
    c.task_lists.remove_all();
    let hidden = hidden();
    for (i, l) in all.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let dot = gtk::Box::builder().valign(gtk::Align::Center).build();
        dot.add_css_class("dot");
        dot.add_css_class(&format!("cal-{i}"));
        row.append(&dot);
        let name = gtk::Label::builder()
            .label(&l.display_name)
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        row.append(&name);
        let shown = gtk::CheckButton::builder()
            .active(!hidden.contains(&l.href))
            .tooltip_text("Show on the Calendar")
            .build();
        let href = l.href.clone();
        shown.connect_toggled(move |s| {
            set_hidden(&href, !s.is_active());
            glib::idle_add_local_once(refresh);
        });
        row.append(&shown);
        if l.holds_events {
            c.calendars.append(&row);
        } else {
            c.task_lists.append(&row);
        }
    }
    let lists_only = all.iter().any(|l| !l.holds_events);
    c.task_heading.set_visible(lists_only);
    c.task_lists.set_visible(lists_only);
    *c.lists.borrow_mut() = all;
}

fn class(c: &Cal, list: &str) -> String {
    let i = c
        .lists
        .borrow()
        .iter()
        .position(|l| l.href == list)
        .unwrap_or(0);
    format!("cal-{i}")
}

fn list_name(c: &Cal, list: &str) -> String {
    let lists = c.lists.borrow();
    lists
        .iter()
        .find(|l| l.href == list)
        .map_or(String::new(), |l| l.display_name.clone())
}

/// Visible events and dated tasks in `[from, to)`.
fn items(from: i64, to: i64) -> Vec<Item> {
    let hidden = hidden();
    let events = core().events(from, to).into_iter().map(Item::Event);
    let tasks = core().scheduled(from, to).into_iter().map(Item::Task);
    let mut out: Vec<Item> = events
        .chain(tasks)
        .filter(|i| !hidden.contains(i.list()))
        .collect();
    out.sort_by_key(Item::key);
    out
}

/// Whether `i` is on the day starting at `day`.
fn on_day(i: &Item, day: i64) -> bool {
    match i {
        // Zero-length events still show on their day.
        Item::Event(e) => e.start < day + DAY && (e.end > day || e.start >= day),
        Item::Task(t) => (day..day + DAY).contains(&when(t).0),
    }
}

fn fmt(t: i64, f: &str) -> String {
    dates::fields(t)
        .format(f)
        .map(String::from)
        .unwrap_or_default()
}

/// Month names are lower case in some languages; a title starts with a capital.
fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

/// Weeks start on Monday. ponytail: fixed; take the locale's first weekday if someone needs Sunday.
fn week_start(day: i64) -> i64 {
    // 1970-01-01, day 0, was a Thursday.
    day - (day.div_euclid(DAY) + 3).rem_euclid(7) * DAY
}

fn clear(w: &impl IsA<gtk::Widget>) {
    while let Some(child) = w.first_child() {
        child.unparent();
    }
}

/// Returns the end of the shown range.
fn render_month(c: &Rc<Cal>) -> i64 {
    let anchor = c.anchor.get();
    let d = dates::fields(anchor);
    let first = anchor - (d.day_of_month() as i64 - 1) * DAY;
    let start = week_start(first);
    let end = start + 42 * DAY;
    c.title.set_title(&capitalized(&fmt(first, "%OB %Y")));
    c.title.set_subtitle("");

    clear(&c.weekdays);
    for i in 0..7 {
        c.weekdays
            .append(&gtk::Label::new(Some(&fmt(start + i * DAY, "%a"))));
    }
    while let Some(child) = c.month.first_child() {
        c.month.remove(&child);
    }
    let all = items(start, end);
    let (today, month) = (today(), d.month());
    for n in 0..42 {
        let day = start + n * DAY;
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 2);
        cell.add_css_class("day");
        cell.set_overflow(gtk::Overflow::Hidden);
        if dates::fields(day).month() != month {
            cell.add_css_class("other");
        }
        if day == today {
            cell.add_css_class("today");
        }
        if day == anchor {
            cell.add_css_class("selected");
        }
        let num = gtk::Label::builder()
            .label(fmt(day, "%-d"))
            .halign(gtk::Align::End)
            .build();
        num.add_css_class("num");
        cell.append(&num);
        let here: Vec<&Item> = all.iter().filter(|i| on_day(i, day)).collect();
        let shown = if here.len() > CHIPS {
            CHIPS - 1
        } else {
            here.len()
        };
        for i in &here[..shown] {
            cell.append(&chip(c, i));
        }
        if here.len() > shown {
            cell.append(&more(
                c,
                day,
                here[shown..].iter().map(|i| (*i).clone()).collect(),
            ));
        }
        // Clicking a day selects it; a task dropped on it moves there.
        let click = gtk::GestureClick::new();
        click.connect_released(move |_, _, _, _| go(|_| day));
        cell.add_controller(click);
        task_drop(&cell, move |t| move_to_day(t, day));
        c.month.attach(&cell, (n % 7) as i32, (n / 7) as i32, 1, 1);
    }
    end
}

/// "+N more": a popover with the rest of the day.
fn more(c: &Rc<Cal>, day: i64, rest: Vec<Item>) -> gtk::MenuButton {
    let label = gtk::Label::builder()
        .label(format!("+{} more", rest.len()))
        .xalign(0.0)
        .build();
    let b = gtk::MenuButton::builder()
        .child(&label)
        .has_frame(false)
        .build();
    b.add_css_class("more");
    let weak = Rc::downgrade(c);
    b.set_create_popup_func(move |b| {
        let Some(c) = weak.upgrade() else { return };
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        list.set_margin_start(6);
        list.set_margin_end(6);
        let head = gtk::Label::builder()
            .label(fmt(day, "%A %-d %B"))
            .xalign(0.0)
            .build();
        head.add_css_class("heading");
        list.append(&head);
        for i in &rest {
            list.append(&chip(&c, i));
        }
        b.set_popover(Some(&gtk::Popover::builder().child(&list).build()));
    });
    b
}

/// A month-day entry: an event (opens its details) or a task (tick, or open it).
fn chip(c: &Rc<Cal>, i: &Item) -> gtk::Widget {
    match i {
        Item::Event(e) => {
            let text = if e.all_day {
                e.summary.clone()
            } else {
                format!("{} {}", fmt(e.start, "%H:%M"), e.summary)
            };
            let label = gtk::Label::builder()
                .label(&text)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let b = gtk::MenuButton::builder()
                .child(&label)
                .has_frame(false)
                .build();
            b.add_css_class("chip");
            b.add_css_class(&class(c, &e.list_href));
            if e.all_day {
                b.add_css_class("all-day");
            }
            if e.cancelled {
                b.add_css_class("cancelled");
            }
            let (weak, e) = (Rc::downgrade(c), e.clone());
            b.set_create_popup_func(move |b| {
                if let Some(c) = weak.upgrade() {
                    b.set_popover(Some(&details(&c, &e)));
                }
            });
            b.upcast()
        }
        Item::Task(t) => {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            row.add_css_class("chip");
            row.add_css_class("task");
            row.add_css_class(&class(c, &t.list_href));
            if t.done {
                row.add_css_class("done");
            }
            let check = gtk::CheckButton::builder()
                .active(t.done)
                .valign(gtk::Align::Center)
                .build();
            let uid = t.uid.clone();
            check.connect_toggled(move |b| {
                if core().set_done(uid.clone(), b.is_active()).is_ok() {
                    request_sync();
                }
            });
            row.append(&check);
            let (at, all_day) = when(t);
            let text = if all_day {
                t.summary.clone()
            } else {
                format!("{} {}", fmt(at, "%H:%M"), t.summary)
            };
            let label = gtk::Label::builder()
                .label(&text)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let open = gtk::Button::builder()
                .child(&label)
                .has_frame(false)
                .hexpand(true)
                .build();
            let task = t.clone();
            open.connect_clicked(move |_| crate::window::edit_task(&task));
            row.append(&open);
            drag_task(&row, t);
            row.upcast()
        }
    }
}

/// When an event is, in words: "Tuesday 20 October, 09:00 – 10:00", or its days.
fn span_text(e: &Event) -> String {
    let date = |t| fmt(t, "%A %-d %B %Y");
    if e.all_day {
        let last = e.end - DAY;
        if last <= e.start {
            date(e.start)
        } else {
            format!("{} – {}", date(e.start), date(last))
        }
    } else if e.end - e.start < DAY && fmt(e.start, "%F") == fmt(e.end - 1, "%F") {
        format!(
            "{}, {} – {}",
            date(e.start),
            fmt(e.start, "%H:%M"),
            fmt(e.end, "%H:%M")
        )
    } else {
        format!(
            "{} {} – {} {}",
            date(e.start),
            fmt(e.start, "%H:%M"),
            date(e.end),
            fmt(e.end, "%H:%M")
        )
    }
}

/// An event's details: what, when, where, which calendar, notes.
fn details(c: &Cal, e: &Event) -> gtk::Popover {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 8);
    b.set_margin_start(6);
    b.set_margin_end(6);
    b.set_margin_top(6);
    b.set_margin_bottom(6);
    b.set_width_request(280);
    let line = |icon: &str, text: &str| {
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let i = gtk::Image::from_icon_name(icon);
        i.set_valign(gtk::Align::Start);
        r.append(&i);
        let l = gtk::Label::builder()
            .label(text)
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .build();
        r.append(&l);
        r
    };
    let title = gtk::Label::builder()
        .label(&e.summary)
        .xalign(0.0)
        .wrap(true)
        .build();
    title.add_css_class("title-4");
    b.append(&title);
    b.append(&line("x-office-calendar-symbolic", &span_text(e)));
    if e.recurring {
        b.append(&line("media-playlist-repeat-symbolic", "Repeats"));
    }
    if let Some(l) = &e.location {
        b.append(&line("mark-location-symbolic", l));
    }
    let cal = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let dot = gtk::Box::builder().valign(gtk::Align::Center).build();
    dot.add_css_class("dot");
    dot.add_css_class(&class(c, &e.list_href));
    cal.append(&dot);
    cal.append(&gtk::Label::new(Some(&list_name(c, &e.list_href))));
    b.append(&cal);
    if !e.reminders.is_empty() {
        let at: Vec<String> = e
            .reminders
            .iter()
            .map(|r| dates::label(*r, false, core().local_now()))
            .collect();
        b.append(&line("alarm-symbolic", &at.join("\n")));
    }
    if let Some(d) = &e.description {
        b.append(&line("text-x-generic-symbolic", d));
    }
    gtk::Popover::builder().child(&b).build()
}

fn render_agenda(c: &Rc<Cal>) {
    let from = c.anchor.get();
    let to = from + AGENDA_DAYS * DAY;
    c.title.set_title("Agenda");
    c.title.set_subtitle(&format!(
        "{} – {}",
        fmt(from, "%-d %b"),
        fmt(to - DAY, "%-d %b %Y")
    ));
    clear(&c.agenda);
    let all = items(from, to);
    let mut any = false;
    for n in 0..AGENDA_DAYS {
        let day = from + n * DAY;
        let here: Vec<&Item> = all.iter().filter(|i| on_day(i, day)).collect();
        if here.is_empty() {
            continue;
        }
        any = true;
        let date = fmt(day, "%A %-d %B");
        let title = match (day - today()) / DAY {
            0 => format!("Today · {date}"),
            1 => format!("Tomorrow · {date}"),
            _ => capitalized(&date),
        };
        let group = adw::PreferencesGroup::builder()
            .title(glib::markup_escape_text(&title))
            .build();
        for i in here {
            group.add(&agenda_row(c, i));
        }
        c.agenda.append(&group);
    }
    if !any {
        let empty = adw::StatusPage::builder()
            .icon_name("x-office-calendar-symbolic")
            .title("Nothing Planned")
            .build();
        empty.add_css_class("compact");
        c.agenda.append(&empty);
    }
}

fn agenda_row(c: &Rc<Cal>, i: &Item) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .use_markup(false)
        .activatable(true)
        .build();
    let dot = gtk::Box::builder().valign(gtk::Align::Center).build();
    dot.add_css_class("dot");
    dot.add_css_class(&class(c, i.list()));
    match i {
        Item::Event(e) => {
            row.set_title(&e.summary);
            let time = if e.all_day {
                "All day".to_string()
            } else {
                format!("{} – {}", fmt(e.start, "%H:%M"), fmt(e.end, "%H:%M"))
            };
            let sub = match &e.location {
                Some(l) => format!("{time} · {l}"),
                None => time,
            };
            row.set_subtitle(&sub);
            row.add_prefix(&dot);
            let (weak, e) = (Rc::downgrade(c), e.clone());
            row.connect_activated(move |row| {
                let Some(c) = weak.upgrade() else { return };
                let pop = details(&c, &e);
                pop.set_parent(row);
                pop.connect_closed(|p| {
                    let p = p.clone();
                    glib::idle_add_local_once(move || p.unparent());
                });
                pop.popup();
            });
        }
        Item::Task(t) => {
            row.set_title(&t.summary);
            let (at, all_day) = when(t);
            row.set_subtitle(&if all_day {
                "Task".to_string()
            } else {
                format!("Task · {}", fmt(at, "%H:%M"))
            });
            let check = gtk::CheckButton::builder()
                .active(t.done)
                .valign(gtk::Align::Center)
                .build();
            let uid = t.uid.clone();
            check.connect_toggled(move |b| {
                if core().set_done(uid.clone(), b.is_active()).is_ok() {
                    request_sync();
                }
            });
            row.add_prefix(&check);
            row.add_prefix(&dot);
            let task = t.clone();
            row.connect_activated(move |_| crate::window::edit_task(&task));
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_start_on_monday() {
        let at = |s| todav_core::ical::parse_time(s).unwrap();
        assert_eq!(week_start(at("20261021")), at("20261019")); // a Wednesday
        assert_eq!(week_start(at("20261019")), at("20261019"));
        assert_eq!(week_start(at("20261025")), at("20261019")); // Sunday
    }
}
