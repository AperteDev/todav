//! Settings the app reads, with their defaults, so every front-end reads them alike. They are
//! stored in the core (`Client::setting`), synced unless listed in `DEVICE_ONLY`.

use std::collections::HashSet;
use todav_core::{Client, DoneView, Sort};

/// Stored values of the "Sort" choices, in the order the front-ends offer them.
pub const SORTS: [(&str, Sort); 6] = [
    ("manual", Sort::Manual),
    ("newest", Sort::Newest),
    ("oldest", Sort::Oldest),
    ("due_soonest", Sort::DueSoonest),
    ("due_latest", Sort::DueLatest),
    ("priority", Sort::Priority),
];

/// Where `key` ("task_sort", "sub_sort") is in `SORTS`; manual by default.
pub fn sort_index(c: &Client, key: &str) -> usize {
    let v = c.setting(key.into()).unwrap_or_default();
    SORTS.iter().position(|(s, _)| *s == v).unwrap_or(0)
}

/// Where tasks show up and where new ones go; date sorting decides the latter.
pub struct Order {
    pub tasks: Sort,
    pub subtasks: Sort,
    pub task_start: bool,
    pub sub_start: bool,
}

pub fn order(c: &Client) -> Order {
    let sort = |key| SORTS[sort_index(c, key)].1;
    let start = |sort, key: &str| match sort {
        Sort::Newest => true,
        Sort::Oldest => false,
        // New tasks have no due date: they go among the undated ones, as in manual order.
        _ => c.setting(key.into()).as_deref() == Some("start"),
    };
    let (tasks, subtasks) = (sort("task_sort"), sort("sub_sort"));
    Order {
        tasks,
        subtasks,
        task_start: start(tasks, "task_new"),
        sub_start: start(subtasks, "sub_new"),
    }
}

/// How completed tasks are shown ("done_tasks", "done_subtasks").
pub fn done_view(c: &Client) -> DoneView {
    let tasks = c.setting("done_tasks".into()).unwrap_or_default();
    let subtasks = c.setting("done_subtasks".into()).unwrap_or_default();
    DoneView {
        in_place: tasks == "in_place" || tasks == "last",
        tasks_last: tasks == "last",
        hide_subtasks: subtasks == "hide",
        subtasks_last: subtasks == "last",
    }
}

/// What completing a task with open subtasks does (`complete_subtasks`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtaskCompletion {
    /// They are completed too, and reopening one reopens its completed parents (as in Nextcloud
    /// Tasks): a completed task never has open subtasks. The default.
    Complete,
    /// They stay open, under their completed parent.
    Keep,
    /// The front-end asks which of the two; where it cannot, they stay open.
    Ask,
}

pub fn subtask_completion(c: &Client) -> SubtaskCompletion {
    match c.setting("complete_subtasks".into()).as_deref() {
        Some("keep") => SubtaskCompletion::Keep,
        Some("ask") => SubtaskCompletion::Ask,
        _ => SubtaskCompletion::Complete,
    }
}

/// Whether a click on a task's title edits it in place (`inline_edit`; off by default). Only
/// with a mouse or touchpad: a tap still opens the task.
pub fn inline_edit(c: &Client) -> bool {
    c.setting("inline_edit".into()).as_deref() == Some("true")
}

/// A switch that is on unless turned off.
fn on(c: &Client, key: &str) -> bool {
    c.setting(key.into()).as_deref() != Some("false")
}

/// Stars on tasks for their priority, and the Important page (`show_stars`).
pub fn stars(c: &Client) -> bool {
    on(c, "show_stars")
}

/// Pins on tasks and the Pinned group (`show_pins`). Off, pinned tasks stay pinned (X-PINNED)
/// and show among the others.
pub fn pins(c: &Client) -> bool {
    on(c, "show_pins")
}

/// What swiping a task does on a touchscreen, right (`swipe_right`) or left (`swipe_left`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeAction {
    /// Nothing: the task does not move that way. The default.
    Off,
    /// Completes it, or reopens a completed one.
    Complete,
    /// Stars it (high priority), or takes the star away.
    Star,
    /// Pins it, or unpins it (a top-level task, with pins shown).
    Pin,
    Delete,
}

/// Stored values of the swipe choices, in the order the front-ends offer them.
pub const SWIPES: [(&str, SwipeAction); 5] = [
    ("off", SwipeAction::Off),
    ("complete", SwipeAction::Complete),
    ("star", SwipeAction::Star),
    ("pin", SwipeAction::Pin),
    ("delete", SwipeAction::Delete),
];

