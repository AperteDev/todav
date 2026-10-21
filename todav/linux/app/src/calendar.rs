//! The Calendar page: events and dated tasks in a month grid, a week of hours or an agenda,
//! with the calendars (and task lists) to show and the tasks still to plan in the sidebar.
//! Times are local wall-clock seconds, as in the core.

mod days;
mod dialog;
mod mini;
mod week;

use crate::window::{get, view};
use crate::{core, dates, request_sync};
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use todav_core::{Due, Event, EventPatch, EventTime, List, Scope, Task, TaskPatch};

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
menubutton.chip > button, menubutton.more > button { min-height: 0; min-width: 0; padding: 0; }
/* A task is a button itself: its colour is its background, so hover and press tint over it. */
button.chip.task { min-width: 0; font-weight: normal; }
button.chip.task:hover { background-image: linear-gradient(alpha(currentColor, 0.07), alpha(currentColor, 0.07)); }
button.chip.task:active { background-image: linear-gradient(alpha(currentColor, 0.16), alpha(currentColor, 0.16)); }
/* An event's button fills it, so its hover does too. */
menubutton.chip, menubutton.more { padding: 0; }
menubutton.chip > button, menubutton.more > button { padding: 0 6px; border-radius: 6px; }
menubutton.block > button { padding: 2px 6px; }
.mini { padding: 6px; }
.mini .weekday { font-size: smaller; opacity: 0.6; padding: 4px 0; }
.mini button.day { min-width: 32px; min-height: 32px; padding: 0; font-weight: normal; font-feature-settings: 'tnum'; }
.mini button.day.other { opacity: 0.45; }
.mini button.day.today { box-shadow: inset 0 0 0 1px @accent_color; }
.mini button.day.selected { background: @accent_bg_color; color: @accent_fg_color; }
.mini .week-band { background: alpha(currentColor, 0.08); border-radius: 9999px; }
/* Ends of a bar that goes on into the next or previous week (or past a full day) are square. */
.chip.from-before, menubutton.chip.from-before > button { border-top-left-radius: 0; border-bottom-left-radius: 0; }
.chip.goes-on, menubutton.chip.goes-on > button { border-top-right-radius: 0; border-bottom-right-radius: 0; }
.chip.task check { min-width: 14px; min-height: 14px; margin: 0 4px 0 0; padding: 0; }
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
/* Opaque, so the block it is resized or moved from does not show through it. */
.drop-preview { border-radius: 6px; padding: 2px 6px; font-size: smaller; font-feature-settings: 'tnum'; background: color-mix(in srgb, @accent_bg_color 25%, @window_bg_color); box-shadow: inset 0 0 0 1px @accent_bg_color; }
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
    mini: mini::Mini,
    calendars: gtk::ListBox,
    task_heading: gtk::ToggleButton,
    task_lists: gtk::ListBox,
    title: adw::WindowTitle,
    sync_state: gtk::Stack,
    search: gtk::SearchBar,
    /// The search, `folded`: while set, matching events show instead of the views.
    query: RefCell<String>,
    mode: adw::ToggleGroup,
    views: gtk::Stack,
    weekdays: gtk::Box,
    month: gtk::Grid,
    agenda: gtk::Box,
    plan_heading: gtk::ToggleButton,
    plan: gtk::ListBox,
    week_head: gtk::Box,
    week_all_day: gtk::Box,
    week_scroll: gtk::ScrolledWindow,
    week_grid: gtk::Overlay,
    week_lines: gtk::DrawingArea,
    hours: gtk::Box,
    /// The week grid's blocks, where they go and what they show.
    blocks: RefCell<Vec<(gtk::Widget, week::Geo, Item)>>,
    /// Week-grid slot a dragged task would land in: (day of the week, minute of the day).
    hover: Cell<Option<(i64, i64)>>,
    /// The block showing where a dragged or resized item lands, with its times, and where that
    /// is: (day of the week, from, to in minutes into the day).
    preview: gtk::Label,
    span: Cell<Option<(i64, i64, i64)>>,
    /// The week view scrolled to the morning once.
    scrolled: Cell<bool>,
    /// An event's details are open: redraws wait (`pending`) until it closes.
    holding: Cell<bool>,
    pending: Cell<bool>,
    /// Midnight of the day the views are around (the selected day).
    anchor: Cell<i64>,
    /// Calendar colours, rebuilt with the sidebar.
    colors: gtk::CssProvider,
    /// Index of each collection's colour class (`cal-<i>`), by href.
    lists: RefCell<Vec<List>>,
    /// The hidden ones the sidebar rows show.
    hidden: RefCell<HashSet<String>>,
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

