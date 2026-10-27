//! The Calendar page: events and dated tasks in a month grid, a week of hours or an agenda,
//! with the calendars (and task lists) to show and the tasks still to plan in the sidebar.
//! Times are local wall-clock seconds, as in the core.

mod days;
mod dialog;
mod mini;
mod week;

use crate::i18n::{gettext, ngettext, pgettext};
use crate::lists::class;
use crate::window::{get, tick, view};
use crate::{core, dates, request_sync};
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use todav_app::calendar::{self, Change, Item, Time, day_lists, on_day, planned, when};
use todav_app::dates::month_first;
use todav_app::prefs;
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
/* The day the extension (or a reminder) opened, pulsing twice as a task the Tasks page opens. */
@keyframes day-flash { from { background-color: transparent; } 50% { background-color: alpha(@accent_bg_color, 0.3); } to { background-color: transparent; } }
.flash { animation: day-flash 900ms ease-in-out 2; border-radius: 6px; }
.chip { padding: 0 6px; min-height: 22px; border-radius: 6px; font-size: smaller; }
.more button { font-weight: normal; }
menubutton.more > button { min-height: 0; min-width: 0; padding: 0; }
/* An event or task is a button itself: its colour is its background, so hover and press tint
   over it. */
button.chip { min-width: 0; font-weight: normal; }
button.chip:hover { background-image: linear-gradient(alpha(currentColor, 0.07), alpha(currentColor, 0.07)); }
button.chip:active { background-image: linear-gradient(alpha(currentColor, 0.16), alpha(currentColor, 0.16)); }
/* The +N more button fills its place, so its hover does too. */
menubutton.more { padding: 0; }
menubutton.more > button { padding: 0 6px; border-radius: 6px; }
.mini { padding: 6px; }
.mini .weekday { font-size: smaller; opacity: 0.6; padding: 4px 0; }
.mini button.day { min-width: 32px; min-height: 32px; padding: 0; font-weight: normal; font-feature-settings: 'tnum'; }
.mini button.day.other { opacity: 0.45; }
.mini button.day.today { box-shadow: inset 0 0 0 1px @accent_color; }
.mini button.day.selected { background: @accent_bg_color; color: @accent_fg_color; }
.mini .week-band { background: alpha(currentColor, 0.08); border-radius: 9999px; }
/* What a day holds: small dots in its calendars' colours; light ones on the selected day. */
.mini .day .dot { min-width: 4px; min-height: 4px; }
.mini .day box { min-height: 4px; }
.mini button.day.selected .dot { background-color: @accent_fg_color; }
/* Ends of a bar that goes on into the next or previous week (or past a full day) are square. */
.chip.from-before { border-top-left-radius: 0; border-bottom-left-radius: 0; }
.chip.goes-on { border-top-right-radius: 0; border-bottom-right-radius: 0; }
/* The check takes no more than itself, leaving the room to the title. */
.chip.task checkbutton { padding: 0; }
.chip.task check { min-width: 14px; min-height: 14px; margin: 0 4px 0 0; padding: 0; }
button.block.task { padding-left: 4px; }
.chip.done label { text-decoration-line: line-through; opacity: 0.55; }
.chip.cancelled label { text-decoration-line: line-through; }
.more { padding: 0 6px; min-height: 20px; font-size: smaller; }
.dot { min-width: 10px; min-height: 10px; border-radius: 9999px; }
.ring { min-width: 12px; min-height: 12px; border: 2px solid; border-radius: 9999px; }
.ring image { -gtk-icon-size: 10px; }
row.done label.title { text-decoration-line: line-through; opacity: 0.55; }
.day:drop(active), .all-day-cell:drop(active), .drop-over, list.plan:drop(active) { box-shadow: inset 0 0 0 2px @accent_bg_color; background: alpha(@accent_bg_color, 0.12); }
.week .weekdays label.today { color: @accent_color; font-weight: bold; opacity: 1; }
.all-day-row { border-bottom: 1px solid alpha(currentColor, 0.15); }
.all-day-cell { padding: 2px; min-height: 26px; }
.hours label { font-size: smaller; opacity: 0.6; font-feature-settings: 'tnum'; }
.block { border-radius: 6px; padding: 2px 6px; font-size: smaller; }
.block label { font-weight: normal; }
.block.task { border-left: 3px solid alpha(currentColor, 0.5); }
/* Opaque, so the block it is resized or moved from does not show through it. */
.drop-preview { border-radius: 6px; padding: 2px 6px; font-size: smaller; font-feature-settings: 'tnum'; background: color-mix(in srgb, @accent_bg_color 25%, @window_bg_color); box-shadow: inset 0 0 0 1px @accent_bg_color; }
.view-menu button { font-weight: normal; }
.drag-card { margin: 8px; padding: 8px 14px; border-radius: 10px; background: @card_bg_color; color: @card_fg_color; box-shadow: 0 2px 8px alpha(black, 0.25); }
";

struct Cal {
    /// The window's Tasks / Calendar stack.
    stack: adw::ViewStack,
    split: adw::OverlaySplitView,
    mini: mini::Mini,
    calendars: gtk::ListBox,
    task_head: gtk::Box,
    task_lists: gtk::ListBox,
    title: adw::WindowTitle,
    sync_state: gtk::Stack,
    search: gtk::SearchBar,
    /// The search, `folded`: while set, matching events show instead of the views.
    query: RefCell<String>,
    mode: Cell<Mode>,
    /// The view menus, in the header and in the bottom bar of a narrow window.
    view_buttons: [gtk::MenuButton; 2],
    views: gtk::Stack,
    weekdays: gtk::Box,
    month: gtk::Grid,
    agenda: gtk::Box,
    plan_heading: gtk::ToggleButton,
    plan: gtk::ListBox,
    /// The UIDs of the tasks in `plan`, by row.
    plan_uids: RefCell<Vec<String>>,
    /// The Day view's week, a row of the month grid.
    week_strip: gtk::Box,
    week_head: gtk::Box,
    week_all_day: gtk::Box,
    week_scroll: gtk::ScrolledWindow,
    week_grid: gtk::Overlay,
    week_lines: gtk::DrawingArea,
    hours: gtk::Box,
    /// The days the hours show, a column each (midnights).
    days: RefCell<Vec<i64>>,
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
    /// The day `show_day` went to, pulsing until then (as the Tasks page pulses a task).
    flash: Cell<Option<(i64, std::time::Instant)>>,
    /// Midnight of the day the views are around (the selected day).
    anchor: Cell<i64>,
    /// The anchor was reached by stepping (a month's first day), not picked: not highlighted.
    browsed: Cell<bool>,
    /// The collections the sidebar shows, in its order.
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
    todav_app::dates::midnight(core().local_now())
}

