//! Events: expanding a stored VEVENT object (a single event, or a recurring series with its
//! RDATE / EXDATE / overridden occurrences) into occurrences in local time.

use crate::ical::{self, Calendar, Comp, parse_time};
use crate::rrule::{Freq, Rule};
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
    /// The series' RRULE (raw), for showing and editing how it repeats.
    pub rule: Option<String>,
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
        rule: None,
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
    let rule_text = master.prop("RRULE").map(|l| l.value.clone());
    let rule = rule_text.as_deref().and_then(Rule::parse);
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
                rule: rule_text.clone(),
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
                rule: rule_text.clone(),
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

// --- editing -------------------------------------------------------------------

/// When an event is, in local time: start and end (exclusive; an all-day event ends at the
/// midnight after its last day).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventTime {
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
}

/// A plain repeat rule, as the event dialog edits it.
#[derive(Debug, Clone, PartialEq)]
pub struct Repeat {
    pub freq: Freq,
    pub interval: i64,
    /// The last day it may happen (local midnight).
    pub until: Option<i64>,
}

/// Fields left `None` stay; `Some("")` clears location and description.
#[derive(Debug, Clone, Default)]
pub struct EventPatch {
    pub summary: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub time: Option<EventTime>,
    /// Seconds from the start (negative: before); replaces all reminders.
    pub reminders: Option<Vec<i64>>,
    /// Some(None) stops repeating. Applies to the whole series.
    pub repeat: Option<Option<Repeat>>,
}

/// Which occurrences of a recurring event an edit or delete is for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scope {
    This,
    Following,
    All,
}

const DAY: i64 = 86400;

/// Local time `t` as a stamp written like `like` (its zone, UTC or floating; a date stays a
/// time in the zone of `fallback`), or a date when `all_day`.
fn to_stamp(
    db: &Connection,
    cal: &Calendar,
    like: &Stamp,
    fallback: &Stamp,
    t: i64,
    all_day: bool,
) -> Result<Stamp> {
    if all_day {
        return Ok(Stamp::Date(t - t.rem_euclid(DAY)));
    }
    let like = if matches!(like, Stamp::Date(_)) {
        fallback
    } else {
        like
    };
    Ok(match like {
        Stamp::Zoned(_, z) => match cal.zone(z) {
            Some(zone) => {
                let utc = store::to_utc(db, t)?;
                Stamp::Zoned(utc + zone.offset_at_utc(utc), z.clone())
            }
            None => Stamp::Zoned(t, z.clone()), // a TZID without a VTIMEZONE reads as local
        },
        Stamp::Utc(_) => Stamp::Utc(store::to_utc(db, t)?),
        _ => Stamp::Floating(t),
    })
}

/// (params, value) of a stamp.
fn render(s: &Stamp) -> (String, String) {
    let full = |t: i64| ical::fmt_utc(t)[..15].to_string();
    match s {
        Stamp::Date(d) => (";VALUE=DATE".into(), ical::fmt_utc(*d)[..8].into()),
        Stamp::Floating(t) => (String::new(), full(*t)),
        Stamp::Utc(t) => (String::new(), ical::fmt_utc(*t)),
        Stamp::Zoned(t, z) => {
            let z = if z.contains([':', ';', ',']) {
                format!("\"{z}\"")
            } else {
                z.clone()
            };
            (format!(";TZID={z}"), full(*t))
        }
    }
}

fn set_stamp(cal: &mut Calendar, comp: usize, name: &str, s: Option<&Stamp>) {
    match s.map(render) {
        Some((p, v)) => cal.set_in(comp, name, &p, Some(&v)),
        None => cal.set_in(comp, name, "", None),
    }
}

/// The VEVENTs overriding single occurrences: (component index, occurrence's planned start).
fn overrides(db: &Connection, cal: &Calendar) -> Vec<(usize, i64)> {
    cal.comps()
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind.eq_ignore_ascii_case("VEVENT"))
        .filter_map(|(i, c)| {
            let rid = tz::stamp(c.prop("RECURRENCE-ID")?)?;
            Some((i, store::local_stamp(db, cal, &rid).ok()?))
        })
        .collect()
}

