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
    // The ntfy server syncs now: an empty one saved when signing in must not clear the one
    // other devices use.
    r#"
DELETE FROM kv WHERE key IN ('setting_ntfy_url', 'settime_ntfy_url')
  AND (SELECT value FROM kv WHERE key = 'setting_ntfy_url') = '';
"#,
    // The server refused this version (`refused_ics`) for `sync_error`: it is not sent again
    // until edited, and the rest syncs on.
    r#"
ALTER TABLE tasks ADD COLUMN sync_error TEXT;
ALTER TABLE tasks ADD COLUMN refused_ics TEXT;
ALTER TABLE events ADD COLUMN sync_error TEXT;
ALTER TABLE events ADD COLUMN refused_ics TEXT;
"#,
    // Tasks moved to another list leave their old server copy here until the next push deletes
    // it: tasks are keyed by UID, so the row itself went with the task.
    r#"
CREATE TABLE moved (
  href TEXT PRIMARY KEY, etag TEXT, list_href TEXT NOT NULL, uid TEXT NOT NULL
);
"#,
    // X-PINNED (Nextcloud Tasks): pinned tasks come first in their list.
    r#"
ALTER TABLE tasks ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
"#,
    // Several accounts, each with its lists (the data moves in `one_account_to_many`).
    r#"
CREATE TABLE accounts (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,                   -- 'nextcloud' or 'local'
  name TEXT NOT NULL DEFAULT '',        -- empty: the front-end names it
  url TEXT NOT NULL DEFAULT '',
  user TEXT NOT NULL DEFAULT '',
  source TEXT NOT NULL DEFAULT '',      -- the front-end's, see `Account::source`
  enabled INTEGER NOT NULL DEFAULT 1
);
ALTER TABLE lists ADD COLUMN account INTEGER REFERENCES accounts(id) ON DELETE CASCADE;
"#,
    // The user's order of the accounts (and so of their lists); in the order added until moved.
    r#"
ALTER TABLE accounts ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
UPDATE accounts SET position = id;
"#,
    // The user's order of the lists, within their account: the server's calendar-order (0 until
    // set), or for lists on this device only ours; `position_dirty`: a new one waits for the
    // server.
    r#"
ALTER TABLE lists ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
ALTER TABLE lists ADD COLUMN position_dirty INTEGER NOT NULL DEFAULT 0;
"#,
];

/// `user_version` after the migration that adds accounts.
const ACCOUNTS: usize = 10;

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

/// How the href of a list kept on this device only starts; it has no server copy, and what is
/// in it never enters the upload queue (`dirty`).
pub const LOCAL: &str = "local:";

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
        if i + 1 == ACCOUNTS {
            one_account_to_many(&tx)?;
        }
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    // Derived columns changed with most migrations so far (local times, reminders, zones, start);
    // after the others rederiving is only wasted time.
    if version > 0 && (version as usize) < MIGRATIONS.len() {
        rederive(&db)?;
    }
    Ok(db)
}