/// What swiping a task to the right (`right`) or to the left does.
pub fn swipe(c: &Client, right: bool) -> SwipeAction {
    let key = if right { "swipe_right" } else { "swipe_left" };
    let v = c.setting(key.into()).unwrap_or_default();
    SWIPES
        .iter()
        .find(|(s, _)| *s == v)
        .map_or(SwipeAction::Off, |(_, a)| *a)
}

/// Whether the calendar's small month marks days with dots (`mini_dots`).
pub fn mini_dots(c: &Client) -> bool {
    on(c, "mini_dots")
}

/// Whether a double click on a calendar item edits it (`double_click_edit`). Off, a click shows
/// its details at once.
pub fn double_click_edit(c: &Client) -> bool {
    on(c, "double_click_edit")
}

/// The weekdays the Work Week view shows (`work_days`, comma-separated, 0 = Sunday): one to six
/// of them, in order; Monday to Friday by default.
pub fn work_days(c: &Client) -> Vec<i64> {
    let mut days: Vec<i64> = c
        .setting("work_days".into())
        .unwrap_or_default()
        .split(',')
        .filter_map(|d| d.trim().parse().ok())
        .filter(|d| (0..7).contains(d))
        .collect();
    days.sort();
    days.dedup();
    if (1..=6).contains(&days.len()) {
        days
    } else {
        vec![1, 2, 3, 4, 5]
    }
}

pub fn set_work_days(c: &Client, days: &[i64]) {
    let v: Vec<String> = days.iter().map(i64::to_string).collect();
    let _ = c.set_setting("work_days".into(), v.join(","));
}

/// Lengths offered for a new task's or event's time, in minutes.
pub const DURATIONS: [i64; 7] = [15, 30, 45, 60, 90, 120, 180];

/// How long, in seconds, a task's time slot lasts when one is planned (`task_duration`,
/// minutes; an hour by default).
pub fn task_duration(c: &Client) -> i64 {
    minutes(c, "task_duration")
}

/// How long, in seconds, a new event lasts (`event_duration`, minutes; an hour by default).
pub fn event_duration(c: &Client) -> i64 {
    minutes(c, "event_duration")
}

fn minutes(c: &Client, key: &str) -> i64 {
    c.setting(key.into())
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|m| *m > 0)
        .unwrap_or(60)
        * 60
}

/// When reminders of tasks without a time go off, in seconds into the day (`reminder_time`,
/// minutes; 9:00 by default).
pub fn all_day_time(c: &Client) -> i64 {
    c.setting("reminder_time".into())
        .and_then(|m| m.parse::<i64>().ok())
        .map_or(9 * 3600, |m| m * 60)
}

/// The offset a new reminder starts with: `reminder_default` for a due time ("At due time" by
/// default), the day itself at `all_day_time` for a date.
pub fn default_reminder(c: &Client, all_day: bool) -> i64 {
    if all_day {
        return all_day_time(c);
    }
    c.setting("reminder_default".into())
        .and_then(|o| o.parse().ok())
        .unwrap_or(0)
}

/// Collections the user hid on the calendar (`hidden_calendars`, one href per line).
pub fn hidden(c: &Client) -> HashSet<String> {
    let stored: Vec<String> = c
        .setting("hidden_calendars".into())
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();
    let mut all: HashSet<String> = stored.iter().cloned().collect();
    // Todav 1.2 kept server paths: the lists at those paths.
    if stored.iter().any(|s| s.starts_with('/')) {
        for l in c.lists().into_iter().chain(c.calendars()) {
            if stored.iter().any(|s| todav_core::same_list(s, &l.href)) {
                all.insert(l.href);
            }
        }
    }
    all
}

/// Hide or show the collections `hrefs` on the calendar.
pub fn set_hidden<'a>(c: &Client, hrefs: impl IntoIterator<Item = &'a str>, hide: bool) {
    let mut h = hidden(c);
    for href in hrefs {
        if hide {
            h.insert(href.to_string());
        } else {
            h.retain(|s| !todav_core::same_list(s, href));
        }
    }
    let mut v: Vec<String> = h.into_iter().collect();
    v.sort();
    let _ = c.set_setting("hidden_calendars".into(), v.join("\n"));
}