fn master_start(cal: &Calendar) -> Stamp {
    cal.main()
        .prop("DTSTART")
        .and_then(tz::stamp)
        .unwrap_or(Stamp::Floating(0))
}

pub(crate) fn is_recurring(cal: &Calendar) -> bool {
    let m = cal.main();
    m.prop("RRULE").is_some() || m.prop("RDATE").is_some() || m.prop("RECURRENCE-ID").is_some()
}

/// Bump LAST-MODIFIED, DTSTAMP and SEQUENCE of component `comp` after an edit.
fn touch_in(cal: &mut Calendar, comp: usize) {
    let t = ical::fmt_utc(crate::now());
    let seq = cal.comps()[comp]
        .text("SEQUENCE")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    cal.set_in(comp, "LAST-MODIFIED", "", Some(&t));
    cal.set_in(comp, "DTSTAMP", "", Some(&t));
    cal.set_in(comp, "SEQUENCE", "", Some(&(seq + 1).to_string()));
}

/// UNTIL for a series starting like `start`, ending after local time `last`: a date for dates,
/// floating for floating times, else UTC.
fn until_value(db: &Connection, start: &Stamp, last: i64) -> Result<String> {
    Ok(match start {
        Stamp::Date(_) => ical::fmt_utc(last)[..8].to_string(),
        Stamp::Floating(_) => ical::fmt_utc(last)[..15].to_string(),
        _ => ical::fmt_utc(store::to_utc(db, last)?),
    })
}

fn rule_value(db: &Connection, start: &Stamp, r: &Repeat) -> Result<String> {
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
    if let Some(u) = r.until {
        v += &format!(";UNTIL={}", until_value(db, start, u + DAY - 1)?);
    }
    Ok(v)
}

/// Apply `p` to component `comp`.
fn apply(
    db: &Connection,
    cal: &mut Calendar,
    comp: usize,
    p: &EventPatch,
    fallback: &Stamp,
) -> Result<()> {
    let text = |s: &str| Some(ical::escape(s)).filter(|s| !s.is_empty());
    if let Some(s) = p.summary.as_deref() {
        cal.set_in(comp, "SUMMARY", "", text(s).as_deref());
    }
    if let Some(s) = p.location.as_deref() {
        cal.set_in(comp, "LOCATION", "", text(s).as_deref());
    }
    if let Some(s) = p.description.as_deref() {
        cal.set_in(comp, "DESCRIPTION", "", text(s).as_deref());
    }
    if let Some(t) = p.time {
        let like = cal.comps()[comp]
            .prop("DTSTART")
            .and_then(tz::stamp)
            .unwrap_or(fallback.clone());
        let start = to_stamp(db, cal, &like, fallback, t.start, t.all_day)?;
        let end = to_stamp(db, cal, &like, fallback, t.end.max(t.start), t.all_day)?;
        set_stamp(cal, comp, "DTSTART", Some(&start));
        set_stamp(cal, comp, "DTEND", Some(&end));
        cal.set_in(comp, "DURATION", "", None);
    }
    if let Some(r) = &p.reminders {
        let v: Vec<String> = r.iter().map(|o| ical::fmt_duration(*o)).collect();
        let t: Vec<(&str, &str)> = v.iter().map(|v| ("", v.as_str())).collect();
        cal.set_alarms_in(comp, &t);
    }
    if let Some(r) = &p.repeat {
        let start = cal.comps()[comp]
            .prop("DTSTART")
            .and_then(tz::stamp)
            .unwrap_or(fallback.clone());
        let v = r.as_ref().map(|r| rule_value(db, &start, r)).transpose()?;
        cal.set_in(comp, "RRULE", "", v.as_deref());
    }
    touch_in(cal, comp);
    Ok(())
}

