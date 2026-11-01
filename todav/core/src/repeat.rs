//! Repeating tasks: a VTODO with an RRULE, counted from its DTSTART or, without one, its DUE (as
//! task apps write them). Done, it moves on in place to its next occurrence: its dates (and
//! reminders set at a time of their own) shift, it stays open, and other clients see one task
//! with new dates. At the end of its rule it is completed as any task.
//!
//! The rule is followed in local wall-clock time, so a task due at 9:00 stays at 9:00 across a
//! change to or from summer time, though Todav writes times in UTC.

use crate::event::Repeat;
use crate::ical::{self, Calendar, parse_time};
use crate::rrule::{Freq, Rule};
use crate::tz::{self, Stamp};
use crate::{Result, store};
use rusqlite::Connection;

const DAY: i64 = 86400;

/// What a task's occurrences count from: DTSTART, else DUE.
pub(crate) fn anchor(cal: &Calendar) -> Option<Stamp> {
    cal.prop("DTSTART")
        .or_else(|| cal.prop("DUE"))
        .and_then(tz::stamp)
}

/// An UNTIL for one last day (local), in each form an anchor may need.
struct Until {
    date: String,
    floating: String,
    utc: String,
}

impl Until {
    fn of_day(db: &Connection, day: i64) -> Result<Until> {
        let last = day + DAY - 1;
        Ok(Until {
            date: ical::fmt_utc(day)[..8].to_string(),
            floating: ical::fmt_utc(last)[..15].to_string(),
            utc: ical::fmt_utc(store::to_utc(db, last)?),
        })
    }

    /// The last day of an UNTIL value, in local time.
    fn of_value(db: &Connection, v: &str) -> Result<Option<Until>> {
        let Some(t) = parse_time(v) else {
            return Ok(None);
        };
        let local = if v.ends_with('Z') {
            store::to_local(db, t)?
        } else {
            t
        };
        Until::of_day(db, local - local.rem_euclid(DAY)).map(Some)
    }

    /// RFC 5545: UNTIL is a date for a date, floating for a floating time, else UTC.
    fn for_anchor(&self, a: &Stamp) -> &str {
        match a {
            Stamp::Date(_) => &self.date,
            Stamp::Floating(_) => &self.floating,
            _ => &self.utc,
        }
    }
}

/// UNTIL as a patch sets it (`new`) and as the task's rule has it (`old`), worked out before
/// the task is changed (it takes the database).
pub(crate) struct Untils {
    new: Option<Until>,
    old: Option<Until>,
}

impl Untils {
    pub(crate) fn new(
        db: &Connection,
        repeat: Option<&Option<Repeat>>,
        rule: Option<&str>,
    ) -> Result<Untils> {
        let new = match repeat {
            Some(Some(r)) => r.until.map(|d| Until::of_day(db, d)).transpose()?,
            _ => None,
        };
        let old = match rule.and_then(Rule::parse).and_then(|r| r.until) {
            Some(v) => Until::of_value(db, &v)?,
            None => None,
        };
        Ok(Untils { new, old })
    }
}

/// `rule` with UNTIL `until` in place of its own UNTIL and COUNT (none: without them).
fn with_until(rule: &str, until: Option<&str>) -> String {
    let mut parts: Vec<String> = rule
        .split(';')
        .filter(|p| !p.is_empty())
        .filter(|p| {
            let k = p.split('=').next().unwrap_or_default().to_ascii_uppercase();
            until.is_none() || (k != "UNTIL" && k != "COUNT")
        })
        .map(String::from)
        .collect();
    if let Some(u) = until {
        parts.push(format!("UNTIL={u}"));
    }
    parts.join(";")
}

/// After an edit: the RRULE `repeat` asks for (Some(None): none), or else the task's own, its
/// UNTIL rewritten when its dates changed between dates and times (`dated_before`: whether
/// they were dates); repeating from when it is done as `from_done` says. A task without dates
/// does not repeat.
pub(crate) fn apply(
    cal: &mut Calendar,
    repeat: Option<&Option<Repeat>>,
    from_done: Option<bool>,
    dated_before: Option<bool>,
    until: &Untils,
) {
    if let Some(f) = from_done {
        cal.set(FROM, "", f.then_some("COMPLETION"));
    }
    set_rule(cal, repeat, dated_before, until);
    if cal.prop("RRULE").is_none() {
        cal.set(FROM, "", None);
    }
}

