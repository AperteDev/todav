//! SQLite schema and row mapping. Every derived column is recomputed from `raw_ics`.

use crate::event::{self, Event};
use crate::ical::{self, Calendar, Line};
use crate::tz::{self, Stamp};
use crate::{List, Result, Task};
use rusqlite::{Connection, OptionalExtension, params};

const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE lists (
  href TEXT PRIMARY KEY, display_name TEXT NOT NULL, color TEXT,
  ctag TEXT, sync_token TEXT, push_topic TEXT,
  push_registration_href TEXT, push_expires INTEGER
);
CREATE TABLE tasks (
  uid TEXT PRIMARY KEY, list_href TEXT NOT NULL REFERENCES lists(href) ON DELETE CASCADE,
  etag TEXT, href TEXT, raw_ics TEXT NOT NULL,
  summary TEXT NOT NULL, description TEXT, status TEXT NOT NULL, completed_at INTEGER,
  category TEXT, parent_uid TEXT, priority INTEGER, due INTEGER,
  sort_order INTEGER, created INTEGER, last_modified INTEGER,
  dirty INTEGER NOT NULL DEFAULT 0,     -- 0 clean, 1 modified, 2 deleted locally
  deleted_remote INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX tasks_list_cat ON tasks(list_href, category, status);
CREATE INDEX tasks_href ON tasks(href);
CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT);
"#,
    r#"
ALTER TABLE tasks ADD COLUMN due_all_day INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN remind_at INTEGER;
"#,
    r#"
CREATE TABLE alarms (
  uid TEXT NOT NULL REFERENCES tasks(uid) ON DELETE CASCADE,
  at INTEGER NOT NULL                   -- local time, see `to_local`
);
CREATE INDEX alarms_uid ON alarms(uid);
CREATE INDEX alarms_at ON alarms(at);
ALTER TABLE tasks DROP COLUMN remind_at;
"#,
    r#"
ALTER TABLE lists ADD COLUMN has_tasks INTEGER NOT NULL DEFAULT 1;
ALTER TABLE lists ADD COLUMN has_events INTEGER NOT NULL DEFAULT 0;
ALTER TABLE lists ADD COLUMN writable INTEGER NOT NULL DEFAULT 1;
-- Pull everything again: events in calendars that also hold tasks were skipped so far.
UPDATE lists SET sync_token = NULL;
ALTER TABLE tasks ADD COLUMN start INTEGER;
ALTER TABLE tasks ADD COLUMN start_all_day INTEGER NOT NULL DEFAULT 0;
CREATE TABLE events (
  list_href TEXT NOT NULL REFERENCES lists(href) ON DELETE CASCADE,
  uid TEXT NOT NULL,
  etag TEXT, href TEXT, raw_ics TEXT NOT NULL,
  summary TEXT NOT NULL,
  first_start INTEGER NOT NULL,         -- local time, see `to_local`
  last_end INTEGER,                     -- NULL: repeats forever
  last_modified INTEGER,
  dirty INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (list_href, uid)
);
CREATE INDEX events_href ON events(href);
CREATE INDEX events_span ON events(first_start, last_end);
"#,
    r#"
ALTER TABLE tasks ADD COLUMN event_uid TEXT;
CREATE INDEX tasks_event ON tasks(event_uid);
"#,
];

/// RELTYPE of the RELATED-TO linking a task to the event it is for (a custom type: PARENT would
/// make the event a parent task to other clients, SIBLING means something else).
pub const EVENT_LINK: &str = "X-TODAV-EVENT";

/// What a stored calendar object is. Tasks are keyed by UID, events by calendar and UID (the
/// same invitation can sit in two calendars).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Task,
    Event,
}

impl Kind {
    pub fn of(cal: &Calendar) -> Kind {
        if cal.kind() == "VEVENT" {
            Kind::Event
        } else {
            Kind::Task
        }
    }

