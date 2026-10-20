//! Events: expanding a stored VEVENT object (a single event, or a recurring series with its
//! RDATE / EXDATE / overridden occurrences) into occurrences in local time.

use crate::ical::{self, Calendar, Comp, parse_time};
use crate::rrule::Rule;
use crate::tz::{self, Stamp};
use crate::{Result, store};
use rusqlite::Connection;

/// One occurrence of an event. Times are local wall-clock time (see `store::to_local`).
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub uid: String,
    pub list_href: String,
    /// Start of this occurrence as the series planned it (an override may have moved it); None
    /// when the event does not repeat. Identifies the occurrence for edits.
    pub recurrence_id: Option<i64>,
    pub summary: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start: i64,
    /// Exclusive: an all-day event ends at the midnight after its last day.
    pub end: i64,
    pub all_day: bool,
    /// Part of a recurring series.
    pub recurring: bool,
    /// STATUS:CANCELLED (a single occurrence, or the whole event).
    pub cancelled: bool,
    /// When its reminders go off, soonest first.
    pub reminders: Vec<i64>,
}

/// Wall-clock length of a component: DTEND − DTSTART, DURATION, or else a day for a date and
/// nothing for a time.
fn duration(c: &Comp) -> i64 {
    let Some(start) = c.prop("DTSTART").and_then(tz::stamp) else {
        return 0;
    };
    if let Some(end) = c.prop("DTEND").and_then(tz::stamp) {
        return (end.wall() - start.wall()).max(0);
    }
    if let Some(d) = c
        .prop("DURATION")
        .and_then(|l| ical::parse_duration(&l.value))
    {
        return d.max(0);
    }
    if matches!(start, Stamp::Date(_)) {
        86400
    } else {
        0
    }
}

/// A stamp with another wall-clock reading in the same zone.
fn at(s: &Stamp, wall: i64) -> Stamp {
    match s {
        Stamp::Date(_) => Stamp::Date(wall),
        Stamp::Floating(_) => Stamp::Floating(wall),
        Stamp::Utc(_) => Stamp::Utc(wall),
        Stamp::Zoned(_, z) => Stamp::Zoned(wall, z.clone()),
    }
}

/// The occurrence of component `c` starting at `start`: start and end in local time, and when
/// its reminders go off (relative to its start, or its end with RELATED=END).
fn occurrence(
    db: &Connection,
    cal: &Calendar,
    c: &Comp,
    start: &Stamp,
) -> Result<(i64, i64, Vec<i64>)> {
    let dur = duration(c);
    let s = store::local_stamp(db, cal, start)?;
    let e = store::local_stamp(db, cal, &at(start, start.wall() + dur))?;
    let mut reminders = Vec::new();
    for t in c.alarm_triggers() {
        let end = t
            .param("RELATED")
            .is_some_and(|r| r.eq_ignore_ascii_case("END"));
        match ical::parse_duration(&t.value) {
            Some(d) => reminders.push(if end { e } else { s } + d),
            None => {
                if let Some(abs) = tz::stamp(t) {
                    reminders.push(store::local_stamp(db, cal, &abs)?);
                }
            }
        }
    }
    reminders.sort();
    Ok((s, e.max(s), reminders))
}

fn event(c: &Comp, list_href: &str, (start, end, reminders): (i64, i64, Vec<i64>)) -> Event {
    Event {
        uid: c.text("UID").unwrap_or_default(),
        list_href: list_href.to_string(),
        recurrence_id: None,
        summary: c.text("SUMMARY").unwrap_or_default(),
        description: c.text("DESCRIPTION"),
        location: c.text("LOCATION"),
        all_day: matches!(c.prop("DTSTART").and_then(tz::stamp), Some(Stamp::Date(_))),
        start,
        end,
        recurring: false,
        cancelled: c
            .text("STATUS")
            .is_some_and(|s| s.eq_ignore_ascii_case("CANCELLED")),
        reminders,
    }
}

/// `UNTIL` as wall-clock time in the zone of `start` (inclusive; a date means its whole day).
fn until_wall(cal: &Calendar, rule: &Rule, start: &Stamp) -> Option<i64> {
    let u = rule.until.as_deref()?;
    let v = parse_time(u)?;
    Some(match start {
        _ if u.len() == 8 => v + 86399,
        Stamp::Zoned(_, z) if u.ends_with('Z') => {
            cal.zone(z).map_or(v, |zone| v + zone.offset_at_utc(v))
        }
        _ => v,
    })
}

