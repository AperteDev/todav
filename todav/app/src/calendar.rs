//! The calendar: events and dated tasks in the calendars shown, by day; where they go in a month
//! row or a week of hours; and what dropping one on a day, an hour or a list does.

use crate::{DAY, dates, prefs};
use std::collections::HashMap;
use todav_core::rrule::{Freq, Rule};
use todav_core::{Client, Due, Event, EventTime, Repeat, Result, Task, TaskPatch};

/// Something on the calendar.
#[derive(Clone, Debug)]
pub enum Item {
    Event(Event),
    Task(Task),
}

impl Item {
    /// The collection it is in.
    pub fn list(&self) -> &str {
        match self {
            Item::Event(e) => &e.list_href,
            Item::Task(t) => &t.list_href,
        }
    }

    pub fn summary(&self) -> &str {
        match self {
            Item::Event(e) => &e.summary,
            Item::Task(t) => &t.summary,
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
pub fn when(t: &Task) -> (i64, bool) {
    match t.start {
        Some(s) => (s, t.start_all_day),
        None => (t.due.unwrap_or(0), t.due_all_day),
    }
}

/// Events and dated tasks in `[from, to)` in the collections shown, all-day ones first, then
/// by time.
pub fn items(c: &Client, from: i64, to: i64) -> Vec<Item> {
    let hidden = prefs::hidden(c);
    let events = c.events(from, to).into_iter().map(Item::Event);
    let tasks = c.scheduled(from, to).into_iter().map(Item::Task);
    let mut out: Vec<Item> = events
        .chain(tasks)
        .filter(|i| !hidden.contains(i.list()))
        .collect();
    out.sort_by_key(Item::key);
    out
}

/// Whether `i` is on the day starting at `day`.
pub fn on_day(i: &Item, day: i64) -> bool {
    match i {
        // Zero-length events still show on their day.
        Item::Event(e) => e.start < day + DAY && (e.end > day || e.start >= day),
        Item::Task(t) => (day..day + DAY).contains(&when(t).0),
    }
}

/// The collections with something on `day` among `all`, three at most: a month day's dots.
pub fn day_lists(all: &[Item], day: i64) -> Vec<&str> {
    let mut lists: Vec<&str> = Vec::new();
    for i in all.iter().filter(|i| on_day(i, day)) {
        if lists.len() < 3 && !lists.contains(&i.list()) {
            lists.push(i.list());
        }
    }
    lists
}

/// Every collection's colour as the app shows it (`lists::color`), by href.
pub fn colors(c: &Client) -> HashMap<String, String> {
    c.calendars()
        .into_iter()
        .chain(c.lists())
        .map(|l| (l.href.clone(), crate::lists::color(&l)))
        .collect()
}

/// The colours of `day_lists` for each of `days` days from `from`.
pub fn day_colors(c: &Client, from: i64, days: i64) -> Vec<Vec<String>> {
    let colors = colors(c);
    let all = items(c, from, from + days * DAY);
    (0..days)
        .map(|n| {
            day_lists(&all, from + n * DAY)
                .into_iter()
                .map(|l| colors.get(l).cloned().unwrap_or_default())
                .collect()
        })
        .collect()
}

/// When an agenda row is on its day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Time {
    /// All of it: an all-day event, or a day inside a longer one.
    AllDay,
    /// From the day's start until then: the last day of a longer event.
    Until(i64),
    At(i64),
    /// A task due that day, without a time.
    Date,
}

/// An agenda row.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Midnight of its day.
    pub day: i64,
    pub time: Time,
    pub item: Item,
    pub color: String,
}

/// What is on each of `days` days from `from`, as the agenda lists it.
pub fn agenda(c: &Client, from: i64, days: i64) -> Vec<Entry> {
    let colors = colors(c);
    let all = items(c, from, from + days * DAY);
    let mut out = Vec::new();
    for day in (0..days).map(|n| from + n * DAY) {
        for i in all.iter().filter(|i| on_day(i, day)) {
            let time = match i {
                Item::Event(e) if e.all_day || (e.start < day && e.end >= day + DAY) => {
                    Time::AllDay
                }
                Item::Event(e) if e.start < day => Time::Until(e.end),
                Item::Event(e) => Time::At(e.start),
                Item::Task(t) => match when(t) {
                    (_, true) => Time::Date,
                    (at, false) => Time::At(at),
                },
            };
            out.push(Entry {
                day,
                time,
                item: i.clone(),
                color: colors.get(i.list()).cloned().unwrap_or_default(),
            });
        }
    }
    out
}

/// Today's events in the calendars shown (not cancelled), all-day ones first.
pub fn today_events(c: &Client) -> Vec<Event> {
    let hidden = prefs::hidden(c);
    let today = dates::midnight(c.local_now());
    let mut all = c.events(today, today + DAY);
    all.retain(|e| !hidden.contains(&e.list_href) && !e.cancelled);
    all.sort_by_key(|e| (!e.all_day, e.start));
    all
}

/// Events in the calendars shown that `hit` takes, within a year either way of `now`: each
/// series once, at its next time (or its last, when all are past); the coming ones first.
pub fn search(c: &Client, now: i64, hit: impl Fn(&Event) -> bool) -> Vec<Event> {
    let hidden = prefs::hidden(c);
    let today = dates::midnight(now);
    let ahead = |e: &Event| e.end > now;
    let mut best: HashMap<(String, String), Event> = HashMap::new();
    let all = c.events(today - 366 * DAY, today + 366 * DAY);
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
    found
}

// --- moving by drag and drop -----------------------------------------------------

/// What a drop does to an item: a task's new dates, or an event's new time (for a repeating
/// one, the front-end asks which occurrences it applies to).
#[derive(Debug, Clone)]
pub enum Change {
    Task(TaskPatch),
    Event(EventTime),
}

/// `i` dropped on the day starting at `day` (a month cell, the week's all-day row): it moves
/// there keeping its time; with `all_day`, an event becomes an all-day event and a task leaves
/// its time slot.
pub fn day_drop(i: &Item, day: i64, all_day: bool) -> Change {
    match i {
        // Out of the hours: due that day, no time (its due time was the slot's end).
        Item::Task(t) if all_day && t.start.is_some() && !t.start_all_day => {
            Change::Task(TaskPatch {
                start: Some(None),
                due: Some(Some(Due {
                    at: day,
                    all_day: true,
                })),
                ..Default::default()
            })
        }
        Item::Task(t) => Change::Task(move_to_day(t, day)),
        Item::Event(e) if all_day && !e.all_day => Change::Event(EventTime {
            start: day,
            end: day + DAY,
            all_day: true,
        }),
        Item::Event(e) => {
            let start = day + e.start.rem_euclid(DAY);
            Change::Event(EventTime {
                start,
                end: start + e.end - e.start,
                all_day: e.all_day,
            })
        }
    }
}

/// `i` dropped on the hour slot starting at `at`: a task is planned there, an event moves there
/// (as long as it was, or the default length).
pub fn slot_drop(c: &Client, i: &Item, at: i64) -> Change {
    match i {
        Item::Task(t) => Change::Task(plan_at(c, t, at)),
        Item::Event(_) => Change::Event(EventTime {
            start: at,
            end: at + drop_length(c, i) * 60,
            all_day: false,
        }),
    }
}

/// `i`, timed, resized in the week view to `[from, to)` by its top edge (`top`) or its bottom: a
/// task's top is when it starts, its bottom when it is due.
pub fn resize(i: &Item, from: i64, to: i64, top: bool) -> Change {
    match i {
        Item::Event(e) => Change::Event(EventTime {
            start: if top { from } else { e.start },
            end: if top { e.end } else { to },
            all_day: false,
        }),
        Item::Task(_) => {
            let at = |at| Some(Some(Due { at, all_day: false }));
            Change::Task(if top {
                TaskPatch {
                    start: at(from),
                    ..Default::default()
                }
            } else {
                TaskPatch {
                    due: at(to),
                    ..Default::default()
                }
            })
        }
    }
}

/// Move a task to the day starting at `day`, keeping its times of day (and its length when
/// it is planned into a slot); a task without a time is due that day.
pub fn move_to_day(t: &Task, day: i64) -> TaskPatch {
    let at = |t: i64, all_day| {
        Some(Due {
            at: day + t.rem_euclid(DAY),
            all_day,
        })
    };
    match (t.start, t.due) {
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
    }
}

/// Plan a task into the slot starting at `at`: as long as it was planned before, else the
/// default length.
pub fn plan_at(c: &Client, t: &Task, at: i64) -> TaskPatch {
    let len = match (t.start, slot_end(t)) {
        (Some(s), Some(e)) => e - s,
        _ => prefs::task_duration(c),
    };
    let time = |t| {
        Some(Some(Due {
            at: t,
            all_day: false,
        }))
    };
    TaskPatch {
        start: time(at),
        due: time(at + len),
        ..Default::default()
    }
}

/// Where a planned task's slot ends: its due time, when that is later the same day. A deadline
/// further off is not the end of a slot.
pub fn slot_end(t: &Task) -> Option<i64> {
    let s = t.start.filter(|_| !t.start_all_day)?;
    t.due
        .filter(|&d| !t.due_all_day && d > s && d <= s - s.rem_euclid(DAY) + DAY)
}

/// Whether `t` has a time slot: a start, or a due time rather than a date. Without one it is in
/// "To Plan" already.
pub fn planned(t: &Task) -> bool {
    t.start.is_some() || (t.due.is_some() && !t.due_all_day)
}

/// Back to "To Plan": no slot, due on the day it was planned for. None when it has no slot.
pub fn unplan(t: &Task) -> Option<TaskPatch> {
    if !planned(t) {
        return None;
    }
    let (at, _) = when(t);
    let due = t.due.map(|_| {
        Some(Due {
            at: dates::midnight(at),
            all_day: true,
        })
    });
    Some(TaskPatch {
        start: Some(None),
        due,
        ..Default::default()
    })
}

/// Move `i` to the list or calendar `href`: a subtask leaves its parent, the whole series of an
/// event goes. One change for undo.
pub fn move_to_list(c: &Client, i: &Item, href: String) -> Result<()> {
    match i {
        Item::Task(t) => c.together(|| {
            if t.parent_uid.is_some() {
                c.promote(t.uid.clone())?;
            }
            c.move_task(t.uid.clone(), href)
        }),
        Item::Event(e) => c.move_event(e.list_href.clone(), e.uid.clone(), href),
    }
}

// --- layout --------------------------------------------------------------------

/// Length in minutes of what a dropped item becomes: its own, else the default.
pub fn drop_length(c: &Client, i: &Item) -> i64 {
    match i {
        Item::Task(t) => match (t.start, slot_end(t)) {
            (Some(s), Some(e)) => (e - s) / 60,
            _ => prefs::task_duration(c) / 60,
        },
        Item::Event(e) if !e.all_day && e.end > e.start => (e.end - e.start) / 60,
        Item::Event(_) => prefs::event_duration(c) / 60,
    }
}

/// When an item takes hours in the week view, from and to; None for what goes above them.
pub fn span(c: &Client, i: &Item) -> Option<(i64, i64)> {
    match i {
        Item::Event(e) if !e.all_day => Some((e.start, e.end.max(e.start + 15 * 60))),
        // Only tasks planned into a slot (a start time) take hours; one just due at a time goes
        // above with the day's other items, its chip showing the time.
        Item::Task(t) => match t.start {
            Some(s) if !t.start_all_day => {
                Some((s, slot_end(t).unwrap_or(s + prefs::task_duration(c))))
            }
            _ => None,
        },
        _ => None,
    }
}

/// A timed item's part within each day of the week starting at `start`: (day of the week, from,
/// to in minutes into the day).
pub fn segments(c: &Client, i: &Item, start: i64) -> Vec<(i64, i64, i64)> {
    let Some((from, to)) = span(c, i) else {
        return Vec::new();
    };
    (0..7)
        .filter_map(|d| {
            let day = start + d * DAY;
            let (a, b) = (from.max(day), to.min(day + DAY));
            (a < b).then(|| (d, (a - day) / 60, (b - day) / 60))
        })
        .collect()
}

/// Columns for the blocks of one day, given as (index, from, to): overlapping ones side by
/// side. Returns (index, column, columns of its cluster).
pub fn columns(mut segs: Vec<(usize, i64, i64)>) -> Vec<(usize, usize, usize)> {
    segs.sort_by_key(|&(_, a, b)| (a, -b));
    let mut out = Vec::new();
    let mut cluster: Vec<(usize, i64, usize)> = Vec::new(); // (index, end, column)
    let mut cluster_end = i64::MIN;
    let flush = |cluster: &mut Vec<(usize, i64, usize)>, out: &mut Vec<(usize, usize, usize)>| {
        let cols = cluster.iter().map(|c| c.2 + 1).max().unwrap_or(1);
        out.extend(cluster.drain(..).map(|(i, _, col)| (i, col, cols)));
    };
    for (i, a, b) in segs {
        if a >= cluster_end {
            flush(&mut cluster, &mut out);
        }
        let col = (0..)
            .find(|col| !cluster.iter().any(|&(_, end, c)| c == *col && end > a))
            .unwrap_or(0);
        cluster.push((i, b, col));
        cluster_end = cluster_end.max(b);
    }
    flush(&mut cluster, &mut out);
    out
}

/// One piece of an item on a week's row: its lane and the days (columns 0–6) it covers.
#[derive(Debug, PartialEq)]
pub struct Piece {
    pub item: usize,
    pub lane: usize,
    pub from: usize,
    pub to: usize,
}

/// Lanes for items covering the days `days[i]` of a week's row (ascending, contiguous): the
/// earliest first, then the longest, each in the lowest lane free on all its days, so one
/// lasting several days is a single bar. At most `max` lanes show: a day with an item past them
/// keeps its last lane for "+N more", listing what it hides there.
pub fn lanes(days: &[Vec<usize>], max: usize) -> (Vec<Piece>, [Vec<usize>; 7]) {
    let mut order: Vec<usize> = (0..days.len()).filter(|&i| !days[i].is_empty()).collect();
    order.sort_by_key(|&i| (days[i][0], std::cmp::Reverse(days[i].len()), i));
    let mut taken: Vec<[bool; 7]> = Vec::new();
    let mut lane = vec![0; days.len()];
    for &i in &order {
        let free = |t: &[bool; 7]| days[i].iter().all(|&d| !t[d]);
        let l = taken.iter().position(free).unwrap_or(taken.len());
        if l == taken.len() {
            taken.push([false; 7]);
        }
        for &d in &days[i] {
            taken[l][d] = true;
        }
        lane[i] = l;
    }
    let limit: Vec<usize> = (0..7)
        .map(|d| {
            let over = order
                .iter()
                .any(|&i| days[i].contains(&d) && lane[i] >= max);
            if over { max - 1 } else { max }
        })
        .collect();
    let mut pieces = Vec::new();
    let mut more: [Vec<usize>; 7] = Default::default();
    for &i in &order {
        let mut run: Option<(usize, usize)> = None;
        for &d in &days[i] {
            if lane[i] < limit[d] {
                run = Some(run.map_or((d, d), |(from, _)| (from, d)));
            } else {
                more[d].push(i);
                if let Some((from, to)) = run.take() {
                    pieces.push(Piece {
                        item: i,
                        lane: lane[i],
                        from,
                        to,
                    });
                }
            }
        }
        if let Some((from, to)) = run {
            pieces.push(Piece {
                item: i,
                lane: lane[i],
                from,
                to,
            });
        }
    }
    for m in &mut more {
        m.sort(); // in the items' order
    }
    (pieces, more)
}

/// How an existing repeat rule shows in an event editor: as a plain repeat (every n days,
/// weeks, months or years, maybe until a day), or None for one only kept as it is ("Custom").
pub fn plain_repeat(rule: &str) -> Option<Repeat> {
    let r = Rule::parse(rule)?;
    let simple = r.supported
        && r.count.is_none()
        && r.by_day.len() <= 1
        && r.by_month_day.is_empty()
        && r.by_month.is_empty()
        && r.by_set_pos.is_empty();
    if !simple
        || !matches!(
            r.freq,
            Freq::Daily | Freq::Weekly | Freq::Monthly | Freq::Yearly
        )
    {
        return None;
    }
    let until = r.until.as_deref().and_then(todav_core::ical::parse_time);
    Some(Repeat {
        freq: r.freq,
        interval: r.interval,
        until: until.map(|u| u - u.rem_euclid(DAY)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(item: usize, lane: usize, from: usize, to: usize) -> Piece {
        Piece {
            item,
            lane,
            from,
            to,
        }
    }

    #[test]
    fn multi_day_items_keep_a_lane_and_overflow_goes_to_more() {
        // 0: Mon–Wed, 1: Tue, 2: Tue–Sun, 3: Wed, 4: Wed
        let days = vec![
            vec![0, 1, 2],
            vec![1],
            vec![1, 2, 3, 4, 5, 6],
            vec![2],
            vec![2],
        ];
        let (pieces, more) = lanes(&days, 3);
        assert_eq!(
            pieces,
            vec![
                piece(0, 0, 0, 2),
                // The longer one first on Tuesday, under the bar from Monday.
                piece(2, 1, 1, 6),
                piece(1, 2, 1, 1),
            ]
        );
        // Wednesday has four: its third lane shows "+2 more" for the two singles.
        assert_eq!(more[2], vec![3, 4]);
        assert!(more.iter().enumerate().all(|(d, m)| d == 2 || m.is_empty()));

        // Without a limit everything gets a lane.
        let (pieces, more) = lanes(&days, usize::MAX);
        assert_eq!(pieces.len(), 5);
        assert!(more.iter().all(Vec::is_empty));
    }

    #[test]
    fn overlapping_blocks_share_the_width() {
        // a 9-10, b 9:30-11, c 10-10:30 (beside b), d 12-13 alone.
        let laid = columns(vec![
            (0, 540, 600),
            (1, 570, 660),
            (2, 600, 630),
            (3, 720, 780),
        ]);
        let at = |i| *laid.iter().find(|l| l.0 == i).unwrap();
        assert_eq!(at(0), (0, 0, 2));
        assert_eq!(at(1), (1, 1, 2));
        assert_eq!(at(2), (2, 0, 2), "c takes a's freed column");
        assert_eq!(at(3), (3, 0, 1));
    }

    #[test]
    fn a_task_moved_to_another_day_keeps_its_times() {
        let at = |s| todav_core::ical::parse_time(s).unwrap();
        let t = Task {
            start: Some(at("20261014T090000")),
            due: Some(at("20261014T103000")),
            ..crate::tasks::tests::task("t", None)
        };
        let p = move_to_day(&t, at("20261016"));
        let due = |d: Option<Option<Due>>| d.flatten().map(|d| (d.at, d.all_day));
        assert_eq!(due(p.start), Some((at("20261016T090000"), false)));
        assert_eq!(due(p.due), Some((at("20261016T103000"), false)));
        assert_eq!(slot_end(&t), Some(at("20261014T103000")));
        let back = unplan(&t).unwrap();
        assert_eq!(due(back.due), Some((at("20261014"), true)));
        assert!(matches!(back.start, Some(None)));
    }

    #[test]
    fn plain_rules_show_as_choices() {
        let p = |r: &str| plain_repeat(r).map(|r| (r.freq, r.interval, r.until.is_some()));
        assert_eq!(p("FREQ=WEEKLY;INTERVAL=2"), Some((Freq::Weekly, 2, false)));
        assert_eq!(p("FREQ=YEARLY"), Some((Freq::Yearly, 1, false)));
        assert_eq!(
            p("FREQ=DAILY;UNTIL=20261031T235959Z"),
            Some((Freq::Daily, 1, true))
        );
        assert_eq!(p("FREQ=WEEKLY;BYDAY=MO,WE,FR"), None, "custom");
        assert_eq!(p("FREQ=MONTHLY;COUNT=3"), None, "custom");
    }
}