/// Add the Calendar page to the window's view stack, its sidebar shown and hidden with the Tasks
/// page's (`tasks`).
pub fn build(views: &adw::ViewStack, tasks: &adw::OverlaySplitView) {
    let b = view!("calendar");
    crate::window::add_banner(get(&b, "banner"), get(&b, "update_banner"));
    let split: adw::OverlaySplitView = get(&b, "split");
    tasks
        .bind_property("show-sidebar", &split, "show-sidebar")
        .bidirectional()
        .sync_create()
        .build();
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
    let (view, bar, sync): (gtk::MenuButton, gtk::ActionBar, gtk::Button) =
        (get(&b, "view"), get(&b, "mode_bar"), get(&b, "sync"));
    let to_bar = |bp: &adw::Breakpoint| {
        bp.add_setter(&view, "visible", Some(&false.to_value()));
        bp.add_setter(&bar, "revealed", Some(&true.to_value()));
        bp.add_setter(&sync, "visible", Some(&false.to_value()));
    };
    to_bar(&breakpoint("max-width: 880sp"));
    let narrow = breakpoint("max-width: 700sp");
    to_bar(&narrow);
    narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
    let page = views.add_titled(&bin, Some("calendar"), &gettext("Calendar"));
    page.set_icon_name(Some("x-office-calendar-symbolic"));
    get::<adw::InlineViewSwitcher>(&b, "switcher").set_stack(Some(views));
    let content_switcher: adw::InlineViewSwitcher = get(&b, "content_switcher");
    content_switcher.set_stack(Some(views));
    crate::window::sidebar_switcher(&split, &content_switcher);

    let display = split.display();
    let css = gtk::CssProvider::new();
    css.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let c = Rc::new(Cal {
        stack: views.clone(),
        split,
        mini: mini::Mini::new(&get(&b, "mini")),
        calendars: get(&b, "calendars"),
        task_head: get(&b, "task_head"),
        task_lists: get(&b, "task_lists"),
        title: get(&b, "title"),
        sync_state: get(&b, "sync_state"),
        search: get(&b, "search"),
        query: RefCell::new(String::new()),
        mode: Cell::new(Mode::from_name(&at_start(
            "start_calendar_view",
            "calendar_mode",
        ))),
        view_buttons: [view, get(&b, "view_narrow")],
        views: get(&b, "views"),
        weekdays: get(&b, "weekdays"),
        month: get(&b, "month"),
        agenda: get(&b, "agenda"),
        plan_heading: get(&b, "plan_heading"),
        plan: get(&b, "plan"),
        plan_uids: RefCell::new(Vec::new()),
        week_strip: get(&b, "week_strip"),
        week_head: get(&b, "week_head"),
        week_all_day: get(&b, "week_all_day"),
        week_scroll: get(&b, "week_scroll"),
        week_grid: get(&b, "week_grid"),
        week_lines: get(&b, "week_lines"),
        hours: get(&b, "hours"),
        days: RefCell::new(Vec::new()),
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
        flash: Cell::new(None),
        anchor: Cell::new(today()),
        browsed: Cell::new(false),
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
                prefs::set_hidden(core(), [href.as_str()], !hidden().contains(href));
                glib::idle_add_local_once(refresh);
            }
        });
        crate::lists::context_menu(list, "win.edit-list", move |row| {
            let c = cal()?;
            section(&c, events).get(row.index() as usize).cloned()
        });
    }
    for b in &c.view_buttons {
        b.set_create_popup_func(|b| b.set_popover(Some(&view_menu())));
    }
    label_views(&c);
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
        action("new-event", || {
            new_event();
        }),
        action("today", || go(|_| today())),
        action("previous", || step(-1)),
        action("next", || step(1)),
        gio::ActionEntry::builder("view")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(|_: &gio::SimpleActionGroup, _, view| {
                if let Some(v) = view.and_then(|v| v.str()) {
                    set_mode(Mode::from_name(v));
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

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Month,
    Week,
    /// The week's days the user picked (`prefs::work_days`).
    WorkWeek,
    /// Three days from the selected one.
    ThreeDays,
    /// The selected day, under its week from the month grid.
    Day,
    Agenda,
}

impl Mode {
    /// In the menu's order.
    const ALL: [Mode; 6] = [
        Mode::Month,
        Mode::Week,
        Mode::WorkWeek,
        Mode::ThreeDays,
        Mode::Day,
        Mode::Agenda,
    ];

    /// As stored in `calendar_mode` and `start_calendar_view`, and the `cal.view` action's target.
    fn name(self) -> &'static str {
        match self {
            Mode::Month => "month",
            Mode::Week => "week",
            Mode::WorkWeek => "work_week",
            Mode::ThreeDays => "three_days",
            Mode::Day => "day",
            Mode::Agenda => "agenda",
        }
    }

    fn from_name(name: &str) -> Mode {
        Mode::ALL
            .into_iter()
            .find(|m| m.name() == name)
            .unwrap_or(Mode::Month)
    }

    fn label(self) -> String {
        match self {
            Mode::Month => gettext("Month"),
            Mode::Week => gettext("Week"),
            Mode::WorkWeek => gettext("Work Week"),
            Mode::ThreeDays => gettext("3 Days"),
            Mode::Day => gettext("Day"),
            Mode::Agenda => gettext("Agenda"),
        }
    }

    /// Its Ctrl+number shortcut: the first three as before there were more views.
    fn key(self) -> u32 {
        match self {
            Mode::Month => 1,
            Mode::Week => 2,
            Mode::Agenda => 3,
            Mode::WorkWeek => 4,
            Mode::ThreeDays => 5,
            Mode::Day => 6,
        }
    }
}

fn mode(c: &Cal) -> Mode {
    c.mode.get()
}

fn set_mode(m: Mode) {
    let Some(c) = cal() else { return };
    c.mode.set(m);
    let _ = core().set_setting("calendar_mode".into(), m.name().into());
    label_views(&c);
    c.search.set_search_mode(false);
    refresh();
}

fn label_views(c: &Cal) {
    for b in &c.view_buttons {
        b.set_label(&c.mode.get().label());
    }
}

/// The views to pick from, the current one checked, their shortcuts beside them. Work Week's days
/// are picked in Preferences.
fn view_menu() -> gtk::Popover {
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let pop = gtk::Popover::builder().child(&list).build();
    pop.add_css_class("view-menu");
    let current = cal().map(|c| c.mode.get());
    for m in Mode::ALL {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.append(
            &gtk::Label::builder()
                .label(m.label())
                .xalign(0.0)
                .hexpand(true)
                .build(),
        );
        if let Some((key, mods)) = gtk::accelerator_parse(format!("<Control>{}", m.key())) {
            let accel = gtk::Label::new(Some(&gtk::accelerator_get_label(key, mods)));
            accel.add_css_class("dim-label");
            row.append(&accel);
        }
        let check = gtk::Image::from_icon_name("object-select-symbolic");
        check.set_opacity(if current == Some(m) { 1.0 } else { 0.0 });
        row.append(&check);
        let pick = gtk::Button::builder().child(&row).build();
        pick.add_css_class("flat");
        let p = pop.downgrade();
        pick.connect_clicked(move |_| {
            if let Some(p) = p.upgrade() {
                p.popdown();
            }
            set_mode(m);
        });
        list.append(&pick);
    }
    pop
}

/// Midnight of a day that is `weekday` (0 = Sunday): to name it.
fn a_day(weekday: i64) -> i64 {
    // 1970-01-04 was a Sunday.
    (3 + weekday) * DAY
}

/// The Work Week view's days, shortly: "Mon, Tue, Wed, Thu, Fri".
pub fn work_days_text() -> String {
    let first = crate::i18n::first_weekday();
    let days = prefs::work_days(core());
    (0..7)
        .map(|d| (first + d) % 7)
        .filter(|d| days.contains(d))
        .map(|d| fmt(a_day(d), "%a"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Pick the Work Week view's days, one to six, over `parent`; `changed` runs after each change.
pub fn work_days_dialog(parent: &impl IsA<gtk::Widget>, changed: impl Fn() + 'static) {
    let dialog = adw::AlertDialog::new(
        Some(&gettext("Work Week Days")),
        Some(&gettext("The days the Work Week view shows: one to six.")),
    );
    dialog.add_response("close", &gettext("Close"));
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    list.add_css_class("boxed-list");
    let first = crate::i18n::first_weekday();
    let rows: Vec<(i64, adw::SwitchRow)> = (0..7)
        .map(|d| {
            let day = (first + d) % 7;
            let row = adw::SwitchRow::builder()
                .title(capitalized(&fmt(a_day(day), "%A")))
                .build();
            list.append(&row);
            (day, row)
        })
        .collect();
    let rows = Rc::new(rows);
    // The last day left can't be turned off, nor a seventh on.
    let limit = {
        let rows = rows.clone();
        move || {
            let on = rows.iter().filter(|(_, r)| r.is_active()).count();
            for (_, r) in rows.iter() {
                r.set_sensitive(if r.is_active() { on > 1 } else { on < 6 });
            }
        }
    };
    let days = prefs::work_days(core());
    for (day, row) in rows.iter() {
        row.set_active(days.contains(day));
    }
    limit();
    let changed = Rc::new(changed);
    for (_, row) in rows.iter() {
        let (rows, limit, changed) = (rows.clone(), limit.clone(), changed.clone());
        row.connect_active_notify(move |_| {
            let on: Vec<i64> = rows
                .iter()
                .filter(|(_, r)| r.is_active())
                .map(|(d, _)| *d)
                .collect();
            prefs::set_work_days(core(), &on);
            limit();
            changed();
            refresh();
        });
    }
    dialog.set_extra_child(Some(&list));
    dialog.present(Some(parent));
}

/// Move to the day `f` picks from the current one.
fn go(f: impl FnOnce(i64) -> i64) {
    visit(f, false);
}

/// Move to the day `f` picks; `browsed`: reached by a step, not picked, so not highlighted.
fn visit(f: impl FnOnce(i64) -> i64, browsed: bool) {
    if let Some(c) = cal() {
        c.anchor.set(f(c.anchor.get()));
        c.browsed.set(browsed);
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
                visit(|_| dates::midnight(&f), true);
            }
        }
        Mode::Week | Mode::WorkWeek => visit(|a| a + by * 7 * DAY, true),
        // Any days next to each other: a day at a time, the first shown being the one picked.
        Mode::ThreeDays | Mode::Day => visit(|a| a + by * DAY, false),
        Mode::Agenda => visit(|a| a + by * AGENDA_STEP * DAY, true),
    }
}

/// A new event titled `summary` today (from the extension).
pub fn new_event_titled(summary: &str) {
    show_day(core().local_now());
    if let Some(title) = new_event() {
        title.set_text(summary);
        title.set_position(-1);
    }
}

/// A new event on the selected day, at the next full hour when that is today, else at 9:00;
/// returns the dialog's title field.
fn new_event() -> Option<adw::EntryRow> {
    let c = cal()?;
    let day = c.anchor.get();
    let now = core().local_now();
    let start = if day == today() {
        (now / 3600 + 1) * 3600
    } else {
        day + 9 * 3600
    };
    let time = EventTime {
        start,
        end: start + prefs::event_duration(core()),
        all_day: false,
    };
    Some(dialog::open(&c, None, time))
}

/// For the extension's agenda: what is on each of `days` days from today, as the Agenda view
/// lists it: (day, time, title, colour, is a task, done).
pub fn agenda(days: i64) -> Vec<(i64, String, String, String, bool, bool)> {
    calendar::agenda(core(), today(), days)
        .into_iter()
        .map(|e| {
            let time = match e.time {
                Time::AllDay => gettext("All day"),
                Time::Until(t) => gettext("Until {time}").replace("{time}", &fmt(t, "%H:%M")),
                Time::At(t) => fmt(t, "%H:%M"),
                Time::Date => String::new(),
            };
            let (task, done) = match &e.item {
                Item::Task(t) => (true, t.done),
                Item::Event(_) => (false, false),
            };
            (
                e.day,
                time,
                e.item.summary().to_string(),
                e.color,
                task,
                done,
            )
        })
        .collect()
}

/// Switch to the calendar on the day holding `t` (local time), the day pulsing once.
pub fn show_day(t: i64) {
    let Some(c) = cal() else { return };
    c.stack.set_visible_child_name("calendar");
    crate::window::show_content(&c.split);
    let day = t - t.rem_euclid(DAY);
    c.anchor.set(day);
    c.browsed.set(false);
    c.flash.set(Some((
        day,
        std::time::Instant::now() + crate::window::FLASH,
    )));
    c.search.set_search_mode(false);
    refresh();
}

/// Pulse `w` if it shows the day `show_day` went to, for what is left of the pulse.
fn flash_if(c: &Cal, w: &impl IsA<gtk::Widget>, day: i64) {
    let left = match c.flash.get() {
        Some((d, until)) if d == day => until.saturating_duration_since(std::time::Instant::now()),
        _ => return,
    };
    if left.is_zero() {
        c.flash.set(None);
        return;
    }
    let w = w.clone().upcast::<gtk::Widget>();
    w.add_css_class("flash");
    glib::timeout_add_local_once(left, move || w.remove_css_class("flash"));
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
        Mode::Agenda => {
            c.views.set_visible_child_name("agenda");
            render_agenda(&c);
            0
        }
        _ => {
            c.views.set_visible_child_name("week");
            week::render(&c)
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
    *c.plan_uids.borrow_mut() = shown.iter().map(|t| t.uid.clone()).collect();
    // Two groups: due by a date (they come first), then without one.
    let dated: Vec<bool> = shown.iter().map(|t| t.due.is_some()).collect();
    c.plan.set_header_func(move |row, _| {
        let i = row.index() as usize;
        let starts = dated.get(i).filter(|&&d| i == 0 || dated[i - 1] != d);
        let header = starts.map(|&d| {
            let l = gtk::Label::builder()
                .label(if d {
                    pgettext("to plan", "Due")
                } else {
                    gettext("No date")
                })
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
        dot.add_css_class(&class(&t.list_href));
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
            .label(gettext("Nothing to plan"))
            .xalign(0.0)
            .build();
        none.add_css_class("dim-label");
        c.plan.append(&none);
        if let Some(row) = none.parent().and_downcast::<gtk::ListBoxRow>() {
            row.set_activatable(false);
        }
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
    apply(i, calendar::day_drop(i, day, all_day));
}

/// Make the change a drop of `i` asks for.
fn apply(i: &Item, change: Change) {
    match (i, change) {
        (Item::Task(t), Change::Task(patch)) => save(&t.uid, patch),
        (Item::Event(e), Change::Event(time)) => move_event(e, time),
        _ => {}
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
            dialog::ask_scope(&c.split, &gettext("Move Repeating Event?"), false, apply)
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

/// Back to "To Plan": no slot, due on the day it was planned for.
fn unplan(t: &Task) {
    if let Some(patch) = calendar::unplan(t) {
        save(&t.uid, patch);
    }
}

fn plan_drop(c: &Rc<Cal>) {
    c.plan.add_css_class("plan");
    // A click does nothing but say how: they are planned by dragging.
    c.plan.connect_row_activated(|_, _| {
        crate::window::toast(&gettext("Drag a task onto the calendar to plan it"))
    });
    // Right-click or a long press: Edit…, its edit dialog.
    crate::lists::context_menu(&c.plan, "win.edit-task", |row| {
        cal()?.plan_uids.borrow().get(row.index() as usize).cloned()
    });
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

fn hidden() -> HashSet<String> {
    prefs::hidden(core())
}

/// Every collection's colour class, and a sidebar row each to show or hide it.
fn refresh_calendars(c: &Rc<Cal>) {
    crate::lists::paint();
    let mut all = core().calendars();
    for l in core().lists() {
        if !all.iter().any(|a| a.href == l.href) {
            all.push(l);
        }
    }
    // Calendars, then task lists, each grouped by account, in the user's order.
    let rank = todav_app::lists::rank(core());
    all.sort_by_key(|l| (!l.holds_events, rank(l.account)));
    let hidden = hidden();
    if *c.lists.borrow() == all && *c.hidden.borrow() == hidden {
        return;
    }
    c.calendars.remove_all();
    c.task_lists.remove_all();
    for l in &all {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.append(&crate::lists::dot(&l.href));
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
        row.set_tooltip_text(Some(&if shown {
            gettext("Shown on the Calendar")
        } else {
            gettext("Hidden from the Calendar")
        }));
        if l.holds_events {
            c.calendars.append(&row);
        } else {
            c.task_lists.append(&row);
        }
        if let Some(r) = row.parent().and_downcast::<gtk::ListBoxRow>() {
            if l.writable {
                list_drop(&r, l);
            }
            let siblings = all
                .iter()
                .filter(|s| s.holds_events == l.holds_events && s.account == l.account);
            crate::lists::reorderable(&r, l, siblings.map(|s| s.href.clone()).collect());
        }
    }
    for (rows, events) in [(&c.calendars, true), (&c.task_lists, false)] {
        let section: Vec<List> = all
            .iter()
            .filter(|l| l.holds_events == events)
            .cloned()
            .collect();
        crate::lists::set_headings(rows, 0, crate::lists::headings(&section));
    }
    let lists_only = all.iter().any(|l| !l.holds_events);
    c.task_head.set_visible(lists_only);
    c.task_lists.set_visible(lists_only);
    *c.lists.borrow_mut() = all;
    *c.hidden.borrow_mut() = hidden;
}

/// Dropping an event on sidebar row `l` moves it to that calendar (all of a repeating one), a
/// task moves to that list with its subtasks (a subtask leaves its parent first).
fn list_drop(row: &gtk::ListBoxRow, l: &List) {
    let drop = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
    let (tasks, events, href) = (l.holds_tasks, l.holds_events, l.href.clone());
    drop.connect_accept(move |_, _| {
        dragging().is_some_and(|i| match &i {
            Item::Task(t) => tasks && t.list_href != href,
            Item::Event(e) => events && e.list_href != href,
        })
    });
    let href = l.href.clone();
    drop.connect_drop(move |_, _, _, _| {
        let Some(i) = dragging() else { return false };
        match calendar::move_to_list(core(), &i, href.clone()) {
            Ok(()) => request_sync(),
            Err(e) => crate::window::toast(&e.to_string()),
        }
        true
    });
    row.add_css_class("cat-drop");
    row.add_controller(drop);
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

fn list_name(c: &Cal, list: &str) -> String {
    let lists = c.lists.borrow();
    lists
        .iter()
        .find(|l| l.href == list)
        .map_or(String::new(), |l| l.display_name.clone())
}

/// Visible events and dated tasks in `[from, to)`.
fn items(from: i64, to: i64) -> Vec<Item> {
    calendar::items(core(), from, to)
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

/// Midnight of the first day of the week holding `day`, the locale's first weekday.
fn week_start(day: i64) -> i64 {
    todav_app::dates::week_start(day, crate::i18n::first_weekday())
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
    let month = d.month();
    for w in 0..6 {
        let from = start + w * 7 * DAY;
        let week: Vec<i64> = (0..7).map(|d| from + d * DAY).collect();
        let here: Vec<&Item> = all
            .iter()
            .filter(|i| week.iter().any(|&d| on_day(i, d)))
            .collect();
        let row = days::row(c, &week, &here, CHIPS, false, |day| {
            day_cell(c, day, dates::fields(day).month() != month, select_in_month)
        });
        c.month.attach(&row, 0, w as i32, 1, 1);
    }
    end
}

/// A day of the month grid: its number (dimmer when `other`, from another month), today and the
/// selected day marked. Clicking it calls `select`, a double click or a long press adds an
/// event that day; what is dropped on it moves there.
fn day_cell(c: &Rc<Cal>, day: i64, other: bool, select: fn(i64)) -> gtk::Box {
    let cell = gtk::Box::new(gtk::Orientation::Vertical, 2);
    cell.add_css_class("day");
    if other {
        cell.add_css_class("other");
    }
    if day == today() {
        cell.add_css_class("today");
    }
    if day == c.anchor.get() && !c.browsed.get() {
        cell.add_css_class("selected");
    }
    let num = gtk::Label::builder()
        .label(fmt(day, "%-d"))
        .halign(gtk::Align::End)
        .build();
    num.add_css_class("num");
    cell.append(&num);
    let click = gtk::GestureClick::new();
    click.connect_released(move |_, n, _, _| {
        if n == 1 {
            select(day);
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
    flash_if(c, &cell, day);
    cell
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
    c.browsed.set(false);
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
        .label(
            ngettext("+{n} more", "+{n} more", rest.len() as u64)
                .replace("{n}", &rest.len().to_string()),
        )
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

/// An item's title on one line, with its time (if any) dimmer at the end, as GNOME Calendar
/// shows them.
pub fn one_line(summary: &str, time: Option<String>) -> gtk::Widget {
    let title = gtk::Label::builder()
        .label(summary)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    let Some(time) = time else {
        return title.upcast();
    };
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.append(&title);
    let time = gtk::Label::new(Some(&time));
    time.add_css_class("dim-label");
    row.append(&time);
    row.upcast()
}

/// A month-day entry: an event (opens its details) or a task (tick, or open it).
fn chip(c: &Rc<Cal>, i: &Item) -> gtk::Widget {
    match i {
        Item::Event(e) => {
            let time = (!e.all_day).then(|| fmt(e.start, "%H:%M"));
            let b = gtk::Button::builder()
                .child(&one_line(&e.summary, time))
                .has_frame(false)
                .build();
            b.add_css_class("chip");
            b.add_css_class(&class(&e.list_href));
            b.set_tooltip_markup(Some(&tooltip(c, i)));
            if e.all_day {
                b.add_css_class("all-day");
            }
            if e.cancelled {
                b.add_css_class("cancelled");
            }
            drag_event(c, &b, e);
            event_clicks(&b, e);
            b.upcast()
        }
        Item::Task(t) => {
            let (at, all_day) = when(t);
            let time = (!all_day).then(|| fmt(at, "%H:%M"));
            task_chip(c, t, &one_line(&t.summary, time), gtk::Align::Center).upcast()
        }
    }
}

/// A task on the calendar: one button, showing its details, that holds its check
/// (`check_align`ed, ticking it) and `label`.
fn task_chip(c: &Rc<Cal>, t: &Task, label: &gtk::Widget, check_align: gtk::Align) -> gtk::Button {
    let check = gtk::CheckButton::builder()
        .active(t.done)
        .valign(check_align)
        .build();
    let uid = t.uid.clone();
    check.connect_toggled(move |b| tick(b, &uid, || {}));
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
    for cls in ["chip", "task", &class(&t.list_href)] {
        b.add_css_class(cls);
    }
    b.set_tooltip_markup(Some(&tooltip(c, &Item::Task(t.clone()))));
    if t.done {
        b.add_css_class("done");
    }
    // A button takes clicks in the capture phase, before its children; in the bubble phase the
    // check, being deeper, gets its own clicks first.
    let controllers = b.observe_controllers();
    for g in controllers.iter::<glib::Object>().flatten() {
        if let Ok(g) = g.downcast::<gtk::GestureClick>() {
            g.set_propagation_phase(gtk::PropagationPhase::Bubble);
        }
    }
    // After the above, which would move its gesture to the bubble phase too.
    let (task, task2) = (t.clone(), t.clone());
    let open = on_clicks(
        &b,
        move || cal().map(|c| task_details(&c, &task)),
        move || crate::window::edit_task(&task2),
    );
    b.connect_clicked(move |_| open());
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

/// A task's dates in words: its slot as an event's time when planned within a day, else when
/// it starts and is due.
fn task_when(t: &Task) -> Vec<String> {
    let at = |t: i64, all_day: bool| {
        let date = fmt(t, "%A %-d %B %Y");
        if all_day {
            date
        } else {
            format!("{date}, {}", fmt(t, "%H:%M"))
        }
    };
    match (t.start, t.due) {
        (Some(s), Some(d))
            if !t.start_all_day && !t.due_all_day && fmt(s, "%F") == fmt(d, "%F") =>
        {
            vec![format!("{} – {}", at(s, false), fmt(d, "%H:%M"))]
        }
        (start, due) => start
            .map(|s| gettext("Starts {date}").replace("{date}", &at(s, t.start_all_day)))
            .into_iter()
            .chain(due.map(|d| gettext("Due {date}").replace("{date}", &at(d, t.due_all_day))))
            .collect(),
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
                lines.push(gettext("Cancelled"));
            }
        }
        Item::Task(t) => {
            lines.extend(task_when(t));
            lines.extend(t.category.clone().filter(|c| !c.is_empty()));
            if t.done {
                lines.push(gettext("Completed"));
            }
            if t.sync_error.is_some() {
                lines.push(gettext("Not saved on the server"));
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

/// A heading counting done/all of `tasks`, a box to tick each, and (with `add`) a field adding
/// one: `add` makes it from the title typed. The event's tasks, a task's subtasks.
fn checklist(
    title: String,
    hint: &str,
    tasks: Rc<dyn Fn() -> Vec<Task>>,
    add: Option<Box<dyn Fn(String) -> Option<Task>>>,
) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let head = gtk::Label::builder().xalign(0.0).margin_top(4).build();
    head.add_css_class("heading");
    let count: Rc<dyn Fn()> = {
        let (tasks, head) = (tasks.clone(), head.clone());
        Rc::new(move || {
            let all = tasks();
            head.set_label(&match all.len() {
                0 => title.clone(),
                n => format!("{title} · {}/{n}", all.iter().filter(|t| t.done).count()),
            });
        })
    };
    count();
    b.append(&head);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for t in tasks() {
        list.append(&check_row(&t, count.clone()));
    }
    b.append(&list);
    if let Some(add) = add {
        let entry = gtk::Entry::builder().placeholder_text(hint).build();
        entry.connect_activate(move |entry| {
            let text = entry.text().trim().to_string();
            if text.is_empty() {
                return;
            }
            if let Some(t) = add(text) {
                entry.set_text("");
                list.append(&check_row(&t, count.clone()));
                count();
                request_sync();
            }
        });
        b.append(&entry);
    }
    b
}

/// A task to tick; `count` updates the heading above it.
fn check_row(t: &Task, count: Rc<dyn Fn()>) -> gtk::CheckButton {
    let check = gtk::CheckButton::builder()
        .label(&t.summary)
        .active(t.done)
        .build();
    let uid = t.uid.clone();
    check.connect_toggled(move |c| {
        let count = count.clone();
        tick(c, &uid, move || count())
    });
    check
}

/// The tasks for an event: tick them, add one (to the task list used last, due when the event
/// starts).
fn event_tasks(e: &Event) -> gtk::Box {
    let uid = e.uid.clone();
    let ev = e.clone();
    let add = move |text: String| {
        let lists = core().lists();
        let last = core().setting("last_list".into());
        let list_href = lists
            .iter()
            .find(|l| Some(&l.href) == last.as_ref())
            .or(lists.first())?
            .href
            .clone();
        let start = prefs::order(core()).task_start;
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
        added.ok().and_then(|t| core().task(t.uid))
    };
    checklist(
        gettext("Tasks"),
        &gettext("Add a task for this event"),
        Rc::new(move || core().event_tasks(uid.clone())),
        Some(Box::new(add)),
    )
}

/// A task's subtasks: tick them, add one.
fn subtasks(t: &Task) -> gtk::Box {
    let (list, uid) = (t.list_href.clone(), t.uid.clone());
    let all = {
        let (list, uid) = (list.clone(), uid.clone());
        move || {
            core()
                .tasks(list.clone(), true)
                .into_iter()
                .filter(|s| s.parent_uid.as_ref() == Some(&uid))
                .collect()
        }
    };
    let add = move |text: String| {
        let start = prefs::order(core()).sub_start;
        core()
            .add_task(list.clone(), text, None, Some(uid.clone()), start)
            .ok()
    };
    checklist(
        gettext("Subtasks"),
        &gettext("Add a subtask"),
        Rc::new(all),
        Some(Box::new(add)),
    )
}

/// The content of a details popover, and an icon-led line of it.
fn details_box() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 8);
    b.set_margin_start(6);
    b.set_margin_end(6);
    b.set_margin_top(6);
    b.set_margin_bottom(6);
    b.set_width_request(280);
    b
}

fn line(icon: &str, text: &str) -> gtk::Box {
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
}

/// The collection's dot and `name`.
fn which(list: &str, name: &str) -> gtk::Box {
    let which = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let dot = gtk::Box::builder().valign(gtk::Align::Center).build();
    dot.add_css_class("dot");
    dot.add_css_class(&class(list));
    which.append(&dot);
    which.append(&gtk::Label::new(Some(name)));
    which
}

/// When the reminders go off, one a line.
fn reminders_line(at: &[i64]) -> Option<gtk::Box> {
    let now = core().local_now();
    let at: Vec<String> = at.iter().map(|r| dates::label(*r, false, now)).collect();
    (!at.is_empty()).then(|| line("alarm-symbolic", &at.join("\n")))
}

/// What a details popover's Delete and Edit… buttons do.
type Buttons = (Box<dyn Fn()>, Box<dyn Fn()>);

/// A details popover holding `b`, with Delete and Edit… under it when `buttons` are given.
/// Changes made in it (a task ticked or added) redraw the views once it closes, not under it.
fn details_popover(b: &gtk::Box, buttons: Option<Buttons>) -> gtk::Popover {
    let pop = gtk::Popover::builder().child(b).build();
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
    if let Some((on_delete, on_edit)) = buttons {
        // Delete at the left edge, away from Edit… at the right.
        let row = gtk::Box::builder().spacing(6).margin_top(4).build();
        let delete = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text(gettext("Delete"))
            .build();
        delete.add_css_class("flat");
        let edit = gtk::Button::builder()
            .label(gettext("Edit…"))
            .hexpand(true)
            .halign(gtk::Align::End)
            .build();
        let p = pop.downgrade();
        delete.connect_clicked(move |_| {
            if let Some(p) = p.upgrade() {
                p.popdown();
            }
            on_delete();
        });
        let p = pop.downgrade();
        edit.connect_clicked(move |_| {
            if let Some(p) = p.upgrade() {
                p.popdown();
            }
            on_edit();
        });
        row.append(&delete);
        row.append(&edit);
        b.append(&row);
    }
    pop
}

/// Clicks on `w`, an item on the calendar: one shows its `details`, two open `edit`. The details
/// wait out the double-click time, as their popover's grab would take a second click; with
/// "Edit on double click" off they show at once and two clicks do nothing more. Returns
/// what `w`'s clicked (or activated) signal does; a check in `w` ticks however fast.
fn on_clicks(
    w: &impl IsA<gtk::Widget>,
    details: impl Fn() -> Option<gtk::Popover> + 'static,
    edit: impl Fn() + 'static,
) -> impl Fn() + 'static {
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    // The second click's own clicked signal, after `edit`.
    let skip = Rc::new(Cell::new(false));
    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (p, s) = (pending.clone(), skip.clone());
    click.connect_pressed(move |g, n, x, y| {
        let on_check = g
            .widget()
            .and_then(|w| w.pick(x, y, gtk::PickFlags::DEFAULT))
            .is_some_and(|t| t.ancestor(gtk::CheckButton::static_type()).is_some());
        if n != 2 || on_check || !prefs::double_click_edit(core()) {
            s.set(false);
            return;
        }
        if let Some(id) = p.take() {
            id.remove();
        }
        s.set(true);
        edit();
    });
    w.add_controller(click);
    let w = w.upcast_ref::<gtk::Widget>().downgrade();
    let details = Rc::new(details);
    move || {
        if skip.replace(false) {
            return;
        }
        // Without editing on a double click there is nothing to wait for.
        if !prefs::double_click_edit(core()) {
            if let (Some(w), Some(pop)) = (w.upgrade(), details()) {
                pop_up(pop, &w);
            }
            return;
        }
        let wait = gtk::Settings::default().map_or(400, |s| s.gtk_double_click_time());
        let (w, details, p) = (w.clone(), details.clone(), pending.clone());
        let id = glib::timeout_add_local_once(
            std::time::Duration::from_millis(wait.max(0) as u64),
            move || {
                p.take();
                if let (Some(w), Some(pop)) = (w.upgrade(), details()) {
                    pop_up(pop, &w);
                }
            },
        );
        if let Some(old) = pending.replace(Some(id)) {
            old.remove();
        }
    }
}

/// An event's button: a click shows its details, a double click edits it.
fn event_clicks(b: &gtk::Button, e: &Event) {
    let (ev, ev2) = (e.clone(), e.clone());
    let open = on_clicks(
        b,
        move || cal().map(|c| details(&c, &ev)),
        move || edit_event(&ev2),
    );
    b.connect_clicked(move |_| open());
}

/// Edit an event, if its calendar may be written to.
fn edit_event(e: &Event) {
    let Some(c) = cal() else { return };
    if !writable(&c, &e.list_href) {
        return;
    }
    let time = EventTime {
        start: e.start,
        end: e.end,
        all_day: e.all_day,
    };
    dialog::open(&c, Some(e.clone()), time);
}

/// Show `pop` from `parent`, which is not a menu button.
fn pop_up(pop: gtk::Popover, parent: &impl IsA<gtk::Widget>) {
    pop.set_parent(parent);
    pop.connect_closed(|p| {
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
    });
    pop.popup();
}

/// A details popover's title; with `rename`, a click edits it in place, Enter (or clicking
/// elsewhere) saves it, Escape keeps the old one.
fn title_label(text: &str, rename: Option<Box<dyn Fn(String)>>) -> gtk::Widget {
    let Some(rename) = rename else {
        let l = gtk::Label::builder()
            .label(text)
            .xalign(0.0)
            .wrap(true)
            .hexpand(true)
            .build();
        l.add_css_class("title-4");
        return l.upcast();
    };
    let l = gtk::EditableLabel::builder()
        .text(text)
        .hexpand(true)
        .tooltip_text(gettext("Click to rename"))
        .build();
    l.add_css_class("title-4");
    let old = RefCell::new(text.to_string());
    l.connect_editing_notify(move |l| {
        let new = l.text().trim().to_string();
        if l.is_editing() || new.is_empty() || new == *old.borrow() {
            return;
        }
        *old.borrow_mut() = new.clone();
        rename(new);
    });
    l.upcast()
}

/// An event's details: what, when, where, which calendar, notes, tasks.
fn details(c: &Cal, e: &Event) -> gtk::Popover {
    let b = details_box();
    let ev = e.clone();
    let rename = move |name: String| {
        let ev = ev.clone();
        let apply = move |scope: Scope| {
            let patch = EventPatch {
                summary: Some(name.clone()),
                ..Default::default()
            };
            let (list, uid) = (ev.list_href.clone(), ev.uid.clone());
            match core().update_event(list, uid, ev.recurrence_id, scope, patch) {
                Ok(()) => request_sync(),
                Err(err) => crate::window::toast(&err.to_string()),
            }
        };
        match cal() {
            Some(c) if ev.recurring => {
                dialog::ask_scope(&c.split, &gettext("Rename Repeating Event?"), false, apply)
            }
            _ => apply(Scope::All),
        }
    };
    let rename = writable(c, &e.list_href).then(|| Box::new(rename) as Box<dyn Fn(String)>);
    b.append(&title_label(&e.summary, rename));
    b.append(&line("x-office-calendar-symbolic", &span_text(e)));
    if e.recurring {
        b.append(&line("media-playlist-repeat-symbolic", &gettext("Repeats")));
    }
    if let Some(l) = &e.location {
        b.append(&line("mark-location-symbolic", l));
    }
    b.append(&which(&e.list_href, &list_name(c, &e.list_href)));
    if let Some(r) = reminders_line(&e.reminders) {
        b.append(&r);
    }
    if let Some(d) = &e.description {
        b.append(&line("text-x-generic-symbolic", d));
    }
    b.append(&event_tasks(e));
    let buttons = writable(c, &e.list_href).then(|| {
        let ev = e.clone();
        let on_edit = move || edit_event(&ev);
        let ev = e.clone();
        let on_delete = move || {
            let ev = ev.clone();
            let del = move |scope: Scope| match core().delete_event(
                ev.list_href.clone(),
                ev.uid.clone(),
                ev.recurrence_id,
                scope,
            ) {
                Ok(()) => {
                    request_sync();
                    crate::window::undo_toast(
                        &gettext("“{title}” deleted").replace("{title}", &ev.summary),
                    );
                }
                Err(err) => crate::window::toast(&err.to_string()),
            };
            match cal() {
                Some(c) if ev.recurring => {
                    dialog::ask_scope(&c.split, &gettext("Delete Repeating Event?"), true, del)
                }
                _ => del(Scope::All),
            }
        };
        (
            Box::new(on_delete) as Box<dyn Fn()>,
            Box::new(on_edit) as Box<dyn Fn()>,
        )
    });
    details_popover(&b, buttons)
}

/// A task's details, as an event's: tick it, when, which list and category, its event, notes,
/// the task a subtask is under, and its own subtasks.
fn task_details(c: &Cal, t: &Task) -> gtk::Popover {
    let b = details_box();
    let check = gtk::CheckButton::builder()
        .active(t.done)
        .valign(gtk::Align::Center)
        .tooltip_text(gettext("Done"))
        .build();
    let uid = t.uid.clone();
    check.connect_toggled(move |b| tick(b, &uid, || {}));
    let uid = t.uid.clone();
    let rename = move |name: String| {
        let patch = TaskPatch {
            summary: Some(name),
            ..Default::default()
        };
        save(&uid, patch);
    };
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    head.append(&check);
    head.append(&title_label(&t.summary, Some(Box::new(rename))));
    b.append(&head);
    if let Some(e) = &t.sync_error {
        b.append(&line(
            "dialog-warning-symbolic",
            &gettext("Not saved on the server: {reason}")
                .replace("{reason}", crate::window::server_reason(e)),
        ));
    }
    let when = task_when(t);
    if !when.is_empty() {
        b.append(&line("x-office-calendar-symbolic", &when.join("\n")));
    }
    let name = match t.category.as_deref().filter(|c| !c.is_empty()) {
        Some(cat) => format!("{} · {cat}", list_name(c, &t.list_href)),
        None => list_name(c, &t.list_href),
    };
    b.append(&which(&t.list_href, &name));
    if let Some(p) = crate::window::priority_text(t.priority) {
        b.append(&line("emblem-important-symbolic", &p));
    }
    if let Some(e) = t.event_uid.clone().and_then(|e| core().event(e)) {
        b.append(&line(
            "insert-link-symbolic",
            &gettext("For {event}").replace("{event}", &e.summary),
        ));
    }
    if let Some(r) = reminders_line(&t.reminders) {
        b.append(&r);
    }
    if let Some(d) = t.description.as_deref().filter(|d| !d.is_empty()) {
        b.append(&line("text-x-generic-symbolic", d));
    }
    if let Some(parent) = t.parent_uid.clone().and_then(|p| core().task(p)) {
        b.append(&line(
            "view-list-bullet-symbolic",
            &gettext("Subtask of {task}").replace("{task}", &parent.summary),
        ));
    }
    b.append(&subtasks(t));
    let task = t.clone();
    let on_delete = move || crate::window::delete_task(&task);
    let task = t.clone();
    let on_edit = move || crate::window::edit_task(&task);
    details_popover(&b, Some((Box::new(on_delete), Box::new(on_edit))))
}

fn render_agenda(c: &Rc<Cal>) {
    let from = c.anchor.get();
    let to = from + AGENDA_DAYS * DAY;
    c.title.set_title(&gettext("Agenda"));
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
            group.add(&agenda_row(i));
        }
        flash_if(c, &group, day);
        c.agenda.append(&group);
    }
    if !any {
        let empty = adw::StatusPage::builder()
            .icon_name("x-office-calendar-symbolic")
            .title(gettext("Nothing Planned"))
            .build();
        empty.add_css_class("compact");
        c.agenda.append(&empty);
    }
}

/// A day's heading: its date in `format`, after "Today" or "Tomorrow" when it is.
fn day_title(day: i64, format: &str) -> String {
    let date = fmt(day, format);
    match (day - today()) / DAY {
        0 => format!("{} · {date}", gettext("Today")),
        1 => format!("{} · {date}", gettext("Tomorrow")),
        _ => capitalized(&date),
    }
}

/// Events in the calendars shown whose title, location or notes hold the search, within a
/// year either way: each series once, at its next time (or its last, when all are past); the
/// coming ones first.
fn render_search(c: &Rc<Cal>) {
    let query = c.query.borrow().clone();
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
    let found = calendar::search(core(), core().local_now(), hit);

    c.title.set_title(&gettext("Search"));
    let n = found.len() as u64;
    c.title
        .set_subtitle(&ngettext("{n} event", "{n} events", n).replace("{n}", &n.to_string()));
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
        group.as_ref().unwrap().1.add(&agenda_row(&Item::Event(e)));
    }
    if group.is_none() {
        let empty = adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title(gettext("No Results Found"))
            .description(gettext("Try a different search"))
            .build();
        empty.add_css_class("compact");
        c.agenda.append(&empty);
    }
}

/// An agenda row's mark in its list's colour, as the extension's and Android's: an event's dot
/// (`done` None) or a task's larger ring, a tick in it when done, in a slot as wide either way so
/// the titles line up. Only a mark: a task is ticked in its details.
fn mark(list: &str, done: Option<bool>) -> gtk::Box {
    let slot = gtk::Box::builder()
        .width_request(18)
        .valign(gtk::Align::Center)
        .build();
    // Not wider than the slot: the tick's expanding (to centre it in the ring) stops here.
    let m = gtk::Box::builder()
        .hexpand(false)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    m.add_css_class(if done.is_some() { "ring" } else { "dot" });
    m.add_css_class(&class(list));
    if done == Some(true) {
        let tick = gtk::Image::from_icon_name("object-select-symbolic");
        tick.set_hexpand(true);
        m.set_tooltip_text(Some(&gettext("Done")));
        m.append(&tick);
    }
    slot.append(&m);
    slot
}

fn agenda_row(i: &Item) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .use_markup(false)
        .activatable(true)
        .build();
    match i {
        Item::Event(e) => {
            row.set_title(&e.summary);
            let time = if e.all_day {
                gettext("All day")
            } else {
                format!("{} – {}", fmt(e.start, "%H:%M"), fmt(e.end, "%H:%M"))
            };
            let sub = match &e.location {
                Some(l) => format!("{time} · {l}"),
                None => time,
            };
            row.set_subtitle(&sub);
            row.add_prefix(&mark(&e.list_href, None));
            let (ev, ev2) = (e.clone(), e.clone());
            let open = on_clicks(
                &row,
                move || cal().map(|c| details(&c, &ev)),
                move || edit_event(&ev2),
            );
            row.connect_activated(move |_| open());
        }
        Item::Task(t) => {
            row.set_title(&t.summary);
            let (at, all_day) = when(t);
            let task = gettext("Task");
            row.set_subtitle(&if all_day {
                task
            } else {
                format!("{task} · {}", fmt(at, "%H:%M"))
            });
            row.add_prefix(&mark(&t.list_href, Some(t.done)));
            if t.done {
                row.add_css_class("done");
            }
            let (task, task2) = (t.clone(), t.clone());
            let open = on_clicks(
                &row,
                move || cal().map(|c| task_details(&c, &task)),
                move || crate::window::edit_task(&task2),
            );
            row.connect_activated(move |_| open());
        }
    }
    row
}
