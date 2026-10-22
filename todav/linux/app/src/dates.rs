//! Due dates as the app shows them. Task times are local wall-clock seconds (see
//! `Client::local_now`), so their fields are read as if they were UTC.

use crate::i18n::{gettext, gettext_noop};
use gtk::glib;
use gtk::prelude::*;
use todav_core::Task;

const DAY: i64 = 86400;

/// Reminder choices for a due date with a time: label (translate it to show it), seconds after
/// the due time.
pub const TIMED: [(&str, i64); 6] = [
    (gettext_noop("At due time"), 0),
    (gettext_noop("5 minutes before"), -300),
    (gettext_noop("15 minutes before"), -900),
    (gettext_noop("30 minutes before"), -1800),
    (gettext_noop("1 hour before"), -3600),
    (gettext_noop("1 day before"), -DAY),
];

/// When reminders of tasks without a time go off, in seconds into the day: the synced
/// `reminder_time` setting (minutes), 9:00 by default.
pub fn all_day_time() -> i64 {
    crate::core()
        .setting("reminder_time".into())
        .and_then(|m| m.parse::<i64>().ok())
        .map_or(9 * 3600, |m| m * 60)
}

/// The offset a new reminder starts with: the synced `reminder_default` setting for a due time
/// ("At due time" by default), the day itself at `all_day_time` for a date.
pub fn default_reminder(all_day: bool) -> i64 {
    if all_day {
        return all_day_time();
    }
    crate::core()
        .setting("reminder_default".into())
        .and_then(|o| o.parse().ok())
        .unwrap_or(0)
}

/// Reminder choices for a due date with a time or (`all_day`) without: label, seconds after the
/// due time. A date alone counts from its midnight.
pub fn reminders(all_day: bool) -> Vec<(String, i64)> {
    if !all_day {
        return TIMED.iter().map(|(l, o)| (gettext(l), *o)).collect();
    }
    let t = all_day_time();
    let at = format!("{}:{:02}", t / 3600, t / 60 % 60);
    vec![
        (gettext("On the day at {time}").replace("{time}", &at), t),
        (
            gettext("The day before at {time}").replace("{time}", &at),
            t - DAY,
        ),
    ]
}

/// Units of a custom reminder, in the order of its unit picker.
const UNITS: [i64; 4] = [60, 3600, DAY, 7 * DAY];

/// A reminder offset as (amount, index in `UNITS`, before): the largest unit it fits exactly.
pub fn span(offset: i64) -> (i64, u32, bool) {
    let n = offset.abs();
    let unit = (0..UNITS.len())
        .rev()
        .find(|&u| n % UNITS[u] == 0 && n > 0)
        .unwrap_or(0);
    (n / UNITS[unit], unit as u32, offset < 0)
}

/// Inverse of `span`.
pub fn offset(amount: i64, unit: u32, before: bool) -> i64 {
    let o = amount * UNITS[unit as usize];
    if before { -o } else { o }
}

pub fn fields(t: i64) -> glib::DateTime {
    glib::DateTime::from_unix_utc(t).unwrap_or_else(|_| glib::DateTime::now_utc().unwrap())
}

/// Midnight of a calendar date.
pub fn midnight(d: &glib::DateTime) -> i64 {
    let (y, m, day) = (d.year(), d.month(), d.day_of_month());
    todav_core::ical::parse_time(&format!("{y:04}{m:02}{day:02}")).unwrap_or(0)
}

fn fmt(d: &glib::DateTime, f: &str) -> String {
    d.format(f).map(String::from).unwrap_or_default()
}

/// Days from today to `t` (negative: past).
fn days(t: i64, now: i64) -> i64 {
    t.div_euclid(DAY) - now.div_euclid(DAY)
}

/// "Today", "Tomorrow", "Yesterday", a weekday in the week ahead, else the date; then the time.
pub fn label(t: i64, all_day: bool, now: i64) -> String {
    let d = fields(t);
    let day = match days(t, now) {
        0 => gettext("Today"),
        1 => gettext("Tomorrow"),
        -1 => gettext("Yesterday"),
        2..=6 => fmt(&d, "%A"),
        _ if d.year() == fields(now).year() => fmt(&d, "%-d %B"),
        _ => fmt(&d, "%-d %B %Y"),
    };
    if all_day {
        day
    } else {
        format!("{day} {}", fmt(&d, "%H:%M"))
    }
}

pub fn overdue(t: &Task, now: i64) -> bool {
    match t.due {
        Some(d) if t.due_all_day => days(d, now) < 0,
        Some(d) => d < now,
        None => false,
    }
}

/// Due today or earlier: what the Upcoming count shows.
pub fn pressing(t: &Task, now: i64) -> bool {
    t.due.is_some_and(|d| days(d, now) <= 0)
}

/// Upcoming page section of an open task with a due date.
pub fn section(t: &Task, now: i64) -> String {
    let d = t.due.unwrap_or(now);
    if overdue(t, now) {
        return gettext("Overdue");
    }
    match days(d, now) {
        0 => gettext("Today"),
        1 => gettext("Tomorrow"),
        2..=6 => fmt(&fields(d), "%A"),
        _ => gettext("Later"),
    }
}

/// Due date pill for a task row: red when overdue, with a bell when a reminder is set.
pub fn tag(t: &Task, now: i64) -> Option<gtk::Box> {
    let at = t.due?;
    let b = gtk::Box::builder()
        .spacing(4)
        .valign(gtk::Align::Center)
        .build();
    b.add_css_class("tag");
    if !t.reminders.is_empty() {
        b.append(&gtk::Image::from_icon_name("alarm-symbolic"));
    }
    b.append(&gtk::Label::new(Some(&label(at, t.due_all_day, now))));
    if overdue(t, now) {
        b.add_css_class("overdue");
    } else if days(at, now) != 0 {
        b.add_css_class("date");
    }
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_count_calendar_days() {
        let now = todav_core::ical::parse_time("20261014T233000").unwrap(); // a Wednesday
        let at = |s| todav_core::ical::parse_time(s).unwrap();
        assert_eq!(label(at("20261015T000500"), false, now), "Tomorrow 00:05");
        assert_eq!(label(at("20261014"), true, now), "Today");
        assert_eq!(label(at("20261013"), true, now), "Yesterday");
        assert_eq!(label(at("20261017"), true, now), "Saturday");
        assert_eq!(label(at("20261031"), true, now), "31 October");
        assert_eq!(label(at("20270102"), true, now), "2 January 2027");
        let task = |due, all_day| Task {
            due: Some(at(due)),
            due_all_day: all_day,
            ..crate::window::tests::task("t", None)
        };
        assert!(
            !overdue(&task("20261014", true), now),
            "a date is due all day"
        );
        assert!(overdue(&task("20261014T120000", false), now));
        assert_eq!(section(&task("20261016", true), now), "Friday");
        assert_eq!(section(&task("20261030", true), now), "Later");
    }

    #[test]
    fn spans_use_the_largest_exact_unit() {
        assert_eq!(span(-900), (15, 0, true));
        assert_eq!(span(9 * 3600), (9, 1, false));
        assert_eq!(span(-2 * DAY), (2, 2, true));
        assert_eq!(span(-14 * DAY), (2, 3, true));
        assert_eq!(span(0), (0, 0, false));
        assert_eq!(span(90), (1, 0, false), "whole minutes");
        for o in [-900, 32400, -54000, 0, -604800] {
            let (a, u, b) = span(o);
            assert_eq!(offset(a, u, b), o);
        }
    }
}