fn set_rule(
    cal: &mut Calendar,
    repeat: Option<&Option<Repeat>>,
    dated_before: Option<bool>,
    until: &Untils,
) {
    let Some(a) = anchor(cal) else {
        cal.set("RRULE", "", None);
        return;
    };
    let rule = match repeat {
        Some(r) => r.as_ref().map(|r| {
            let freq = match r.freq {
                Freq::Daily => "DAILY",
                Freq::Weekly => "WEEKLY",
                Freq::Monthly => "MONTHLY",
                Freq::Yearly => "YEARLY",
            };
            let mut v = format!("FREQ={freq}");
            if r.interval > 1 {
                v += &format!(";INTERVAL={}", r.interval);
            }
            match &until.new {
                Some(u) => with_until(&v, Some(u.for_anchor(&a))),
                None => v,
            }
        }),
        None => {
            let Some(old) = cal.prop("RRULE").map(|l| l.value.clone()) else {
                return;
            };
            let dated = matches!(a, Stamp::Date(_));
            match &until.old {
                Some(u) if dated_before.is_some_and(|d| d != dated) => {
                    Some(with_until(&old, Some(u.for_anchor(&a))))
                }
                _ => return,
            }
        }
    };
    cal.set("RRULE", "", rule.as_deref());
}

/// A stamp moved by `days` whole days of local wall-clock time.
fn shifted(db: &Connection, s: &Stamp, days: i64) -> Result<Stamp> {
    let d = days * DAY;
    Ok(match s {
        Stamp::Date(t) => Stamp::Date(t + d),
        Stamp::Floating(t) => Stamp::Floating(t + d),
        Stamp::Zoned(t, z) => Stamp::Zoned(t + d, z.clone()),
        Stamp::Utc(t) => Stamp::Utc(store::to_utc(db, store::to_local(db, *t)? + d)?),
    })
}

fn render(s: &Stamp) -> (String, String) {
    match s {
        Stamp::Date(t) => (";VALUE=DATE".into(), ical::fmt_utc(*t)[..8].to_string()),
        Stamp::Floating(t) => (String::new(), ical::fmt_utc(*t)[..15].to_string()),
        Stamp::Utc(t) => (String::new(), ical::fmt_utc(*t)),
        Stamp::Zoned(t, z) => (format!(";TZID={z}"), ical::fmt_utc(*t)[..15].to_string()),
    }
}

/// Move task `cal`, done at local time `now`, on to its next occurrence: the first after the
/// one it is at that is not over by `now` (a missed one is skipped), or, repeating from when it
/// was done (`from_done`), one period after `now`'s day. False when it has none (its rule
/// ended, or it has no rule this can follow): then it is completed as any task.
pub(crate) fn advance(
    db: &Connection,
    cal: &mut Calendar,
    now: i64,
    from_done: bool,
) -> Result<bool> {
    let Some(rule) = cal.prop("RRULE").and_then(|l| Rule::parse(&l.value)) else {
        return Ok(false);
    };
    let Some(a) = anchor(cal) else {
        return Ok(false);
    };
    if !rule.supported {
        return Ok(false);
    }
    let at = store::local_stamp(db, cal, &a)?;
    let tod = at.rem_euclid(DAY);
    let today = now - now.rem_euclid(DAY);
    let until = match rule.until.as_deref() {
        Some(u) if u.len() == 8 => parse_time(u).map(|v| v + DAY - 1),
        Some(u) if u.ends_with('Z') => parse_time(u).map(|v| store::to_local(db, v)).transpose()?,
        Some(u) => parse_time(u),
        None => None,
    };
    // Over: a date before today, a time already past.
    let over = |t: i64| match a {
        Stamp::Date(_) => t < today,
        _ => t <= now,
    };
    let exdates: Vec<i64> = cal
        .main()
        .props("EXDATE")
        .iter()
        .flat_map(|l| tz::stamps(l))
        .filter_map(|s| store::local_stamp(db, cal, &s).ok())
        .collect();
    // From when it was done: a series starting on that day, at the task's time.
    let start = if from_done { today + tod } else { at };
    let mut next = None;
    let mut span = 64 * DAY * rule.interval.max(1);
    let mut from = start + 1;
    while next.is_none() && from - start < 200 * 366 * DAY {
        let found = rule.expand(start, from, from + span, until);
        if found.is_empty() && until.is_some_and(|u| from > u) {
            break;
        }
        next = found
            .into_iter()
            .find(|t| !exdates.contains(t) && (from_done || !over(*t)));
        from += span;
        span *= 2;
    }
    let Some(next) = next else {
        return Ok(false);
    };
    let days = (next - next.rem_euclid(DAY) - (at - tod)) / DAY;
    // COUNT counts from DTSTART, which moves: what is left of it, from the new one on.
    if let Some(count) = rule.count {
        let passed = rule.expand(start, start + 1, next, until).len() as u32 + 1;
        let left = count.saturating_sub(if from_done { 1 } else { passed });
        if left == 0 {
            return Ok(false);
        }
        let v = cal
            .prop("RRULE")
            .map(|l| l.value.clone())
            .unwrap_or_default();
        let parts: Vec<String> = v
            .split(';')
            .filter(|p| !p.is_empty())
            .map(|p| match p.split_once('=') {
                Some((k, _)) if k.eq_ignore_ascii_case("COUNT") => format!("COUNT={left}"),
                _ => p.to_string(),
            })
            .collect();
        cal.set("RRULE", "", Some(&parts.join(";")));
    }
    for name in ["DTSTART", "DUE"] {
        if let Some(s) = cal.prop(name).and_then(tz::stamp) {
            let (params, value) = render(&shifted(db, &s, days)?);
            cal.set(name, &params, Some(&value));
        }
    }
    // Reminders at a time of their own move with it; relative ones follow their date.
    let triggers: Vec<(String, String)> = cal
        .alarm_triggers()
        .into_iter()
        .map(|t| (t.params.clone(), t.value.clone()))
        .collect();
    if triggers
        .iter()
        .any(|(_, v)| ical::parse_duration(v).is_none())
    {
        let mut moved = Vec::new();
        for (p, v) in triggers {
            match parse_time(&v).filter(|_| v.ends_with('Z')) {
                Some(t) => moved.push((p, render(&shifted(db, &Stamp::Utc(t), days)?).1)),
                None => moved.push((p, v)),
            }
        }
        let t: Vec<(&str, &str)> = moved
            .iter()
            .map(|(p, v)| (p.as_str(), v.as_str()))
            .collect();
        cal.set_alarms(&t);
    }
    Ok(true)
}