/// The override for occurrence `rid`, made from the master if there is none yet.
fn override_for(db: &Connection, cal: &mut Calendar, rid: i64, fallback: &Stamp) -> Result<usize> {
    if let Some((i, _)) = overrides(db, cal).into_iter().find(|(_, r)| *r == rid) {
        return Ok(i);
    }
    let master = cal.main_index().unwrap_or(0);
    let start = master_start(cal);
    let len = duration(&cal.main());
    let occ = to_stamp(
        db,
        cal,
        &start,
        fallback,
        rid,
        matches!(start, Stamp::Date(_)),
    )?;
    let end = match &occ {
        Stamp::Date(d) => Stamp::Date(d + len),
        s => at(s, s.wall() + len),
    };
    let lines = cal.comp_lines(master);
    cal.insert_comp(master + 1, lines);
    let i = master + 1;
    for name in ["RRULE", "RDATE", "EXDATE", "DURATION"] {
        cal.set_in(i, name, "", None);
    }
    set_stamp(cal, i, "RECURRENCE-ID", Some(&occ));
    set_stamp(cal, i, "DTSTART", Some(&occ));
    set_stamp(cal, i, "DTEND", Some(&end));
    Ok(i)
}

/// EXDATEs of the master, in local time.
fn exdates(db: &Connection, cal: &Calendar) -> Vec<i64> {
    let lines = cal.main().props("EXDATE");
    let stamps = lines.iter().flat_map(|l| tz::stamps(l));
    stamps
        .filter_map(|s| store::local_stamp(db, cal, &s).ok())
        .collect()
}

/// Rewrite the master's EXDATEs (local times) in its own form.
fn set_exdates(db: &Connection, cal: &mut Calendar, days: &[i64], fallback: &Stamp) -> Result<()> {
    let master = cal.main_index().unwrap_or(0);
    let start = master_start(cal);
    let all_day = matches!(start, Stamp::Date(_));
    let stamps = days
        .iter()
        .map(|d| to_stamp(db, cal, &start, fallback, *d, all_day))
        .collect::<Result<Vec<_>>>()?;
    match stamps.first().map(render) {
        Some((p, _)) => {
            let v: Vec<String> = stamps.iter().map(|s| render(s).1).collect();
            cal.set_in(master, "EXDATE", &p, Some(&v.join(",")));
        }
        None => cal.set_in(master, "EXDATE", "", None),
    }
    Ok(())
}

/// End the series before occurrence `rid`; overrides and exceptions from there on go.
fn end_before(db: &Connection, cal: &mut Calendar, rid: i64, fallback: &Stamp) -> Result<()> {
    for (i, r) in overrides(db, cal).into_iter().rev() {
        if r >= rid {
            cal.remove_comp(i);
        }
    }
    let keep: Vec<i64> = exdates(db, cal).into_iter().filter(|d| *d < rid).collect();
    set_exdates(db, cal, &keep, fallback)?;
    let master = cal.main_index().unwrap_or(0);
    let start = master_start(cal);
    let last = if matches!(start, Stamp::Date(_)) {
        rid - DAY
    } else {
        rid - 1
    };
    let rule = cal
        .main()
        .prop("RRULE")
        .map(|l| l.value.clone())
        .unwrap_or_default();
    let mut parts: Vec<&str> = rule
        .split(';')
        .filter(|p| !p.is_empty() && !p.starts_with("UNTIL=") && !p.starts_with("COUNT="))
        .collect();
    let until = format!("UNTIL={}", until_value(db, &start, last)?);
    parts.push(&until);
    cal.set_in(master, "RRULE", "", Some(&parts.join(";")));
    touch_in(cal, master);
    Ok(())
}

/// Shift overrides (where they were planned and where they are) and exceptions by `delta`
/// (the series moved).
fn shift_exceptions(
    db: &Connection,
    cal: &mut Calendar,
    delta: i64,
    fallback: &Stamp,
) -> Result<()> {
    let start = master_start(cal);
    let all_day = matches!(start, Stamp::Date(_));
    for (i, rid) in overrides(db, cal) {
        let s = to_stamp(db, cal, &start, fallback, rid + delta, all_day)?;
        set_stamp(cal, i, "RECURRENCE-ID", Some(&s));
        for name in ["DTSTART", "DTEND"] {
            let Some(old) = cal.comps()[i].prop(name).and_then(tz::stamp) else {
                continue;
            };
            let t = store::local_stamp(db, cal, &old)? + delta;
            let new = to_stamp(db, cal, &old, fallback, t, matches!(old, Stamp::Date(_)))?;
            set_stamp(cal, i, name, Some(&new));
        }
    }
    let days: Vec<i64> = exdates(db, cal).iter().map(|d| d + delta).collect();
    if !days.is_empty() {
        set_exdates(db, cal, &days, fallback)?;
    }
    Ok(())
}