/// Occurrences of the event in `cal` that overlap `[from, to)` (local times), soonest first.
pub(crate) fn occurrences(
    db: &Connection,
    cal: &Calendar,
    list_href: &str,
    from: i64,
    to: i64,
) -> Result<Vec<Event>> {
    let master = cal.main();
    let Some(start) = master.prop("DTSTART").and_then(tz::stamp) else {
        return Ok(Vec::new());
    };
    let overrides: Vec<(i64, Comp)> = cal
        .comps()
        .into_iter()
        .filter(|c| c.kind.eq_ignore_ascii_case("VEVENT"))
        .filter_map(|c| {
            let rid = tz::stamp(c.prop("RECURRENCE-ID")?)?;
            Some((store::local_stamp(db, cal, &rid).ok()?, c))
        })
        .collect();
    let rule = master.prop("RRULE").and_then(|l| Rule::parse(&l.value));
    let rdates: Vec<Stamp> = master
        .props("RDATE")
        .iter()
        .flat_map(|l| tz::stamps(l))
        .collect();
    let recurring = rule.is_some() || !rdates.is_empty() || !overrides.is_empty();
    if !recurring {
        let occ = occurrence(db, cal, &master, &start)?;
        let overlaps = occ.0 < to && (occ.1 > from || occ.0 >= from);
        return Ok(if overlaps {
            vec![event(&master, list_href, occ)]
        } else {
            vec![]
        });
    }

    let exdates: Vec<i64> = master
        .props("EXDATE")
        .iter()
        .flat_map(|l| tz::stamps(l))
        .filter_map(|s| store::local_stamp(db, cal, &s).ok())
        .collect();
    // Expand in the series' own wall-clock time, with room for zone differences and length.
    let dur = duration(&master);
    let margin = 2 * 86400 + dur;
    let mut starts: Vec<Stamp> = match &rule {
        Some(r) => r
            .expand(
                start.wall(),
                from - margin,
                to + 2 * 86400,
                until_wall(cal, r, &start),
            )
            .into_iter()
            .map(|w| at(&start, w))
            .collect(),
        None => vec![start.clone()],
    };
    starts.extend(rdates);

    let mut out = Vec::new();
    for s in starts {
        let local = store::local_stamp(db, cal, &s)?;
        if exdates.contains(&local) || overrides.iter().any(|(rid, _)| *rid == local) {
            continue;
        }
        let occ = occurrence(db, cal, &master, &s)?;
        if occ.0 < to && (occ.1 > from || occ.0 >= from) {
            out.push(Event {
                recurrence_id: Some(local),
                recurring: true,
                ..event(&master, list_href, occ)
            });
        }
    }
    // Overridden occurrences are where the override puts them, in range or not.
    for (rid, c) in &overrides {
        let Some(s) = c.prop("DTSTART").and_then(tz::stamp) else {
            continue;
        };
        let occ = occurrence(db, cal, c, &s)?;
        if occ.0 < to && (occ.1 > from || occ.0 >= from) && !exdates.contains(rid) {
            out.push(Event {
                recurrence_id: Some(*rid),
                recurring: true,
                ..event(c, list_href, occ)
            });
        }
    }
    out.sort_by_key(|e| (e.start, e.end));
    Ok(out)
}