/// Marks a task that repeats from when it was done rather than from its dates. No standard
/// says how, and other clients keep this one only for us: theirs, if any, stay on the device.
pub const FROM: &str = "X-TODAV-REPEAT-FROM";

/// Whether a task repeats from when it was done rather than from its dates (`FROM`).
pub(crate) fn from_done(cal: &Calendar) -> bool {
    cal.text(FROM)
        .is_some_and(|v| v.eq_ignore_ascii_case("COMPLETION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(props: &str) -> Calendar {
        let props = props.replace('\n', "\r\n");
        Calendar::parse(&format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VTODO\r\nUID:u\r\nSUMMARY:s\r\n{props}\r\nEND:VTODO\r\nEND:VCALENDAR\r\n"
        ))
        .unwrap()
    }

    fn t(s: &str) -> i64 {
        parse_time(s).unwrap()
    }

    fn value(cal: &Calendar, name: &str) -> String {
        cal.prop(name).map(|l| l.value.clone()).unwrap_or_default()
    }

    #[test]
    fn done_it_moves_to_the_next_occurrence_not_yet_over() {
        let db = store::open(":memory:").unwrap();
        // Weekly, due Monday, done on Wednesday: the next Monday.
        let mut c = todo("DUE;VALUE=DATE:20261102\nRRULE:FREQ=WEEKLY");
        assert!(advance(&db, &mut c, t("20261104T120000"), false).unwrap());
        assert_eq!(value(&c, "DUE"), "20261109");
        assert_eq!(value(&c, "RRULE"), "FREQ=WEEKLY");
        // Daily, four days missed: today's, which is not over yet.
        let mut c = todo("DUE;VALUE=DATE:20261101\nRRULE:FREQ=DAILY");
        assert!(advance(&db, &mut c, t("20261105T120000"), false).unwrap());
        assert_eq!(value(&c, "DUE"), "20261105");
        // At 9:00 every day, done at 10:00: tomorrow's; the start moves with the due date.
        let mut c =
            todo("DTSTART:20261110T090000\nDUE:20261110T093000\nRRULE:FREQ=DAILY;INTERVAL=2");
        assert!(advance(&db, &mut c, t("20261110T100000"), false).unwrap());
        assert_eq!(value(&c, "DTSTART"), "20261112T090000");
        assert_eq!(value(&c, "DUE"), "20261112T093000");
    }

    #[test]
    fn its_rule_ends_by_count_or_until() {
        let db = store::open(":memory:").unwrap();
        let mut c = todo("DUE;VALUE=DATE:20261101\nRRULE:FREQ=DAILY;COUNT=3");
        let now = t("20261101T080000");
        assert!(advance(&db, &mut c, now, false).unwrap());
        assert_eq!(
            (value(&c, "DUE"), value(&c, "RRULE")),
            ("20261102".into(), "FREQ=DAILY;COUNT=2".into())
        );
        assert!(advance(&db, &mut c, now, false).unwrap());
        assert_eq!(value(&c, "RRULE"), "FREQ=DAILY;COUNT=1");
        assert!(!advance(&db, &mut c, now, false).unwrap(), "the last one");
        // Skipped occurrences count too.
        let mut c = todo("DUE;VALUE=DATE:20261101\nRRULE:FREQ=DAILY;COUNT=5");
        assert!(advance(&db, &mut c, t("20261103T080000"), false).unwrap());
        assert_eq!(
            (value(&c, "DUE"), value(&c, "RRULE")),
            ("20261103".into(), "FREQ=DAILY;COUNT=3".into())
        );
        let mut c = todo("DUE;VALUE=DATE:20261102\nRRULE:FREQ=DAILY;UNTIL=20261102");
        assert!(!advance(&db, &mut c, now, false).unwrap());
        assert_eq!(value(&c, "DUE"), "20261102", "left as it was");
        // A rule this cannot follow ends it too.
        let mut c = todo("DUE;VALUE=DATE:20261102\nRRULE:FREQ=HOURLY");
        assert!(!advance(&db, &mut c, now, false).unwrap());
    }

    #[test]
    fn from_when_it_was_done_it_counts_from_that_day() {
        let db = store::open(":memory:").unwrap();
        let mut c = todo("DUE:20261101T180000\nRRULE:FREQ=MONTHLY");
        assert!(advance(&db, &mut c, t("20261120T070000"), true).unwrap());
        assert_eq!(value(&c, "DUE"), "20261220T180000");
    }

    #[test]
    fn utc_times_and_reminders_at_a_time_keep_their_local_hour() {
        let db = store::open(":memory:").unwrap();
        let local = |v: &str| store::to_local(&db, t(v)).unwrap();
        let mut c = todo(
            "DUE:20261110T080000Z\nRRULE:FREQ=WEEKLY\nBEGIN:VALARM\nACTION:DISPLAY\nTRIGGER;VALUE=DATE-TIME:20261110T070000Z\nEND:VALARM\nBEGIN:VALARM\nACTION:DISPLAY\nTRIGGER;RELATED=END:-PT15M\nEND:VALARM",
        );
        assert!(advance(&db, &mut c, local("20261110T090000Z"), false).unwrap());
        let due = value(&c, "DUE");
        assert_eq!(local(&due), local("20261110T080000Z") + 7 * DAY);
        let triggers: Vec<String> = c.alarm_triggers().iter().map(|l| l.value.clone()).collect();
        assert_eq!(triggers.len(), 2);
        assert_eq!(local(&triggers[0]), local("20261110T070000Z") + 7 * DAY);
        assert_eq!(triggers[1], "-PT15M");
    }

    #[test]
    fn a_repeat_set_writes_until_as_its_dates_are() {
        let db = store::open(":memory:").unwrap();
        let repeat = |until| {
            Some(Repeat {
                freq: Freq::Weekly,
                interval: 2,
                until,
            })
        };
        let r = repeat(Some(t("20261231")));
        let u = Untils::new(&db, Some(&r), None).unwrap();
        let mut c = todo("DUE;VALUE=DATE:20261102");
        apply(&mut c, Some(&r), None, Some(true), &u);
        assert_eq!(value(&c, "RRULE"), "FREQ=WEEKLY;INTERVAL=2;UNTIL=20261231");
        let mut c = todo("DUE:20261102T090000");
        apply(&mut c, Some(&r), Some(true), Some(false), &u);
        assert_eq!(value(&c, FROM), "COMPLETION");
        assert_eq!(
            value(&c, "RRULE"),
            "FREQ=WEEKLY;INTERVAL=2;UNTIL=20261231T235959"
        );
        // Its dates got times: so does its UNTIL; other parts stay.
        let mut c = todo("DUE:20261102T090000\nRRULE:FREQ=WEEKLY;BYDAY=MO;UNTIL=20261231");
        let u = Untils::new(&db, None, Some("FREQ=WEEKLY;BYDAY=MO;UNTIL=20261231")).unwrap();
        apply(&mut c, None, None, Some(true), &u);
        assert_eq!(
            value(&c, "RRULE"),
            "FREQ=WEEKLY;BYDAY=MO;UNTIL=20261231T235959"
        );
        // Without dates it does not repeat.
        let mut c = todo("RRULE:FREQ=DAILY\nX-TODAV-REPEAT-FROM:COMPLETION");
        apply(&mut c, None, None, None, &u);
        assert!(c.prop("RRULE").is_none() && c.prop(FROM).is_none());
    }
}