/// Todav 1.2 kept one account in `kv` (`account_url`, `account_user`, its GNOME Online Accounts
/// id in the `goa_account` setting) and the lists on this device behind the `local_mode`
/// setting. Each becomes an account; server lists are keyed by their full URL from now on, so
/// two servers' lists with the same path stay apart.
fn one_account_to_many(tx: &Connection) -> Result<()> {
    tx.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    let setting = |k: &str| kv_get(tx, &format!("setting_{k}"));
    let account = (kv_get(tx, "account_url")?, kv_get(tx, "account_user")?);
    if let (Some(url), Some(user)) = account {
        let goa = setting("goa_account")?.unwrap_or_default();
        tx.execute(
            "INSERT INTO accounts (kind, url, user, source) VALUES ('nextcloud', ?1, ?2, ?3)",
            params![url, user, goa],
        )?;
        let id = tx.last_insert_rowid();
        let origin = crate::caldav::origin(&url);
        let not_local = format!("NOT LIKE '{LOCAL}%'");
        tx.execute(
            &format!("UPDATE lists SET account = ?1, href = ?2 || href WHERE href {not_local}"),
            params![id, origin],
        )?;
        for table in ["tasks", "events", "moved"] {
            tx.execute(
                &format!(
                    "UPDATE {table} SET list_href = ?1 || list_href WHERE list_href {not_local}"
                ),
                [&origin],
            )?;
        }
        if let Some(last) = setting("last_list")?.filter(|l| l.starts_with('/')) {
            kv_set(tx, "setting_last_list", &format!("{origin}{last}"))?;
        }
        // Per account now: the category file, the settings file's cache, the last sync.
        tx.execute(
            "UPDATE kv SET key = 'a' || ?1 || '_' || key
             WHERE key LIKE 'config\\_%' ESCAPE '\\' OR key = 'last_sync'",
            [id],
        )?;
        let synced = setting("sync_settings")?.as_deref() != Some("false");
        let to = if synced {
            id.to_string()
        } else {
            String::new()
        };
        kv_set(tx, "setting_settings_account", &to)?;
    }
    let local_mode = setting("local_mode")?.as_deref() == Some("true");
    let local_lists: bool = tx.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM lists WHERE href LIKE '{LOCAL}%')"),
        [],
        |r| r.get(0),
    )?;
    // Hidden lists stay hidden: the account is there, switched off.
    if local_mode || local_lists {
        tx.execute(
            "INSERT INTO accounts (kind, enabled) VALUES ('local', ?1)",
            [local_mode],
        )?;
        tx.execute(
            &format!("UPDATE lists SET account = ?1 WHERE href LIKE '{LOCAL}%'"),
            [tx.last_insert_rowid()],
        )?;
    }
    for k in ["goa_account", "local_mode", "sync_settings"] {
        tx.execute(
            "DELETE FROM kv WHERE key IN ('setting_' || ?1, 'settime_' || ?1)",
            [k],
        )?;
    }
    tx.execute(
        "DELETE FROM kv WHERE key IN ('account_url', 'account_user')",
        [],
    )?;
    Ok(())
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
pub(crate) fn local_time(
    db: &Connection,
    cal: &Calendar,
    l: Option<&Line>,
) -> Result<Option<(i64, bool)>> {
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

/// Every list and calendar, in the user's order (`Client::reorder_lists`), then by name; each
/// server one keyed by its full URL (`Dav::absolute`).
pub fn lists(db: &Connection) -> Result<Vec<List>> {
    let mut st = db.prepare(
        "SELECT l.href, l.display_name, l.color,
           (SELECT COUNT(*) FROM tasks t WHERE t.list_href = l.href AND t.dirty != 2
              AND t.status NOT IN ('COMPLETED', 'CANCELLED')),
           l.has_tasks, l.has_events, l.writable, COALESCE(l.account, 0)
         FROM lists l ORDER BY l.position, l.display_name COLLATE NOCASE",
    )?;
    let rows = st.query_map([], |r| {
        let href: String = r.get(0)?;
        Ok(List {
            local: href.starts_with(LOCAL),
            href,
            display_name: r.get(1)?,
            color: r.get(2)?,
            open_count: r.get(3)?,
            holds_tasks: r.get(4)?,
            holds_events: r.get(5)?,
            writable: r.get(6)?,
            account: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The account list `href` belongs to (0: none, as in tests).
pub fn list_account(db: &Connection, href: &str) -> Result<i64> {
    Ok(db
        .query_row(
            "SELECT COALESCE(account, 0) FROM lists WHERE href = ?",
            [href],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

/// Tasks are keyed by UID: one in a calendar two accounts share shows once, in the account
/// that had it first. Whether task `uid` is in another account's list than `list_href`.
/// TODO(v2): key tasks by list and UID, as events are.
pub fn task_elsewhere(db: &Connection, uid: &str, list_href: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS (SELECT 1 FROM tasks t JOIN lists l ON l.href = t.list_href
           WHERE t.uid = ?1 AND t.list_href != ?2
             AND l.account IS NOT (SELECT account FROM lists WHERE href = ?2))",
        [uid, list_href],
        |r| r.get(0),
    )?)
}

/// Lists of switched-off accounts: hidden, not synced.
pub fn hidden(db: &Connection) -> Result<std::collections::HashSet<String>> {
    let mut st = db.prepare(
        "SELECT l.href FROM lists l JOIN accounts a ON a.id = l.account WHERE NOT a.enabled",
    )?;
    let rows = st.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

const TASK_COLS: &str = "uid, list_href, summary, description, status, completed_at, category, parent_uid, priority, due, due_all_day,
  (SELECT group_concat(at) FROM (SELECT at FROM alarms a WHERE a.uid = tasks.uid ORDER BY at)),
  sort_order, created, last_modified, start, start_all_day, event_uid,
  CASE WHEN refused_ics IS raw_ics THEN sync_error END, pinned";

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
        sync_error: r.get(18)?,
        pinned: r.get(19)?,
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

/// Open tasks of high priority (1 to 4, what Nextcloud Tasks stars) in every list, highest
/// first.
pub fn important(db: &Connection) -> Result<Vec<Task>> {
    let mut st = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE dirty != 2 AND priority BETWEEN 1 AND 4
           AND status NOT IN ('COMPLETED', 'CANCELLED')
         ORDER BY priority, COALESCE(sort_order, created, 0), created, uid"
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
/// (overdue included); dated ones first, soonest first. A Google task takes no slot: a date is
/// all its plan.
pub fn unscheduled(db: &Connection, until: i64) -> Result<Vec<Task>> {
    let mut st = db.prepare(&format!(
        "SELECT {TASK_COLS} FROM tasks WHERE dirty != 2
           AND status NOT IN ('COMPLETED', 'CANCELLED')
           AND (start IS NULL OR start_all_day)
           AND (due IS NULL OR (due_all_day AND due < ?1 AND substr(list_href, 1, length(?2)) != ?2))
         ORDER BY due IS NULL, due, list_href, COALESCE(sort_order, created, 0), uid"
    ))?;
    let rows = st.query_map(params![until, crate::gtasks::API], task_row)?;
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

/// The object at `href` (a server path) in list `list_href`: another account's server may
/// have one at the same path.
pub fn raw_by_href(db: &Connection, list_href: &str, href: &str) -> Result<Option<RawObject>> {
    for kind in [Kind::Task, Kind::Event] {
        let sql = format!(
            "SELECT {RAW_COLS} FROM {} WHERE list_href = ?1 AND href = ?2",
            kind.table()
        );
        if let Some(o) = db
            .query_row(&sql, [list_href, href], |r| raw_row(r, kind))
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

/// What waits for the server in the lists of `account` (None: of every switched-on account).
pub fn dirty(db: &Connection, account: Option<i64>) -> Result<Vec<RawObject>> {
    let mut out = Vec::new();
    for kind in [Kind::Task, Kind::Event] {
        // Not a version the server refused: it would refuse it again.
        let sql = format!(
            "SELECT {RAW_COLS} FROM {} WHERE dirty != 0 AND refused_ics IS NOT raw_ics
               AND list_href NOT LIKE '{LOCAL}%'
               AND list_href IN (SELECT l.href FROM lists l LEFT JOIN accounts a ON a.id = l.account
                 WHERE COALESCE(a.enabled, 1) AND (?1 IS NULL OR l.account = ?1))",
            kind.table()
        );
        let rows = db
            .prepare(&sql)?
            .query_map([account], |r| raw_row(r, kind))?
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
           due_all_day, start, start_all_day, event_uid, pinned)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
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
            cal.text("X-PINNED").as_deref() == Some("true"),
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
    // In step with the server again: any refusal is over.
    let sql = format!(
        "UPDATE {} SET href = ?3, etag = ?4, dirty = ?5, sync_error = NULL, refused_ics = NULL
         WHERE {}",
        o.kind.table(),
        o.kind.key()
    );
    db.execute(&sql, params![o.list_href, o.uid, href, etag, dirty])?;
    Ok(())
}

/// The server refused `o` as it is now, saying `error`; returns its title. It is not sent again
/// until edited.
pub fn refuse(db: &Connection, o: &RawObject, error: &str) -> Result<String> {
    let (table, key) = (o.kind.table(), o.kind.key());
    db.execute(
        &format!("UPDATE {table} SET sync_error = ?3, refused_ics = raw_ics WHERE {key}"),
        params![o.list_href, o.uid, error],
    )?;
    Ok(db.query_row(
        &format!("SELECT summary FROM {table} WHERE {key}"),
        params![o.list_href, o.uid],
        |r| r.get(0),
    )?)
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
    fn one_account_and_local_lists_become_accounts() {
        let path = std::env::temp_dir().join(format!("todav-test-migrate-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let path = path.to_string_lossy().to_string();
        {
            // As Todav 1.2 left it.
            let db = Connection::open(&path).unwrap();
            for m in &MIGRATIONS[..ACCOUNTS - 1] {
                db.execute_batch(m).unwrap();
            }
            db.pragma_update(None, "user_version", ACCOUNTS as i64 - 1)
                .unwrap();
            db.execute_batch(
                "INSERT INTO kv (key, value) VALUES ('account_url', 'https://cloud.example/nc'),
                   ('account_user', 'kris'), ('setting_goa_account', 'account_1'),
                   ('setting_local_mode', 'false'), ('setting_last_list', '/dav/l/'),
                   ('config_json', '{}'), ('last_sync', '5'), ('push_private', 'k');
                 INSERT INTO lists (href, display_name) VALUES ('/dav/l/', 'L'), ('local:x/', 'X');
                 INSERT INTO tasks (uid, list_href, raw_ics, summary, status)
                   VALUES ('u', '/dav/l/', 'BEGIN:VCALENDAR
BEGIN:VTODO
UID:u
SUMMARY:s
END:VTODO
END:VCALENDAR
', 's', 'NEEDS-ACTION');",
            )
            .unwrap();
        }
        let db = open(&path).unwrap();
        let accounts: Vec<(i64, String, String, String, bool)> = db
            .prepare("SELECT id, kind, url, source, enabled FROM accounts ORDER BY id")
            .unwrap()
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(
            accounts,
            [
                (
                    1,
                    "nextcloud".into(),
                    "https://cloud.example/nc".into(),
                    "account_1".into(),
                    true
                ),
                (2, "local".into(), "".into(), "".into(), false),
            ],
            "local lists that were hidden stay hidden"
        );
        let lists = lists(&db).unwrap();
        let keys: Vec<(&str, i64)> = lists.iter().map(|l| (l.href.as_str(), l.account)).collect();
        assert_eq!(keys, [("https://cloud.example/dav/l/", 1), ("local:x/", 2)]);
        assert_eq!(
            task(&db, "u").unwrap().unwrap().list_href,
            "https://cloud.example/dav/l/"
        );
        let kv = |k: &str| kv_get(&db, k).unwrap();
        assert_eq!(
            kv("setting_last_list").as_deref(),
            Some("https://cloud.example/dav/l/")
        );
        assert_eq!(kv("setting_settings_account").as_deref(), Some("1"));
        assert_eq!(kv("a1_config_json").as_deref(), Some("{}"));
        assert_eq!(kv("a1_last_sync").as_deref(), Some("5"));
        assert_eq!(kv("push_private").as_deref(), Some("k"), "device keys stay");
        for gone in [
            "account_url",
            "setting_goa_account",
            "setting_local_mode",
            "config_json",
        ] {
            assert_eq!(kv(gone), None, "{gone}");
        }
    }

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