/// Apply `p` to occurrence `rid` (local; None for an event that does not repeat) for `scope`.
/// Splitting a series ("this and following") returns the new series to store beside it.
pub(crate) fn edit(
    db: &Connection,
    cal: &mut Calendar,
    rid: Option<i64>,
    scope: Scope,
    p: &EventPatch,
    fallback: &Stamp,
) -> Result<Option<Calendar>> {
    let master = cal.main_index().unwrap_or(0);
    let first = store::local_stamp(db, cal, &master_start(cal))?;
    let (Some(rid), true) = (rid, is_recurring(cal)) else {
        apply(db, cal, master, p, fallback)?;
        return Ok(None);
    };
    match scope {
        Scope::This => {
            let i = override_for(db, cal, rid, fallback)?;
            let p = EventPatch {
                repeat: None,
                ..p.clone()
            };
            apply(db, cal, i, &p, fallback)?;
            Ok(None)
        }
        Scope::All => {
            // Moving an occurrence moves the whole series as much.
            let mut p = p.clone();
            if let Some(t) = p.time {
                let delta = t.start - rid;
                p.time = Some(EventTime {
                    start: first + delta,
                    end: first + delta + (t.end - t.start),
                    all_day: t.all_day,
                });
                if delta != 0 {
                    shift_exceptions(db, cal, delta, fallback)?;
                }
            }
            apply(db, cal, master, &p, fallback)?;
            Ok(None)
        }
        Scope::Following if rid <= first => edit(db, cal, Some(rid), Scope::All, p, fallback),
        Scope::Following => {
            let mut next = cal.clone();
            end_before(db, cal, rid, fallback)?;
            // The new series: from this occurrence on, under a new UID.
            for (i, r) in overrides(db, &next).into_iter().rev() {
                if r < rid {
                    next.remove_comp(i);
                }
            }
            let keep: Vec<i64> = exdates(db, &next)
                .into_iter()
                .filter(|d| *d >= rid)
                .collect();
            set_exdates(db, &mut next, &keep, fallback)?;
            let m = next.main_index().unwrap_or(0);
            let rule = next
                .main()
                .prop("RRULE")
                .and_then(|l| Rule::parse(&l.value));
            if let Some(count) = rule.as_ref().and_then(|r| r.count) {
                // As many as were left.
                let before = occurrences(db, cal, "", i64::MIN / 4, rid)?.len() as u32;
                let v = next
                    .main()
                    .prop("RRULE")
                    .map(|l| l.value.clone())
                    .unwrap_or_default();
                let v = v.replace(
                    &format!("COUNT={count}"),
                    &format!("COUNT={}", count.saturating_sub(before).max(1)),
                );
                next.set_in(m, "RRULE", "", Some(&v));
            }
            let len = duration(&next.main());
            let start = master_start(&next);
            let all_day = matches!(start, Stamp::Date(_));
            let s = to_stamp(db, &next, &start, fallback, rid, all_day)?;
            let e = match &s {
                Stamp::Date(d) => Stamp::Date(d + len),
                s => at(s, s.wall() + len),
            };
            set_stamp(&mut next, m, "DTSTART", Some(&s));
            set_stamp(&mut next, m, "DTEND", Some(&e));
            next.set_in(m, "DURATION", "", None);
            let uid = ical::escape(&crate::new_uid());
            let comps = next.comps().len();
            for i in 0..comps {
                if next.comps()[i].kind.eq_ignore_ascii_case("VEVENT") {
                    next.set_in(i, "UID", "", Some(&uid));
                }
            }
            edit(db, &mut next, Some(rid), Scope::All, p, fallback)?;
            Ok(Some(next))
        }
    }
}

