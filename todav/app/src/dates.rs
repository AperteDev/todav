//! Dates as the app shows them. Task and event times are local wall-clock seconds (see
//! `Client::local_now`): their fields are read as if they were UTC.

use crate::DAY;
use todav_core::ical::{civil_from_days, days_from_civil};
use todav_core::{Client, Task};

/// Days from the day of `now` to the day of `t` (negative: past).
pub fn days(t: i64, now: i64) -> i64 {
    t.div_euclid(DAY) - now.div_euclid(DAY)
}

/// Midnight of the day holding `t`.
pub fn midnight(t: i64) -> i64 {
    t - t.rem_euclid(DAY)
}

/// (year, month, day of the month) of `t`.
pub fn date(t: i64) -> (i64, i64, i64) {
    civil_from_days(t.div_euclid(DAY))
}

/// Midnight of a date; months past 12 or below 1 go on into the next or previous years.
pub fn on(year: i64, month: i64, day: i64) -> i64 {
    let (y, m) = (
        year + (month - 1).div_euclid(12),
        (month - 1).rem_euclid(12) + 1,
    );
    days_from_civil(y, m, day) * DAY
}

/// Midnight of the first day of the month holding `t`.
pub fn month_first(t: i64) -> i64 {
    let (y, m, _) = date(t);
    on(y, m, 1)
}

/// Midnight of the first day of the week holding `t`, weeks starting on `first` (0 = Sunday).
pub fn week_start(t: i64, first: i64) -> i64 {
    // 1970-01-01, day 0, was a Thursday (4).
    let day = midnight(t);
    day - (day.div_euclid(DAY) + 4 - first).rem_euclid(7) * DAY
}

/// The weekday of `t`, 0 = Sunday.
pub fn weekday(t: i64) -> i64 {
    (t.div_euclid(DAY) + 4).rem_euclid(7)
}

/// The days of the week holding `t` (weeks starting on `first`) that are among `weekdays`
/// (0 = Sunday), in order: the Work Week view's days.
pub fn days_of_week(t: i64, first: i64, weekdays: &[i64]) -> Vec<i64> {
    let start = week_start(t, first);
    (0..7)
        .map(|d| start + d * DAY)
        .filter(|d| weekdays.contains(&weekday(*d)))
        .collect()
}

/// How a day is named, seen from the day of `now`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Day {
    Today,
    Tomorrow,
    Yesterday,
    /// In the week ahead: its weekday.
    Weekday,
    /// Its date, in the current year.
    ThisYear,
    /// Its date with the year.
    Date,
}

pub fn day(t: i64, now: i64) -> Day {
    match days(t, now) {
        0 => Day::Today,
        1 => Day::Tomorrow,
        -1 => Day::Yesterday,
        2..=6 => Day::Weekday,
        _ if date(t).0 == date(now).0 => Day::ThisYear,
        _ => Day::Date,
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

/// Sections of the Upcoming page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Overdue,
    Today,
    Tomorrow,
    /// A day in the week ahead (its midnight), by its weekday.
    Weekday(i64),
    Later,
}

/// The Upcoming section of an open task with a due date.
pub fn section(t: &Task, now: i64) -> Section {
    if overdue(t, now) {
        return Section::Overdue;
    }
    let d = t.due.unwrap_or(now);
    match days(d, now) {
        0 => Section::Today,
        1 => Section::Tomorrow,
        2..=6 => Section::Weekday(midnight(d)),
        _ => Section::Later,
    }
}

/// Reminder choices for a due date with a time, in seconds after it: at due time, 5, 15 and 30
/// minutes, an hour and a day before.
pub const TIMED: [i64; 6] = [0, -300, -900, -1800, -3600, -DAY];

/// Reminder choices for a due date with a time or (`all_day`) without one, in seconds after
/// the due time: `TIMED`, or on the day and the day before at `prefs::all_day_time` (a date
/// alone counts from its midnight).
pub fn presets(c: &Client, all_day: bool) -> Vec<i64> {
    if !all_day {
        return TIMED.to_vec();
    }
    let t = crate::prefs::all_day_time(c);
    vec![t, t - DAY]
}

/// Units of a custom reminder: minutes, hours, days, weeks.
pub const UNITS: [i64; 4] = [60, 3600, DAY, 7 * DAY];

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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> i64 {
        todav_core::ical::parse_time(s).unwrap()
    }

    #[test]
    fn a_work_week_is_its_weekdays_in_order() {
        let wed = at("20261014T120000");
        assert_eq!(weekday(wed), 3);
        let days = |first, weekdays: &[i64]| days_of_week(wed, first, weekdays);
        let (mon, fri, sun) = (at("20261012"), at("20261016"), at("20261011"));
        assert_eq!(days(1, &[1, 2, 3, 4, 5]).first(), Some(&mon));
        assert_eq!(days(1, &[1, 2, 3, 4, 5]).last(), Some(&fri));
        // Sunday ends a week starting on Monday, and starts one starting on Sunday.
        assert_eq!(days(1, &[0, 5]), vec![fri, at("20261018")]);
        assert_eq!(days(0, &[0, 5]), vec![sun, fri]);
    }

    #[test]
    fn days_are_named_by_calendar_days() {
        let now = at("20261014T233000"); // a Wednesday
        assert_eq!(day(at("20261015T000500"), now), Day::Tomorrow);
        assert_eq!(day(at("20261014"), now), Day::Today);
        assert_eq!(day(at("20261013"), now), Day::Yesterday);
        assert_eq!(day(at("20261017"), now), Day::Weekday);
        assert_eq!(day(at("20261031"), now), Day::ThisYear);
        assert_eq!(day(at("20270102"), now), Day::Date);
        let task = |due, all_day| Task {
            due: Some(at(due)),
            due_all_day: all_day,
            ..crate::tasks::tests::task("t", None)
        };
        assert!(
            !overdue(&task("20261014", true), now),
            "a date is due all day"
        );
        assert!(overdue(&task("20261014T120000", false), now));
        assert_eq!(
            section(&task("20261016T080000", false), now),
            Section::Weekday(at("20261016"))
        );
        assert_eq!(section(&task("20261030", true), now), Section::Later);
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

    #[test]
    fn weeks_start_on_the_locales_day() {
        // Monday first.
        assert_eq!(week_start(at("20261021"), 1), at("20261019")); // a Wednesday
        assert_eq!(week_start(at("20261019"), 1), at("20261019"));
        assert_eq!(week_start(at("20261025T120000"), 1), at("20261019")); // Sunday
        // Sunday first.
        assert_eq!(week_start(at("20261021"), 0), at("20261018"));
        assert_eq!(week_start(at("20261025"), 0), at("20261025"));
        assert_eq!(week_start(at("20261024"), 0), at("20261018")); // Saturday
    }

    #[test]
    fn months_go_on_into_other_years() {
        assert_eq!(month_first(at("20261014T120000")), at("20261001"));
        assert_eq!(on(2026, 13, 1), at("20270101"));
        assert_eq!(on(2026, 0, 1), at("20251201"));
    }
}