    fn table(self) -> &'static str {
        match self {
            Kind::Task => "tasks",
            Kind::Event => "events",
        }
    }

    /// WHERE clause for a row, with the calendar as ?1 and the UID as ?2.
    fn key(self) -> &'static str {
        match self {
            Kind::Task => "uid = ?2",
            Kind::Event => "list_href = ?1 AND uid = ?2",
        }
    }
}

pub const CLEAN: i64 = 0;
pub const MODIFIED: i64 = 1;
pub const DELETED: i64 = 2;

pub fn open(path: &str) -> Result<Connection> {
    let db = Connection::open(path)?;
    db.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, m) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = db.unchecked_transaction()?;
        tx.execute_batch(m)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    // Derived columns changed with every migration so far (local times, reminders, zones, start).
    if version > 0 && (version as usize) < MIGRATIONS.len() {
        rederive(&db)?;
    }
    Ok(db)
}

/// Recompute every object's derived columns from its calendar data.
fn rederive(db: &Connection) -> Result<()> {
    let mut all = Vec::new();
    for kind in [Kind::Task, Kind::Event] {
        let sql = format!("SELECT {RAW_COLS} FROM {}", kind.table());
        let rows = db
            .prepare(&sql)?
            .query_map([], |r| raw_row(r, kind))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        all.extend(rows);
    }
    for t in all {
        if let Some(cal) = Calendar::parse(&t.raw_ics) {
            save(
                db,
                &t.list_href,
                t.href.as_deref(),
                t.etag.as_deref(),
                &cal,
                t.dirty,
            )?;
        }
    }
    Ok(())
}

/// Local wall-clock time of a UTC timestamp, in the system time zone (as seconds since the
/// epoch, as if local time were UTC). Dates and times shown to the user use this.
pub fn to_local(db: &Connection, utc: i64) -> Result<i64> {
    Ok(db.query_row(
        "SELECT unixepoch(?1, 'unixepoch', 'localtime')",
        [utc],
        |r| r.get(0),
    )?)
}

/// Inverse of `to_local`.
pub fn to_utc(db: &Connection, local: i64) -> Result<i64> {
    Ok(
        db.query_row("SELECT unixepoch(?1, 'unixepoch', 'utc')", [local], |r| {
            r.get(0)
        })?,
    )
}

/// A stamp as local time: a zoned time through its VTIMEZONE; dates, floating times (and a TZID
/// without a VTIMEZONE) as they read.
pub fn local_stamp(db: &Connection, cal: &Calendar, s: &Stamp) -> Result<i64> {
    match cal.utc(s) {
        Some(u) => to_local(db, u),
        None => Ok(s.wall()),
    }
}

/// A DATE / DATE-TIME value as local time, and whether it is a date only.
fn local_time(db: &Connection, cal: &Calendar, l: Option<&Line>) -> Result<Option<(i64, bool)>> {
    let Some(s) = l.and_then(tz::stamp) else {
        return Ok(None);
    };
    Ok(Some((
        local_stamp(db, cal, &s)?,
        matches!(s, Stamp::Date(_)),
    )))
}

/// When each alarm goes off, in local time: an absolute TRIGGER, or one relative to DUE
/// (RELATED=END) or DTSTART (the default; DUE when there is no DTSTART).
fn alarm_times(db: &Connection, cal: &Calendar, due: Option<i64>) -> Result<Vec<i64>> {
    let start = local_time(db, cal, cal.prop("DTSTART"))?.map(|(s, _)| s);
    let mut out = Vec::new();
    for t in cal.alarm_triggers() {
        let at = match ical::parse_duration(&t.value) {
            Some(d) => {
                let end = t
                    .param("RELATED")
                    .is_some_and(|r| r.eq_ignore_ascii_case("END"));
                let base = if end { due } else { start.or(due) };
                base.map(|b| b + d)
            }
            None => local_time(db, cal, Some(t))?.map(|(t, _)| t),
        };
        out.extend(at);
    }
    out.sort();
    Ok(out)
}