/// Add the Calendar page to the window's view stack.
pub fn build(views: &adw::ViewStack) {
    let b = view!("calendar");
    let split: adw::NavigationSplitView = get(&b, "split");
    // Its own breakpoints, the narrowest last (the last matching one wins): tight, the view
    // switcher moves to a bar at the bottom and the sync button goes (F5 still syncs); narrow,
    // one pane at a time, with the Tasks page.
    let bin = adw::BreakpointBin::builder()
        .width_request(360)
        .height_request(294)
        .child(&split)
        .build();
    let breakpoint = |condition| {
        let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse(condition).unwrap());
        bin.add_breakpoint(bp.clone());
        bp
    };
    let (mode, bar, sync): (adw::ToggleGroup, gtk::ActionBar, gtk::Button) =
        (get(&b, "mode"), get(&b, "mode_bar"), get(&b, "sync"));
    let to_bar = |bp: &adw::Breakpoint| {
        bp.add_setter(&mode, "visible", Some(&false.to_value()));
        bp.add_setter(&bar, "revealed", Some(&true.to_value()));
        bp.add_setter(&sync, "visible", Some(&false.to_value()));
    };
    to_bar(&breakpoint("max-width: 880sp"));
    let narrow = breakpoint("max-width: 700sp");
    to_bar(&narrow);
    narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
    let page = views.add_titled(&bin, Some("calendar"), "Calendar");
    page.set_icon_name(Some("x-office-calendar-symbolic"));
    get::<adw::InlineViewSwitcher>(&b, "switcher").set_stack(Some(views));

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
        mini: mini::Mini::new(&get(&b, "mini")),
        calendars: get(&b, "calendars"),
        task_heading: get(&b, "task_heading"),
        task_lists: get(&b, "task_lists"),
        title: get(&b, "title"),
        sync_state: get(&b, "sync_state"),
        search: get(&b, "search"),
        query: RefCell::new(String::new()),
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
        preview: gtk::Label::builder()
            .css_classes(["drop-preview"])
            .xalign(0.0)
            .yalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .can_target(false)
            .visible(false)
            .build(),
        span: Cell::new(None),
        scrolled: Cell::new(false),
        holding: Cell::new(false),
        pending: Cell::new(false),
        anchor: Cell::new(today()),
        colors,
        lists: RefCell::new(Vec::new()),
        hidden: RefCell::new(HashSet::new()),
    });
    CAL.with(|cell| *cell.borrow_mut() = Some(c.clone()));

    week::setup(&c);
    plan_drop(&c);
    // Clicking a calendar's row shows or hides it.
    for (list, events) in [(&c.calendars, true), (&c.task_lists, false)] {
        list.connect_row_activated(move |_, row| {
            let Some(c) = cal() else { return };
            // Rows are in the section's order.
            if let Some(href) = section(&c, events).get(row.index() as usize) {
                set_hidden([href.as_str()], !hidden().contains(href));
                glib::idle_add_local_once(refresh);
            }
        });
    }
    // The Month / Week / Agenda choice in the bottom bar, when it is there.
    let narrow: adw::ToggleGroup = get(&b, "mode_narrow");
    c.mode
        .bind_property("active-name", &narrow, "active-name")
        .bidirectional()
        .sync_create()
        .build();
    // Shrinking keeps the views, as on the Tasks page; the sidebar is a Back away.
    c.split.set_show_content(true);
    c.split.connect_collapsed_notify(|s| {
        if s.is_collapsed() {
            s.set_show_content(true);
        }
    });
    let mode = at_start("start_calendar_view", "calendar_mode");
    c.mode.set_active_name(Some(match mode.as_str() {
        "week" | "agenda" => mode.as_str(),
        _ => "month",
    }));
    c.mode.connect_active_name_notify(|m| {
        let name = m.active_name().map(|n| n.to_string()).unwrap_or_default();
        let _ = core().set_setting("calendar_mode".into(), name);
        if let Some(c) = cal() {
            c.search.set_search_mode(false);
        }
        refresh();
    });
    // Typing on the page searches the events; going to a day, a view or a period ends it.
    let query: gtk::SearchEntry = get(&b, "query");
    crate::window::search_bar(&c.search, &query, views, "calendar");
    query.connect_search_changed(|e| {
        let Some(c) = cal() else { return };
        *c.query.borrow_mut() = crate::window::folded(e.text().trim());
        refresh();
    });
    // The header's buttons and the page's shortcuts.
    let actions = gio::SimpleActionGroup::new();
    let action = |name: &str, f: fn()| {
        gio::ActionEntry::builder(name)
            .activate(move |_: &gio::SimpleActionGroup, _, _| f())
            .build()
    };
    actions.add_action_entries([
        action("new-event", new_event),
        action("today", || go(|_| today())),
        action("previous", || step(-1)),
        action("next", || step(1)),
        gio::ActionEntry::builder("view")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(|_: &gio::SimpleActionGroup, _, view| {
                if let (Some(c), Some(v)) = (cal(), view.and_then(|v| v.str())) {
                    c.mode.set_active_name(Some(v));
                }
            })
            .build(),
    ]);
    c.split.insert_action_group("cal", Some(&actions));
    // New events are written in the system's zone.
    core().set_local_zone(glib::TimeZone::local().identifier().to_string());

    // Shown when chosen in Preferences, or when it was left open.
    if at_start("start_view", "last_view") == "calendar" {
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

/// What to show at start: the setting `choice`, or the `last` one used when it says so.
fn at_start(choice: &str, last: &str) -> String {
    match core().setting(choice.into()).unwrap_or_default().as_str() {
        "" | "last" => core().setting(last.into()).unwrap_or_default(),
        v => v.to_string(),
    }
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
        c.search.set_search_mode(false);
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

/// The timed event on now or next within a week, in the calendars shown (for the extension).
/// A new event on the selected day, at the next full hour when that is today, else at 9:00.
fn new_event() {
    let Some(c) = cal() else { return };
    let day = c.anchor.get();
    let now = core().local_now();
    let start = if day == today() {
        (now / 3600 + 1) * 3600
    } else {
        day + 9 * 3600
    };
    let time = EventTime {
        start,
        end: start + 3600,
        all_day: false,
    };
    dialog::open(&c, None, time);
}

pub fn next_event() -> Option<Event> {
    let hidden = hidden();
    let now = core().local_now();
    core()
        .events(now, now + 7 * DAY)
        .into_iter()
        .filter(|e| !e.all_day && !e.cancelled && e.end > now && !hidden.contains(&e.list_href))
        .min_by_key(|e| e.start)
}

/// Today's events in the calendars shown, for the Tasks page's Upcoming.
pub fn today_events() -> Vec<Event> {
    let hidden = hidden();
    let mut all = core().events(today(), today() + DAY);
    all.retain(|e| !hidden.contains(&e.list_href) && !e.cancelled);
    all.sort_by_key(|e| (!e.all_day, e.start));
    all
}

/// Switch to the calendar on the day holding `t` (local time).
pub fn show_day(t: i64) {
    let Some(c) = cal() else { return };
    c.stack.set_visible_child_name("calendar");
    c.split.set_show_content(true);
    c.anchor.set(t - t.rem_euclid(DAY));
    c.search.set_search_mode(false);
    refresh();
}

/// Redraw the page if it is built; cheap enough to call on every change.
pub fn refresh() {
    let Some(c) = cal() else { return };
    if c.holding.get() {
        c.pending.set(true);
        return;
    }
    refresh_calendars(&c);
    mini::refresh(&c);
    let until = match mode(&c) {
        _ if !c.query.borrow().is_empty() => {
            c.views.set_visible_child_name("agenda");
            render_search(&c);
            0
        }
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
    let shown: Vec<&Task> = tasks.iter().take(40).collect();
    // Two groups: due by a date (they come first), then without one.
    let dated: Vec<bool> = shown.iter().map(|t| t.due.is_some()).collect();
    c.plan.set_header_func(move |row, _| {
        let i = row.index() as usize;
        let starts = dated.get(i).filter(|&&d| i == 0 || dated[i - 1] != d);
        let header = starts.map(|&d| {
            let l = gtk::Label::builder()
                .label(if d { "Due" } else { "No date" })
                .xalign(0.0)
                .margin_start(18)
                .margin_top(6)
                .margin_bottom(2)
                .build();
            l.add_css_class("caption-heading");
            l.add_css_class("dim-label");
            l
        });
        row.set_header(header.as_ref());
    });
    for t in shown {
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
        row.set_tooltip_markup(Some(&tooltip(c, &Item::Task(t.clone()))));
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

// --- moving tasks and events by drag and drop --------------------------------

thread_local! {
    /// The task or event being dragged.
    static DRAGGING: RefCell<Option<Item>> = const { RefCell::new(None) };
    /// How far below its top a week block was grabbed (0 for anything else): it stays under the
    /// pointer at that point while it moves.
    static GRAB: Cell<f64> = const { Cell::new(0.0) };
    /// The card with its title following the pointer; hidden over the week's hours, where the
    /// preview block shows it.
    static CARD: RefCell<Option<gtk::Label>> = const { RefCell::new(None) };
}

fn dragging() -> Option<Item> {
    DRAGGING.with(|d| d.borrow().clone())
}

/// Make `w` drag `i`: onto a day or an hour of the week; a task also back to "To Plan".
fn drag_item(w: &impl IsA<gtk::Widget>, i: Item) {
    let src = gtk::DragSource::new();
    src.set_actions(gdk::DragAction::MOVE);
    src.connect_prepare(|src, _, y| {
        let block = src.widget().is_some_and(|w| w.has_css_class("block"));
        GRAB.set(if block { y } else { 0.0 });
        Some(gdk::ContentProvider::for_value(&"todav-item".to_value()))
    });
    src.connect_drag_begin(move |_, drag| {
        let title = match &i {
            Item::Task(t) => t.summary.clone(),
            Item::Event(e) => e.summary.clone(),
        };
        DRAGGING.with(|d| *d.borrow_mut() = Some(i.clone()));
        let card = gtk::Label::new(Some(&title));
        card.add_css_class("drag-card");
        if let Ok(icon) = gtk::DragIcon::for_drag(drag).downcast::<gtk::DragIcon>() {
            icon.set_child(Some(&card));
        }
        CARD.with(|c| *c.borrow_mut() = Some(card));
    });
    src.connect_drag_end(|_, _, _| {
        DRAGGING.with(|d| d.borrow_mut().take());
        CARD.with(|c| c.borrow_mut().take());
        if let Some(c) = cal() {
            c.hover.set(None);
            week::show_preview(&c, None);
        }
    });
    w.add_controller(src);
}

fn drag_task(w: &impl IsA<gtk::Widget>, t: &Task) {
    drag_item(w, Item::Task(t.clone()));
}

/// An event in a calendar we may change can be dragged.
fn drag_event(c: &Cal, w: &impl IsA<gtk::Widget>, e: &Event) {
    if writable(c, &e.list_href) {
        drag_item(w, Item::Event(e.clone()));
    }
}

fn writable(c: &Cal, list: &str) -> bool {
    c.lists
        .borrow()
        .iter()
        .any(|l| l.href == list && l.writable)
}

/// A drop target taking the dragged items `accepts` agrees to (no highlight for the others);
/// `dropped` gets the item.
fn item_drop(
    w: &impl IsA<gtk::Widget>,
    accepts: fn(&Item) -> bool,
    dropped: impl Fn(&Item) + 'static,
) -> gtk::DropTarget {
    let drop = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
    drop.connect_accept(move |_, _| dragging().is_some_and(|i| accepts(&i)));
    drop.connect_drop(move |_, _, _, _| {
        let Some(i) = dragging() else { return false };
        dropped(&i);
        true
    });
    w.add_controller(drop.clone());
    drop
}

/// Drop target for a day (a month cell, the week's all-day row): items move there keeping their
/// time; with `all_day`, events become all-day events and tasks leave their time slot.
fn day_drop(w: &impl IsA<gtk::Widget>, day: i64, all_day: bool) {
    item_drop(w, |_| true, move |i| drop_on_day(i, day, all_day));
}

fn drop_on_day(i: &Item, day: i64, all_day: bool) {
    match i {
        // Out of the hours: due that day, no time (its due time was the slot's end).
        Item::Task(t) if all_day && t.start.is_some() && !t.start_all_day => save(
            &t.uid,
            TaskPatch {
                start: Some(None),
                due: Some(Some(Due {
                    at: day,
                    all_day: true,
                })),
                ..Default::default()
            },
        ),
        Item::Task(t) => move_to_day(t, day),
        Item::Event(e) if all_day && !e.all_day => move_event(
            e,
            EventTime {
                start: day,
                end: day + DAY,
                all_day: true,
            },
        ),
        Item::Event(e) => {
            let start = day + e.start.rem_euclid(DAY);
            move_event(
                e,
                EventTime {
                    start,
                    end: start + e.end - e.start,
                    all_day: e.all_day,
                },
            )
        }
    }
}

/// Give an occurrence a new time; for a repeating event, ask which occurrences move.
fn move_event(e: &Event, time: EventTime) {
    if (time.start, time.end, time.all_day) == (e.start, e.end, e.all_day) {
        return;
    }
    let e = e.clone();
    let apply = move |scope: Scope| {
        let patch = EventPatch {
            time: Some(time),
            ..Default::default()
        };
        match core().update_event(
            e.list_href.clone(),
            e.uid.clone(),
            e.recurrence_id,
            scope,
            patch,
        ) {
            Ok(()) => request_sync(),
            Err(err) => crate::window::toast(&err.to_string()),
        }
    };
    match cal() {
        Some(c) if e.recurring => {
            dialog::ask_scope(&c.split, "Move Repeating Event?", false, apply)
        }
        _ => apply(Scope::All),
    }
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
    let len = match (t.start, slot_end(t)) {
        (Some(s), Some(e)) => e - s,
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

/// Where a planned task's slot ends: its due time, when that is later the same day. A deadline
/// further off is not the end of a slot.
fn slot_end(t: &Task) -> Option<i64> {
    let s = t.start.filter(|_| !t.start_all_day)?;
    t.due
        .filter(|&d| !t.due_all_day && d > s && d <= s - s.rem_euclid(DAY) + DAY)
}

/// Whether `t` has a time slot: a start, or a due time rather than a date. Without one it is in
/// "To Plan" already.
fn planned(t: &Task) -> bool {
    t.start.is_some() || (t.due.is_some() && !t.due_all_day)
}

/// Back to "To Plan": no slot, due on the day it was planned for.
fn unplan(t: &Task) {
    if !planned(t) {
        return;
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
    // Only planned tasks: one dragged out of the list does not light it up.
    let accepts = |i: &Item| matches!(i, Item::Task(t) if planned(t));
    item_drop(&c.plan, accepts, |i| {
        if let Item::Task(t) = i {
            unplan(t);
        }
    });
}

pub fn set_syncing(on: bool) {
    if let Some(c) = cal() {
        crate::window::set_syncing(&c.sync_state, on);
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

fn set_hidden<'a>(hrefs: impl IntoIterator<Item = &'a str>, hide: bool) {
    let mut h = hidden();
    for href in hrefs {
        if hide {
            h.insert(href.to_string());
        } else {
            h.remove(href);
        }
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
    let hidden = hidden();
    if *c.lists.borrow() == all && *c.hidden.borrow() == hidden {
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
        // A mark when shown (the row toggles it, see `build`); kept in place when hidden.
        let shown = !hidden.contains(&l.href);
        let mark = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .opacity(if shown { 1.0 } else { 0.0 })
            .build();
        row.append(&mark);
        row.set_tooltip_text(Some(if shown {
            "Shown on the Calendar"
        } else {
            "Hidden from the Calendar"
        }));
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
    *c.hidden.borrow_mut() = hidden;
}

/// The hrefs of the calendars (`events`) or task lists, in sidebar order.
fn section(c: &Cal, events: bool) -> Vec<String> {
    c.lists
        .borrow()
        .iter()
        .filter(|l| l.holds_events == events)
        .map(|l| l.href.clone())
        .collect()
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
    let first = month_first(anchor);
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
    let cell = |day: i64| {
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 2);
        cell.add_css_class("day");
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
        // Clicking a day selects it, a double click or a long press adds an event that day;
        // what is dropped on it moves there.
        let click = gtk::GestureClick::new();
        click.connect_released(move |_, n, _, _| {
            if n == 1 {
                select_in_month(day);
            }
        });
        cell.add_controller(click);
        new_event_on(&cell, move |_, _, _| {
            Some(EventTime {
                start: day,
                end: day + DAY,
                all_day: true,
            })
        });
        day_drop(&cell, day, false);
        cell
    };
    for w in 0..6 {
        let from = start + w * 7 * DAY;
        let here: Vec<&Item> = all
            .iter()
            .filter(|i| (0..7).any(|d| on_day(i, from + d * DAY)))
            .collect();
        let row = days::row(c, from, &here, CHIPS, false, cell);
        c.month.attach(&row, 0, w as i32, 1, 1);
    }
    end
}

/// Midnight of the first day of the month holding `day`.
fn month_first(day: i64) -> i64 {
    day - (dates::fields(day).day_of_month() as i64 - 1) * DAY
}

/// Select a day of the month grid. In the month shown only the highlight moves: a rebuilt
/// grid would lose the second click of a double click.
fn select_in_month(day: i64) {
    let Some(c) = cal() else { return };
    let first = month_first(day);
    if first != month_first(c.anchor.get()) {
        return go(|_| day);
    }
    c.anchor.set(day);
    let mut d = week_start(first);
    let mut row = c.month.first_child();
    while let Some(r) = row {
        for w in days::cells(&r) {
            if d == day {
                w.add_css_class("selected");
            } else {
                w.remove_css_class("selected");
            }
            d += DAY;
        }
        row = r.next_sibling();
    }
    mini::refresh(&c);
}

/// A double click, or a long press on a touchscreen, on `w` adds an event at the time `at`
/// gives for that point, if any.
fn new_event_on(
    w: &impl IsA<gtk::Widget>,
    at: impl Fn(&Rc<Cal>, f64, f64) -> Option<EventTime> + 'static,
) {
    let open = Rc::new(move |g: &gtk::Gesture, x, y| {
        let Some(c) = cal() else { return };
        if let Some(time) = at(&c, x, y) {
            g.set_state(gtk::EventSequenceState::Claimed);
            dialog::open(&c, None, time);
        }
    });
    let click = gtk::GestureClick::new();
    let o = open.clone();
    click.connect_pressed(move |g, n, x, y| {
        if n == 2 {
            o(g.upcast_ref(), x, y);
        }
    });
    w.add_controller(click);
    let hold = gtk::GestureLongPress::builder().touch_only(true).build();
    hold.connect_pressed(move |g, x, y| open(g.upcast_ref(), x, y));
    w.add_controller(hold);
}

/// "+N more": a popover with the rest of the day.
fn more_button(c: &Rc<Cal>, day: i64, rest: Vec<Item>) -> gtk::MenuButton {
    let label = gtk::Label::builder()
        .label(format!("+{} more", rest.len()))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
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
            b.set_tooltip_markup(Some(&tooltip(c, i)));
            if e.all_day {
                b.add_css_class("all-day");
            }
            if e.cancelled {
                b.add_css_class("cancelled");
            }
            drag_event(c, &b, e);
            let (weak, e) = (Rc::downgrade(c), e.clone());
            b.set_create_popup_func(move |b| {
                if let Some(c) = weak.upgrade() {
                    b.set_popover(Some(&details(&c, &e)));
                }
            });
            b.upcast()
        }
        Item::Task(t) => {
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
            task_chip(c, t, &label, gtk::Align::Center).upcast()
        }
    }
}

/// A task on the calendar: one button, opening it, that holds its check (`check_align`ed,
/// ticking it) and `label`.
fn task_chip(c: &Cal, t: &Task, label: &gtk::Label, check_align: gtk::Align) -> gtk::Button {
    let check = gtk::CheckButton::builder()
        .active(t.done)
        .valign(check_align)
        .build();
    let uid = t.uid.clone();
    check.connect_toggled(move |b| {
        if core().set_done(uid.clone(), b.is_active()).is_ok() {
            request_sync();
        }
    });
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.append(&check);
    if let Some(w) = crate::window::sync_warning(t) {
        w.set_margin_end(4);
        content.append(&w);
    }
    // The title takes the rest: left to its natural width, a wrapping label can end up a "…".
    label.set_hexpand(true);
    content.append(label);
    let b = gtk::Button::builder()
        .child(&content)
        .has_frame(false)
        .build();
    for cls in ["chip", "task", &class(c, &t.list_href)] {
        b.add_css_class(cls);
    }
    b.set_tooltip_markup(Some(&tooltip(c, &Item::Task(t.clone()))));
    if t.done {
        b.add_css_class("done");
    }
    let task = t.clone();
    b.connect_clicked(move |_| crate::window::edit_task(&task));
    // A button takes clicks in the capture phase, before its children; in the bubble phase the
    // check, being deeper, gets its own clicks first.
    let controllers = b.observe_controllers();
    for g in controllers.iter::<glib::Object>().flatten() {
        if let Ok(g) = g.downcast::<gtk::GestureClick>() {
            g.set_propagation_phase(gtk::PropagationPhase::Bubble);
        }
    }
    drag_task(&b, t);
    b
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

/// An event's or task's tooltip (markup): the whole title, when, where and in which list; the
/// views cut titles short.
fn tooltip(c: &Cal, i: &Item) -> String {
    let mut lines = Vec::new();
    let title = match i {
        Item::Event(e) => &e.summary,
        Item::Task(t) => &t.summary,
    };
    match i {
        Item::Event(e) => {
            lines.push(span_text(e));
            lines.extend(e.location.clone());
            if e.cancelled {
                lines.push("Cancelled".into());
            }
        }
        Item::Task(t) => {
            let at = |t: i64, all_day: bool| {
                let date = fmt(t, "%A %-d %B %Y");
                if all_day {
                    date
                } else {
                    format!("{date}, {}", fmt(t, "%H:%M"))
                }
            };
            match (t.start, t.due) {
                // Planned into a slot of one day: as an event's time.
                (Some(s), Some(d))
                    if !t.start_all_day && !t.due_all_day && fmt(s, "%F") == fmt(d, "%F") =>
                {
                    lines.push(format!("{} – {}", at(s, false), fmt(d, "%H:%M")))
                }
                (start, due) => {
                    lines.extend(start.map(|s| format!("Starts {}", at(s, t.start_all_day))));
                    lines.extend(due.map(|d| format!("Due {}", at(d, t.due_all_day))));
                }
            }
            lines.extend(t.category.clone().filter(|c| !c.is_empty()));
            if t.done {
                lines.push("Completed".into());
            }
            if t.sync_error.is_some() {
                lines.push("Not saved on the server".into());
            }
        }
    }
    lines.push(list_name(c, i.list()));
    let mut text = format!("<b>{}</b>", glib::markup_escape_text(title));
    for l in lines.iter().filter(|l| !l.is_empty()) {
        text.push('\n');
        text.push_str(&glib::markup_escape_text(l));
    }
    text
}

/// The tasks for an event: tick them, add one (to the task list used last, due when the event
/// starts).
fn event_tasks(e: &Event) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let head = gtk::Label::builder().xalign(0.0).margin_top(4).build();
    head.add_css_class("heading");
    head.set_label(&tasks_heading(&e.uid));
    b.append(&head);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for t in core().event_tasks(e.uid.clone()) {
        list.append(&task_check(&t, &head));
    }
    b.append(&list);
    let entry = gtk::Entry::builder()
        .placeholder_text("Add a task for this event")
        .build();
    let ev = e.clone();
    entry.connect_activate(move |entry| {
        let text = entry.text().trim().to_string();
        let lists = core().lists();
        let last = core().setting("last_list".into());
        let Some(list_href) = lists
            .iter()
            .find(|l| Some(&l.href) == last.as_ref())
            .or(lists.first())
            .map(|l| l.href.clone())
        else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let start = crate::settings::order().task_start;
        let patch = TaskPatch {
            event: Some(Some(ev.uid.clone())),
            due: Some(Some(Due {
                at: ev.start,
                all_day: ev.all_day,
            })),
            ..Default::default()
        };
        let added = core().together(|| {
            let t = core().add_task(list_href, text, None, None, start)?;
            core().update_task(t.uid.clone(), patch).map(|_| t)
        });
        if let Ok(t) = added {
            entry.set_text("");
            if let Some(t) = core().task(t.uid) {
                list.append(&task_check(&t, &head));
            }
            head.set_label(&tasks_heading(&ev.uid));
            request_sync();
        }
    });
    b.append(&entry);
    b
}

/// "Tasks · done/all" for an event.
fn tasks_heading(event_uid: &str) -> String {
    let tasks = core().event_tasks(event_uid.into());
    match tasks.len() {
        0 => "Tasks".into(),
        n => format!("Tasks · {}/{n}", tasks.iter().filter(|t| t.done).count()),
    }
}

/// A task of an event, to tick; `head` shows the count.
fn task_check(t: &Task, head: &gtk::Label) -> gtk::CheckButton {
    let check = gtk::CheckButton::builder()
        .label(&t.summary)
        .active(t.done)
        .build();
    let (uid, event, head) = (
        t.uid.clone(),
        t.event_uid.clone().unwrap_or_default(),
        head.clone(),
    );
    check.connect_toggled(move |c| {
        if core().set_done(uid.clone(), c.is_active()).is_ok() {
            head.set_label(&tasks_heading(&event));
            request_sync();
        }
    });
    check
}

/// An event's details: what, when, where, which calendar, notes, tasks.
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
    let which = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let dot = gtk::Box::builder().valign(gtk::Align::Center).build();
    dot.add_css_class("dot");
    dot.add_css_class(&class(c, &e.list_href));
    which.append(&dot);
    which.append(&gtk::Label::new(Some(&list_name(c, &e.list_href))));
    b.append(&which);
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
    b.append(&event_tasks(e));
    let pop = gtk::Popover::builder().child(&b).build();
    // Changes made here (a task ticked or added) redraw the views once it closes, not under it.
    pop.connect_show(|_| {
        if let Some(c) = cal() {
            c.holding.set(true);
        }
    });
    pop.connect_closed(|_| {
        if let Some(c) = cal() {
            c.holding.set(false);
            if c.pending.replace(false) {
                glib::idle_add_local_once(refresh);
            }
        }
    });
    if writable(c, &e.list_href) {
        let buttons = gtk::Box::builder()
            .spacing(6)
            .halign(gtk::Align::End)
            .margin_top(4)
            .build();
        let delete = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Delete")
            .build();
        delete.add_css_class("flat");
        let edit = gtk::Button::builder().label("Edit…").build();
        let (p, ev) = (pop.downgrade(), e.clone());
        edit.connect_clicked(move |_| {
            if let Some(p) = p.upgrade() {
                p.popdown();
            }
            if let Some(c) = cal() {
                let time = EventTime {
                    start: ev.start,
                    end: ev.end,
                    all_day: ev.all_day,
                };
                dialog::open(&c, Some(ev.clone()), time);
            }
        });
        let (p, ev) = (pop.downgrade(), e.clone());
        delete.connect_clicked(move |_| {
            let Some(p) = p.upgrade() else { return };
            p.popdown();
            let ev = ev.clone();
            let del = move |scope: Scope| match core().delete_event(
                ev.list_href.clone(),
                ev.uid.clone(),
                ev.recurrence_id,
                scope,
            ) {
                Ok(()) => {
                    request_sync();
                    crate::window::undo_toast(&format!("“{}” deleted", ev.summary));
                }
                Err(err) => crate::window::toast(&err.to_string()),
            };
            match cal() {
                Some(c) if ev.recurring => {
                    dialog::ask_scope(&c.split, "Delete Repeating Event?", true, del)
                }
                _ => del(Scope::All),
            }
        });
        buttons.append(&delete);
        buttons.append(&edit);
        b.append(&buttons);
    }
    pop
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
        let group = adw::PreferencesGroup::builder()
            .title(glib::markup_escape_text(&day_title(day, "%A %-d %B")))
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

/// A day's heading: its date in `format`, after "Today" or "Tomorrow" when it is.
fn day_title(day: i64, format: &str) -> String {
    let date = fmt(day, format);
    match (day - today()) / DAY {
        0 => format!("Today · {date}"),
        1 => format!("Tomorrow · {date}"),
        _ => capitalized(&date),
    }
}

/// Events in the calendars shown whose title, location or notes hold the search, within a
/// year either way: each series once, at its next time (or its last, when all are past); the
/// coming ones first.
fn render_search(c: &Rc<Cal>) {
    let query = c.query.borrow().clone();
    let (now, hidden) = (core().local_now(), hidden());
    let hit = |e: &Event| {
        [
            Some(&e.summary),
            e.location.as_ref(),
            e.description.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|s| crate::window::folded(s).contains(&query))
    };
    let ahead = |e: &Event| e.end > now;
    let mut best: HashMap<(String, String), Event> = HashMap::new();
    let all = core().events(today() - 366 * DAY, today() + 366 * DAY);
    for e in all
        .into_iter()
        .filter(|e| !hidden.contains(&e.list_href) && hit(e))
    {
        let key = (e.list_href.clone(), e.uid.clone());
        let better = best
            .get(&key)
            .is_none_or(|old| match (ahead(&e), ahead(old)) {
                (true, true) => e.start < old.start,
                (false, false) => e.start > old.start,
                (a, _) => a,
            });
        if better {
            best.insert(key, e);
        }
    }
    let mut found: Vec<Event> = best.into_values().collect();
    found.sort_by_key(|e| (!ahead(e), if ahead(e) { e.start } else { -e.start }));

    c.title.set_title("Search");
    c.title.set_subtitle(&match found.len() {
        1 => "1 event".to_string(),
        n => format!("{n} events"),
    });
    clear(&c.agenda);
    let mut group: Option<(i64, adw::PreferencesGroup)> = None;
    for e in found {
        let day = e.start - e.start.rem_euclid(DAY);
        if group.as_ref().is_none_or(|(d, _)| *d != day) {
            let g = adw::PreferencesGroup::builder()
                .title(glib::markup_escape_text(&day_title(day, "%A %-d %B %Y")))
                .build();
            c.agenda.append(&g);
            group = Some((day, g));
        }
        group
            .as_ref()
            .unwrap()
            .1
            .add(&agenda_row(c, &Item::Event(e)));
    }
    if group.is_none() {
        let empty = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title("No Results Found")
            .description("Try a different search")
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