/// First start and last end (local) of everything the object holds; no end for a series that
/// repeats forever.
pub(crate) fn span(db: &Connection, cal: &Calendar) -> Result<(i64, Option<i64>)> {
    let master = cal.main();
    let Some(start) = master.prop("DTSTART").and_then(tz::stamp) else {
        return Ok((0, Some(0)));
    };
    let first = occurrence(db, cal, &master, &start)?;
    let rule = master.prop("RRULE").and_then(|l| Rule::parse(&l.value));
    let endless = rule
        .as_ref()
        .is_some_and(|r| r.supported && r.count.is_none() && r.until.is_none());
    if endless {
        return Ok((first.0, None));
    }
    // Bounded: every occurrence, overrides and extra dates included.
    let all = occurrences(db, cal, "", i64::MIN / 4, i64::MAX / 4)?;
    let start = all
        .iter()
        .map(|e| e.start)
        .min()
        .unwrap_or(first.0)
        .min(first.0);
    let end = all
        .iter()
        .map(|e| e.end)
        .max()
        .unwrap_or(first.1)
        .max(first.1);
    Ok((start, Some(end)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cal(body: &str) -> Calendar {
        Calendar::parse(&format!("BEGIN:VCALENDAR\n{body}\nEND:VCALENDAR\n")).unwrap()
    }

    fn t(s: &str) -> i64 {
        parse_time(s).unwrap()
    }

    fn starts(db: &Connection, c: &Calendar, from: &str, to: &str) -> Vec<String> {
        occurrences(db, c, "/c/", t(from), t(to))
            .unwrap()
            .iter()
            .map(|e| format!("{} {}", &ical::fmt_utc(e.start)[..13], e.summary))
            .collect()
    }

    #[test]
    fn weekly_series_with_exdate_and_moved_occurrence() {
        let db = store::open(":memory:").unwrap();
        // Floating times, so the test does not depend on this machine's zone.
        let c = cal(
            "BEGIN:VEVENT\nUID:w\nSUMMARY:Standup\nDTSTART:20261005T090000\nDTEND:20261005T091500\nRRULE:FREQ=WEEKLY;BYDAY=MO,TH;UNTIL=20261019T235959\nEXDATE:20261008T090000\nEND:VEVENT\nBEGIN:VEVENT\nUID:w\nRECURRENCE-ID:20261012T090000\nSUMMARY:Standup (moved)\nDTSTART:20261013T110000\nDTEND:20261013T111500\nEND:VEVENT",
        );
        assert_eq!(
            starts(&db, &c, "20261001", "20261101"),
            [
                "20261005T0900 Standup",
                "20261013T1100 Standup (moved)",
                "20261015T0900 Standup",
                "20261019T0900 Standup"
            ]
        );
        let e = &occurrences(&db, &c, "/c/", t("20261013"), t("20261014")).unwrap()[0];
        assert_eq!(
            (e.recurrence_id, e.end - e.start, e.recurring),
            (Some(t("20261012T090000")), 900, true)
        );
        // Only the moved occurrence's own range finds it.
        assert_eq!(
            starts(&db, &c, "20261012", "20261013"),
            Vec::<String>::new()
        );
        assert_eq!(
            span(&db, &c).unwrap(),
            (t("20261005T090000"), Some(t("20261019T091500")))
        );
    }

    #[test]
    fn all_day_and_reminders() {
        let db = store::open(":memory:").unwrap();
        let c = cal(
            "BEGIN:VEVENT\nUID:b\nSUMMARY:Birthday\nDTSTART;VALUE=DATE:19900314\nDTEND;VALUE=DATE:19900315\nRRULE:FREQ=YEARLY\nBEGIN:VALARM\nACTION:DISPLAY\nTRIGGER:-PT15H\nEND:VALARM\nEND:VEVENT",
        );
        let e = occurrences(&db, &c, "/c/", t("20260101"), t("20270101")).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(
            (e[0].start, e[0].end, e[0].all_day),
            (t("20260314"), t("20260315"), true)
        );
        assert_eq!(e[0].reminders, [t("20260313T090000")]);
        // A day event overlaps its own day only.
        assert!(
            occurrences(&db, &c, "/c/", t("20260315"), t("20260316"))
                .unwrap()
                .is_empty()
        );
        assert_eq!(span(&db, &c).unwrap(), (t("19900314"), None));
    }

    #[test]
    fn single_event_in_another_zone() {
        let db = store::open(":memory:").unwrap();
        let c = cal(
            "BEGIN:VTIMEZONE\nTZID:Asia/Tokyo\nBEGIN:STANDARD\nTZOFFSETFROM:+0900\nTZOFFSETTO:+0900\nDTSTART:19700101T000000\nEND:STANDARD\nEND:VTIMEZONE\nBEGIN:VEVENT\nUID:j\nSUMMARY:Call\nDTSTART;TZID=Asia/Tokyo:20261020T180000\nDURATION:PT1H\nEND:VEVENT",
        );
        let utc = t("20261020T090000");
        let local = store::to_local(&db, utc).unwrap();
        let e = occurrences(&db, &c, "/c/", local - 1, local + 1).unwrap();
        assert_eq!(
            (e[0].start, e[0].end, e[0].recurring),
            (local, local + 3600, false)
        );
    }

    /// Every event in the exports in `<repo>/tmp/events/` expands without error over 2026, and
    /// bounded series end after they start.
    #[test]
    fn real_events_expand() {
        let db = store::open(":memory:").unwrap();
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tmp/events");
        for f in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let text = std::fs::read_to_string(f.path()).unwrap();
            // Group each export's VEVENTs by UID, as the server stores them, with all zones.
            let zones: String = text
                .split("BEGIN:VTIMEZONE")
                .skip(1)
                .map(|z| {
                    format!(
                        "BEGIN:VTIMEZONE{}END:VTIMEZONE\r\n",
                        z.split("END:VTIMEZONE").next().unwrap()
                    )
                })
                .collect();
            let mut by_uid: std::collections::BTreeMap<String, String> = Default::default();
            for ev in text.split("BEGIN:VEVENT").skip(1) {
                let body = ev.split("END:VEVENT").next().unwrap();
                let uid = body
                    .lines()
                    .find_map(|l| l.strip_prefix("UID:"))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                by_uid
                    .entry(uid)
                    .or_default()
                    .push_str(&format!("BEGIN:VEVENT{body}END:VEVENT\r\n"));
            }
            for (_, evs) in by_uid {
                let c =
                    Calendar::parse(&format!("BEGIN:VCALENDAR\r\n{zones}{evs}END:VCALENDAR\r\n"))
                        .unwrap();
                let year = occurrences(&db, &c, "/c/", t("20260101"), t("20270101")).unwrap();
                assert!(year.iter().all(|e| e.end >= e.start));
                let (first, last) = span(&db, &c).unwrap();
                assert!(last.is_none_or(|l| l >= first), "{evs}");
            }
        }
    }
}
