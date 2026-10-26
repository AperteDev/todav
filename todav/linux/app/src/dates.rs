//! Due dates as the app shows them. Task times are local wall-clock seconds (see
//! `Client::local_now`), so their fields are read as if they were UTC.

use crate::core;
use crate::i18n::{gettext, gettext_noop};
use gtk::glib;
use gtk::prelude::*;
use todav_app::dates::{Day, Section};
pub use todav_app::dates::{offset, overdue, span};
use todav_core::Task;

/// Labels of `todav_app::dates::TIMED`, in its order (translate them to show them).
pub const TIMED: [&str; 6] = [
    gettext_noop("At due time"),
    gettext_noop("5 minutes before"),
    gettext_noop("15 minutes before"),
    gettext_noop("30 minutes before"),
    gettext_noop("1 hour before"),
    gettext_noop("1 day before"),
];

/// Reminder choices for a due date with a time or (`all_day`) without: label, seconds after the
/// due time (`todav_app::dates::presets`).
pub fn reminders(all_day: bool) -> Vec<(String, i64)> {
    let offsets = todav_app::dates::presets(core(), all_day);
    if !all_day {
        return TIMED.iter().map(|l| gettext(l)).zip(offsets).collect();
    }
    let t = todav_app::prefs::all_day_time(core());
    let at = format!("{}:{:02}", t / 3600, t / 60 % 60);
    let labels = [
        gettext("On the day at {time}").replace("{time}", &at),
        gettext("The day before at {time}").replace("{time}", &at),
    ];
    labels.into_iter().zip(offsets).collect()
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

/// "Today", "Tomorrow", "Yesterday", a weekday in the week ahead, else the date; then the time.
pub fn label(t: i64, all_day: bool, now: i64) -> String {
    let d = fields(t);
    let day = match todav_app::dates::day(t, now) {
        Day::Today => gettext("Today"),
        Day::Tomorrow => gettext("Tomorrow"),
        Day::Yesterday => gettext("Yesterday"),
        Day::Weekday => fmt(&d, "%A"),
        Day::ThisYear => fmt(&d, "%-d %B"),
        Day::Date => fmt(&d, "%-d %B %Y"),
    };
    if all_day {
        day
    } else {
        format!("{day} {}", fmt(&d, "%H:%M"))
    }
}

/// Upcoming page section of an open task with a due date.
pub fn section(t: &Task, now: i64) -> String {
    match todav_app::dates::section(t, now) {
        Section::Overdue => gettext("Overdue"),
        Section::Today => gettext("Today"),
        Section::Tomorrow => gettext("Tomorrow"),
        Section::Weekday(d) => fmt(&fields(d), "%A"),
        Section::Later => gettext("Later"),
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
    } else if todav_app::dates::days(at, now) != 0 {
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
        assert_eq!(section(&task("20261016", true), now), "Friday");
        assert_eq!(section(&task("20261030", true), now), "Later");
    }
}