pub fn kv_get(db: &Connection, key: &str) -> Result<Option<String>> {
    Ok(db
        .query_row("SELECT value FROM kv WHERE key = ?", [key], |r| r.get(0))
        .optional()?)
}

pub fn kv_set(db: &Connection, key: &str, value: &str) -> Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO kv (key, value) VALUES (?, ?)",
        [key, value],
    )?;
    Ok(())
}

pub fn lists(db: &Connection) -> Result<Vec<List>> {
    let mut st = db.prepare(
        "SELECT l.href, l.display_name, l.color,
           (SELECT COUNT(*) FROM tasks t WHERE t.list_href = l.href AND t.dirty != 2
              AND t.status NOT IN ('COMPLETED', 'CANCELLED')),
           l.has_tasks, l.has_events, l.writable
         FROM lists l ORDER BY l.display_name COLLATE NOCASE",
    )?;
    let rows = st.query_map([], |r| {
        Ok(List {
            href: r.get(0)?,
            display_name: r.get(1)?,
            color: r.get(2)?,
            open_count: r.get(3)?,
            holds_tasks: r.get(4)?,
            holds_events: r.get(5)?,
            writable: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

const TASK_COLS: &str = "uid, list_href, summary, description, status, completed_at, category, parent_uid, priority, due, due_all_day,
  (SELECT group_concat(at) FROM (SELECT at FROM alarms a WHERE a.uid = tasks.uid ORDER BY at)),
  sort_order, created, last_modified, start, start_all_day, event_uid";

fn task_row(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    let status: String = r.get(4)?;
    Ok(Task {
        uid: r.get(0)?,
        list_href: r.get(1)?,
        summary: r.get(2)?,
        description: r.get(3)?,
        done: status == "COMPLETED",
        status,
        completed_at: r.get(5)?,
        category: r.get(6)?,
        parent_uid: r.get(7)?,
        priority: r.get(8)?,
        due: r.get(9)?,
        due_all_day: r.get(10)?,
        reminders: r
            .get::<_, Option<String>>(11)?
            .unwrap_or_default()
            .split(',')
            .filter_map(|v| v.parse().ok())
            .collect(),
        sort_order: r.get(12)?,
        created: r.get(13)?,
        last_modified: r.get(14)?,
        start: r.get(15)?,
        start_all_day: r.get(16)?,
        event_uid: r.get(17)?,
    })
}

/// Tasks (done ones too) for the event with this UID.
pub fn event_tasks(db: &Connection, event_uid: &str) -> Result<Vec<Task>> {
    let mut st = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE event_uid = ?1 AND dirty != 2
         ORDER BY COALESCE(sort_order, created, 0), created, uid"
    ))?;
    let rows = st.query_map([event_uid], task_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn tasks(db: &Connection, list_href: &str, include_done: bool) -> Result<Vec<Task>> {
    let mut st = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE list_href = ?1 AND dirty != 2
           AND (?2 OR status NOT IN ('COMPLETED', 'CANCELLED'))
         ORDER BY COALESCE(sort_order, created, 0), created, uid"
    ))?;
    let rows = st.query_map(params![list_href, include_done], task_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Open tasks with a due date in every list, soonest first.
pub fn upcoming(db: &Connection) -> Result<Vec<Task>> {
    let mut st = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE dirty != 2 AND due IS NOT NULL
           AND status NOT IN ('COMPLETED', 'CANCELLED')
         ORDER BY due, COALESCE(sort_order, created, 0), uid"
    ))?;
    let rows = st.query_map([], task_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Tasks, done ones too, due or starting in `[from, to)` (local times), in every list.
pub fn scheduled(db: &Connection, from: i64, to: i64) -> Result<Vec<Task>> {
    let mut st = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE dirty != 2 AND status != 'CANCELLED'
           AND ((due >= ?1 AND due < ?2) OR (start >= ?1 AND start < ?2))
         ORDER BY COALESCE(start, due), COALESCE(sort_order, created, 0), uid"
    ))?;
    let rows = st.query_map([from, to], task_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Open tasks with no time slot yet, to plan: undated, or due by a date before `until`
/// (overdue included); dated ones first, soonest first.
pub fn unscheduled(db: &Connection, until: i64) -> Result<Vec<Task>> {
    let mut st = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE dirty != 2
           AND status NOT IN ('COMPLETED', 'CANCELLED')
           AND (start IS NULL OR start_all_day)
           AND (due IS NULL OR (due_all_day AND due < ?1))
         ORDER BY due IS NULL, due, list_href, COALESCE(sort_order, created, 0), uid"
    ))?;
    let rows = st.query_map([until], task_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Open tasks with a reminder going off after `from` and at `to` at the latest (local times).
pub fn reminders(db: &Connection, from: i64, to: i64) -> Result<Vec<Task>> {
    let mut st = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE dirty != 2
           AND uid IN (SELECT uid FROM alarms WHERE at > ?1 AND at <= ?2)
           AND status NOT IN ('COMPLETED', 'CANCELLED')
         ORDER BY due"
    ))?;
    let rows = st.query_map([from, to], task_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn task(db: &Connection, uid: &str) -> Result<Option<Task>> {
    Ok(db
        .query_row(
            &format!("SELECT {TASK_COLS} FROM tasks WHERE uid = ?"),
            [uid],
            task_row,
        )
        .optional()?)
}

/// Raw row state of a task or event, as sync needs it.
pub struct RawObject {
    pub kind: Kind,
    pub uid: String,
    pub list_href: String,
    pub href: Option<String>,
    pub etag: Option<String>,
    pub raw_ics: String,
    pub dirty: i64,
}

fn raw_row(r: &rusqlite::Row, kind: Kind) -> rusqlite::Result<RawObject> {
    Ok(RawObject {
        kind,
        uid: r.get(0)?,
        list_href: r.get(1)?,
        href: r.get(2)?,
        etag: r.get(3)?,
        raw_ics: r.get(4)?,
        dirty: r.get(5)?,
    })
}

const RAW_COLS: &str = "uid, list_href, href, etag, raw_ics, dirty";

/// A task by UID (`list_href` is not needed), or an event by calendar and UID.
pub fn raw(db: &Connection, kind: Kind, list_href: &str, uid: &str) -> Result<Option<RawObject>> {
    let sql = format!(
        "SELECT {RAW_COLS} FROM {} WHERE {}",
        kind.table(),
        kind.key()
    );
    Ok(db
        .query_row(&sql, [list_href, uid], |r| raw_row(r, kind))
        .optional()?)
}

pub fn raw_by_href(db: &Connection, href: &str) -> Result<Option<RawObject>> {
    for kind in [Kind::Task, Kind::Event] {
        let sql = format!("SELECT {RAW_COLS} FROM {} WHERE href = ?", kind.table());
        if let Some(o) = db
            .query_row(&sql, [href], |r| raw_row(r, kind))
            .optional()?
        {
            return Ok(Some(o));
        }
    }
    Ok(None)
}

/// Clean objects that the server has, in one calendar.
pub fn synced(db: &Connection, list_href: &str) -> Result<Vec<RawObject>> {
    let mut out = Vec::new();
    for kind in [Kind::Task, Kind::Event] {
        let sql = format!(
            "SELECT {RAW_COLS} FROM {} WHERE list_href = ? AND href IS NOT NULL",
            kind.table()
        );
        let rows = db
            .prepare(&sql)?
            .query_map([list_href], |r| raw_row(r, kind))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(rows);
    }
    Ok(out)
}

pub fn dirty(db: &Connection) -> Result<Vec<RawObject>> {
    let mut out = Vec::new();
    for kind in [Kind::Task, Kind::Event] {
        let sql = format!("SELECT {RAW_COLS} FROM {} WHERE dirty != 0", kind.table());
        let rows = db
            .prepare(&sql)?
            .query_map([], |r| raw_row(r, kind))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(rows);
    }
    Ok(out)
}

/// Insert or replace a task or event from its calendar data, recomputing every derived column.
/// Returns its UID.
pub fn save(
    db: &Connection,
    list_href: &str,
    href: Option<&str>,
    etag: Option<&str>,
    cal: &Calendar,
    dirty: i64,
) -> Result<String> {
    if Kind::of(cal) == Kind::Event {
        return save_event(db, list_href, href, etag, cal, dirty);
    }
    let uid = cal.text("UID").unwrap_or_default();
    let status = cal.text("STATUS").unwrap_or_else(|| "NEEDS-ACTION".into());
    let due = local_time(db, cal, cal.prop("DUE"))?;
    let start = local_time(db, cal, cal.prop("DTSTART"))?;
    let alarms = alarm_times(db, cal, due.map(|(d, _)| d))?;
    db.execute(
        "INSERT OR REPLACE INTO tasks (uid, list_href, etag, href, raw_ics, summary, description, status,
           completed_at, category, parent_uid, priority, due, sort_order, created, last_modified, dirty,
           due_all_day, start, start_all_day, event_uid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
        params![
            uid,
            list_href,
            etag,
            href,
            cal.to_ics(),
            cal.text("SUMMARY").unwrap_or_default(),
            cal.text("DESCRIPTION"),
            status,
            cal.time("COMPLETED"),
            cal.categories().into_iter().next(),
            cal.parent_uid(),
            cal.text("PRIORITY").and_then(|p| p.parse::<i64>().ok()).filter(|&p| p != 0), // 0 = undefined
            due.map(|(d, _)| d),
            cal.text("X-APPLE-SORT-ORDER")
                .or_else(|| cal.text("X-TODAV-ORDER"))
                .and_then(|p| p.parse::<i64>().ok()),
            cal.time("CREATED"),
            cal.time("LAST-MODIFIED"),
            dirty,
            due.is_some_and(|(_, all_day)| all_day),
            start.map(|(s, _)| s),
            start.is_some_and(|(_, all_day)| all_day),
            cal.related(EVENT_LINK),
        ],
    )?;
    db.execute("DELETE FROM alarms WHERE uid = ?", [&uid])?;
    for at in alarms {
        db.execute(
            "INSERT INTO alarms (uid, at) VALUES (?, ?)",
            params![uid, at],
        )?;
    }
    Ok(uid)
}

fn save_event(
    db: &Connection,
    list_href: &str,
    href: Option<&str>,
    etag: Option<&str>,
    cal: &Calendar,
    dirty: i64,
) -> Result<String> {
    let uid = cal.text("UID").unwrap_or_default();
    let (first, last) = event::span(db, cal)?;
    db.execute(
        "INSERT OR REPLACE INTO events (list_href, uid, etag, href, raw_ics, summary, first_start, last_end,
           last_modified, dirty)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            list_href,
            uid,
            etag,
            href,
            cal.to_ics(),
            cal.text("SUMMARY").unwrap_or_default(),
            first,
            last,
            cal.time("LAST-MODIFIED"),
            dirty,
        ],
    )?;
    Ok(uid)
}

/// Occurrences of every event overlapping `[from, to)` (local times), soonest first.
pub fn events(db: &Connection, from: i64, to: i64) -> Result<Vec<Event>> {
    // A day of slack: the stored span is in the zone the machine had when it was saved.
    let rows: Vec<(String, String)> = db
        .prepare(
            "SELECT list_href, raw_ics FROM events WHERE dirty != 2
               AND first_start < ?2 + 86400 AND (last_end IS NULL OR last_end > ?1 - 86400)",
        )?
        .query_map([from, to], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::new();
    for (list, ics) in rows {
        if let Some(cal) = Calendar::parse(&ics) {
            out.extend(event::occurrences(db, &cal, &list, from, to)?);
        }
    }
    out.sort_by(|a, b| (a.start, a.end, &a.summary).cmp(&(b.start, b.end, &b.summary)));
    Ok(out)
}

/// The event with this UID as of `from`: its next occurrence within a year, else its first.
pub fn event_by_uid(db: &Connection, uid: &str, from: i64) -> Result<Option<Event>> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT list_href, raw_ics FROM events WHERE uid = ? AND dirty != 2 LIMIT 1",
            [uid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((list, ics)) = row else {
        return Ok(None);
    };
    let Some(cal) = Calendar::parse(&ics) else {
        return Ok(None);
    };
    let next = event::occurrences(db, &cal, &list, from, from + 366 * 86400)?;
    match next.into_iter().next() {
        Some(e) => Ok(Some(e)),
        None => Ok(
            event::occurrences(db, &cal, &list, i64::MIN / 4, i64::MAX / 4)?
                .into_iter()
                .next(),
        ),
    }
}

/// A VTIMEZONE for `tzid` from any stored event, to write new times in that zone.
pub fn vtimezone(db: &Connection, tzid: &str) -> Result<Option<Vec<Line>>> {
    let rows: Vec<String> = db
        .prepare("SELECT raw_ics FROM events WHERE instr(raw_ics, ?1) > 0 LIMIT 20")?
        .query_map([format!("TZID:{tzid}")], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for ics in rows {
        let Some(cal) = Calendar::parse(&ics) else {
            continue;
        };
        let comps = cal.comps();
        let found = comps.iter().position(|c| {
            c.kind.eq_ignore_ascii_case("VTIMEZONE") && c.text("TZID").as_deref() == Some(tzid)
        });
        if let Some(i) = found {
            return Ok(Some(cal.comp_lines(i)));
        }
    }
    Ok(None)
}

pub fn set_sync_state(
    db: &Connection,
    o: &RawObject,
    href: Option<&str>,
    etag: Option<&str>,
    dirty: i64,
) -> Result<()> {
    let sql = format!(
        "UPDATE {} SET href = ?3, etag = ?4, dirty = ?5 WHERE {}",
        o.kind.table(),
        o.kind.key()
    );
    db.execute(&sql, params![o.list_href, o.uid, href, etag, dirty])?;
    Ok(())
}

pub fn delete(db: &Connection, o: &RawObject) -> Result<()> {
    let sql = format!("DELETE FROM {} WHERE {}", o.kind.table(), o.kind.key());
    db.execute(&sql, [&o.list_href, &o.uid])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoned_due_times_go_through_their_vtimezone() {
        let db = open(":memory:").unwrap();
        db.execute(
            "INSERT INTO lists (href, display_name) VALUES ('/l/', 'L')",
            [],
        )
        .unwrap();
        // 09:00 in New York (UTC-4 in October) is 13:00 UTC, whatever zone this machine is in.
        let cal = Calendar::parse("BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:America/New_York\r\nBEGIN:DAYLIGHT\r\nTZOFFSETFROM:-0500\r\nTZOFFSETTO:-0400\r\nDTSTART:20070311T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU\r\nEND:DAYLIGHT\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:-0400\r\nTZOFFSETTO:-0500\r\nDTSTART:20071104T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=1SU\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VTODO\r\nUID:u\r\nSUMMARY:s\r\nDUE;TZID=America/New_York:20261020T090000\r\nEND:VTODO\r\nEND:VCALENDAR\r\n").unwrap();
        save(&db, "/l/", None, None, &cal, CLEAN).unwrap();
        let utc = ical::parse_time("20261020T130000").unwrap();
        let t = task(&db, "u").unwrap().unwrap();
        assert_eq!(
            (t.due, t.due_all_day),
            (Some(to_local(&db, utc).unwrap()), false)
        );
    }
}