/// Remove occurrence `rid` (None: the event) for `scope`; true when nothing is left and the
/// whole object should go.
pub(crate) fn delete(
    db: &Connection,
    cal: &mut Calendar,
    rid: Option<i64>,
    scope: Scope,
    fallback: &Stamp,
) -> Result<bool> {
    let first = store::local_stamp(db, cal, &master_start(cal))?;
    let (Some(rid), true) = (rid, is_recurring(cal)) else {
        return Ok(true);
    };
    match scope {
        Scope::All => Ok(true),
        Scope::Following if rid <= first => Ok(true),
        Scope::Following => {
            end_before(db, cal, rid, fallback)?;
            Ok(false)
        }
        Scope::This => {
            if let Some((i, _)) = overrides(db, cal).into_iter().find(|(_, r)| *r == rid) {
                cal.remove_comp(i);
            }
            let mut days = exdates(db, cal);
            days.push(rid);
            set_exdates(db, cal, &days, fallback)?;
            let master = cal.main_index().unwrap_or(0);
            touch_in(cal, master);
            Ok(false)
        }
    }
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

    const FLOAT: Stamp = Stamp::Floating(0);

    fn weekly(extra: &str) -> Calendar {
        cal(&format!(
            "BEGIN:VEVENT\nUID:w\nSUMMARY:Standup\nDTSTART:20261005T090000\nDTEND:20261005T091500\nRRULE:FREQ=WEEKLY{extra}\nEND:VEVENT"
        ))
    }

    fn times(db: &Connection, c: &Calendar) -> Vec<String> {
        starts(db, c, "20261001", "20261201")
    }

    fn moved(start: &str, end: &str) -> EventPatch {
        EventPatch {
            time: Some(EventTime {
                start: t(start),
                end: t(end),
                all_day: false,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn this_occurrence_gets_an_override() {
        let db = store::open(":memory:").unwrap();
        let mut c = weekly(";COUNT=3");
        let p = EventPatch {
            summary: Some("Moved".into()),
            ..moved("20261013T110000", "20261013T111500")
        };
        assert!(
            edit(
                &db,
                &mut c,
                Some(t("20261012T090000")),
                Scope::This,
                &p,
                &FLOAT
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(
            times(&db, &c),
            [
                "20261005T0900 Standup",
                "20261013T1100 Moved",
                "20261019T0900 Standup"
            ]
        );
        // Again: the same override, not a second one.
        let p = EventPatch {
            summary: Some("Moved again".into()),
            ..Default::default()
        };
        edit(
            &db,
            &mut c,
            Some(t("20261012T090000")),
            Scope::This,
            &p,
            &FLOAT,
        )
        .unwrap();
        assert_eq!(c.to_ics().matches("RECURRENCE-ID").count(), 1);
        assert_eq!(times(&db, &c)[1], "20261013T1100 Moved again");
    }

    #[test]
    fn all_moves_the_series_with_its_exceptions() {
        let db = store::open(":memory:").unwrap();
        let mut c = weekly(";COUNT=4\nEXDATE:20261012T090000");
        edit(
            &db,
            &mut c,
            Some(t("20261019T090000")),
            Scope::This,
            &EventPatch {
                summary: Some("Late".into()),
                ..Default::default()
            },
            &FLOAT,
        )
        .unwrap();
        // One hour later, from any occurrence.
        edit(
            &db,
            &mut c,
            Some(t("20261019T090000")),
            Scope::All,
            &moved("20261019T100000", "20261019T101500"),
            &FLOAT,
        )
        .unwrap();
        assert_eq!(
            times(&db, &c),
            [
                "20261005T1000 Standup",
                "20261019T1000 Late",
                "20261026T1000 Standup"
            ]
        );
    }

    #[test]
    fn following_splits_the_series() {
        let db = store::open(":memory:").unwrap();
        let mut c = weekly(";COUNT=4");
        let p = EventPatch {
            summary: Some("New".into()),
            ..Default::default()
        };
        let next = edit(
            &db,
            &mut c,
            Some(t("20261019T090000")),
            Scope::Following,
            &p,
            &FLOAT,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            times(&db, &c),
            ["20261005T0900 Standup", "20261012T0900 Standup"]
        );
        assert_eq!(
            times(&db, &next),
            ["20261019T0900 New", "20261026T0900 New"]
        );
        assert_ne!(next.text("UID"), c.text("UID"));
        // From the first occurrence it is the whole series.
        let mut c = weekly(";COUNT=2");
        assert!(
            edit(
                &db,
                &mut c,
                Some(t("20261005T090000")),
                Scope::Following,
                &p,
                &FLOAT
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(times(&db, &c), ["20261005T0900 New", "20261012T0900 New"]);
    }

    #[test]
    fn deleting_occurrences() {
        let db = store::open(":memory:").unwrap();
        let mut c = weekly(";COUNT=4");
        assert!(!delete(&db, &mut c, Some(t("20261012T090000")), Scope::This, &FLOAT).unwrap());
        assert!(
            !delete(
                &db,
                &mut c,
                Some(t("20261026T090000")),
                Scope::Following,
                &FLOAT
            )
            .unwrap()
        );
        assert_eq!(
            times(&db, &c),
            ["20261005T0900 Standup", "20261019T0900 Standup"]
        );
        assert!(
            delete(
                &db,
                &mut c,
                Some(t("20261005T090000")),
                Scope::Following,
                &FLOAT
            )
            .unwrap(),
            "from the start: all"
        );
        let mut single = cal("BEGIN:VEVENT\nUID:s\nDTSTART:20261005T090000\nEND:VEVENT");
        assert!(delete(&db, &mut single, None, Scope::This, &FLOAT).unwrap());
    }

    #[test]
    fn repeat_set_and_cleared() {
        let db = store::open(":memory:").unwrap();
        let mut c = cal(
            "BEGIN:VEVENT\nUID:s\nSUMMARY:S\nDTSTART;VALUE=DATE:20261005\nDTEND;VALUE=DATE:20261006\nEND:VEVENT",
        );
        let repeat = Repeat {
            freq: Freq::Weekly,
            interval: 2,
            until: Some(t("20261102")),
        };
        edit(
            &db,
            &mut c,
            None,
            Scope::All,
            &EventPatch {
                repeat: Some(Some(repeat)),
                ..Default::default()
            },
            &FLOAT,
        )
        .unwrap();
        assert_eq!(
            c.text("RRULE").as_deref(),
            Some("FREQ=WEEKLY;INTERVAL=2;UNTIL=20261102")
        );
        assert_eq!(times(&db, &c).len(), 3, "5, 19 October, 2 November");
        edit(
            &db,
            &mut c,
            Some(t("20261019")),
            Scope::All,
            &EventPatch {
                repeat: Some(None),
                ..Default::default()
            },
            &FLOAT,
        )
        .unwrap();
        assert_eq!(times(&db, &c), ["20261005T0000 S"]);
    }

    #[test]
    fn zoned_events_stay_in_their_zone() {
        let db = store::open(":memory:").unwrap();
        let mut c = cal(
            "BEGIN:VTIMEZONE\nTZID:Asia/Tokyo\nBEGIN:STANDARD\nTZOFFSETFROM:+0900\nTZOFFSETTO:+0900\nDTSTART:19700101T000000\nEND:STANDARD\nEND:VTIMEZONE\nBEGIN:VEVENT\nUID:j\nSUMMARY:Call\nDTSTART;TZID=Asia/Tokyo:20261020T180000\nDTEND;TZID=Asia/Tokyo:20261020T190000\nEND:VEVENT",
        );
        // To 10:00 UTC, whatever this machine's zone: 19:00 in Tokyo.
        let local = store::to_local(&db, t("20261020T100000")).unwrap();
        let p = EventPatch {
            time: Some(EventTime {
                start: local,
                end: local + 3600,
                all_day: false,
            }),
            ..Default::default()
        };
        edit(&db, &mut c, None, Scope::All, &p, &FLOAT).unwrap();
        assert!(
            c.to_ics()
                .contains("DTSTART;TZID=Asia/Tokyo:20261020T190000"),
            "{}",
            c.to_ics()
        );
        let e = &occurrences(&db, &c, "/c/", local - 1, local + 1).unwrap()[0];
        assert_eq!((e.start, e.end), (local, local + 3600));
    }
}
