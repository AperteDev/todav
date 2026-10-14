//! SQLite schema and row mapping. Every derived column is recomputed from `raw_ics`.

use crate::ical::{self, Calendar, Line};
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
];

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
    if version == 1 {
        rederive(&db)?; // `due` became local time; all-day and reminders are new
    }
    Ok(db)
}

/// Recompute every task's derived columns from its calendar data.
fn rederive(db: &Connection) -> Result<()> {
    let all: Vec<RawTask> = db
        .prepare(&format!("SELECT {RAW_COLS} FROM tasks"))?
        .query_map([], raw_row)?
        .collect::<rusqlite::Result<_>>()?;
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

/// A DATE / DATE-TIME value as local time, and whether it is a date only.
/// ponytail: TZID times are taken as local; off by the difference for another zone than the system's.
fn local_time(db: &Connection, l: Option<&Line>) -> Result<Option<(i64, bool)>> {
    let Some((l, t)) = l.and_then(|l| Some((l, ical::parse_time(&l.value)?))) else {
        return Ok(None);
    };
    Ok(Some(if l.value.ends_with('Z') {
        (to_local(db, t)?, false)
    } else {
        (t, l.value.len() == 8)
    }))
}

/// When the first alarm goes off, in local time: an absolute TRIGGER, or one relative to DUE
/// (RELATED=END) or DTSTART (the default; DUE when there is no DTSTART).
fn remind_at(db: &Connection, cal: &Calendar, due: Option<i64>) -> Result<Option<i64>> {
    let Some(t) = cal.alarm_trigger() else {
        return Ok(None);
    };
    if let Some(d) = ical::parse_duration(&t.value) {
        let end = t
            .param("RELATED")
            .is_some_and(|r| r.eq_ignore_ascii_case("END"));
        let start = local_time(db, cal.prop("DTSTART"))?.map(|(s, _)| s);
        let base = if end { due } else { start.or(due) };
        return Ok(base.map(|b| b + d));
    }
    Ok(local_time(db, Some(t))?.map(|(t, _)| t))
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
              AND t.status NOT IN ('COMPLETED', 'CANCELLED'))
         FROM lists l ORDER BY l.display_name COLLATE NOCASE",
    )?;
    let rows = st.query_map([], |r| {
        Ok(List {
            href: r.get(0)?,
            display_name: r.get(1)?,
            color: r.get(2)?,
            open_count: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

const TASK_COLS: &str = "uid, list_href, summary, description, status, completed_at, category, parent_uid, priority, due, due_all_day, remind_at, sort_order, created, last_modified";

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
        remind_at: r.get(11)?,
        sort_order: r.get(12)?,
        created: r.get(13)?,
        last_modified: r.get(14)?,
    })
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

/// Open tasks whose reminder goes off after `from` and at `to` at the latest (local times).
pub fn reminders(db: &Connection, from: i64, to: i64) -> Result<Vec<Task>> {
    let mut st = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE dirty != 2 AND remind_at > ?1 AND remind_at <= ?2
           AND status NOT IN ('COMPLETED', 'CANCELLED')
         ORDER BY remind_at"
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

/// Raw row state needed by sync: (list_href, href, etag, raw_ics, dirty).
pub struct RawTask {
    pub uid: String,
    pub list_href: String,
    pub href: Option<String>,
    pub etag: Option<String>,
    pub raw_ics: String,
    pub dirty: i64,
}

fn raw_row(r: &rusqlite::Row) -> rusqlite::Result<RawTask> {
    Ok(RawTask {
        uid: r.get(0)?,
        list_href: r.get(1)?,
        href: r.get(2)?,
        etag: r.get(3)?,
        raw_ics: r.get(4)?,
        dirty: r.get(5)?,
    })
}

const RAW_COLS: &str = "uid, list_href, href, etag, raw_ics, dirty";

pub fn raw(db: &Connection, uid: &str) -> Result<Option<RawTask>> {
    Ok(db
        .query_row(
            &format!("SELECT {RAW_COLS} FROM tasks WHERE uid = ?"),
            [uid],
            raw_row,
        )
        .optional()?)
}

pub fn raw_by_href(db: &Connection, href: &str) -> Result<Option<RawTask>> {
    Ok(db
        .query_row(
            &format!("SELECT {RAW_COLS} FROM tasks WHERE href = ?"),
            [href],
            raw_row,
        )
        .optional()?)
}

pub fn dirty(db: &Connection) -> Result<Vec<RawTask>> {
    let mut st = db.prepare(&format!("SELECT {RAW_COLS} FROM tasks WHERE dirty != 0"))?;
    let rows = st.query_map([], raw_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Insert or replace a task from its calendar data, recomputing every derived column.
pub fn save(
    db: &Connection,
    list_href: &str,
    href: Option<&str>,
    etag: Option<&str>,
    cal: &Calendar,
    dirty: i64,
) -> Result<String> {
    let uid = cal.text("UID").unwrap_or_default();
    let status = cal.text("STATUS").unwrap_or_else(|| "NEEDS-ACTION".into());
    let due = local_time(db, cal.prop("DUE"))?;
    let remind = remind_at(db, cal, due.map(|(d, _)| d))?;
    db.execute(
        "INSERT OR REPLACE INTO tasks (uid, list_href, etag, href, raw_ics, summary, description, status,
           completed_at, category, parent_uid, priority, due, sort_order, created, last_modified, dirty,
           due_all_day, remind_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
            remind,
        ],
    )?;
    Ok(uid)
}

pub fn set_sync_state(
    db: &Connection,
    uid: &str,
    href: Option<&str>,
    etag: Option<&str>,
    dirty: i64,
) -> Result<()> {
    db.execute(
        "UPDATE tasks SET href = ?2, etag = ?3, dirty = ?4 WHERE uid = ?1",
        params![uid, href, etag, dirty],
    )?;
    Ok(())
}

pub fn delete(db: &Connection, uid: &str) -> Result<()> {
    db.execute("DELETE FROM tasks WHERE uid = ?", [uid])?;
    Ok(())
}
