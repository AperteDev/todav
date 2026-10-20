pub mod caldav;
mod config;
pub mod ical;
pub mod json;
mod login;
pub mod push;
pub mod rrule;
mod settings;
pub mod store;
mod sync;
pub mod tz;

use caldav::Dav;
use ical::Calendar;
use rusqlite::Connection;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub use config::{CategoryGroup, CategoryMeta, DoneView, Sort};
pub use login::{Credentials, LoginFlow, login_flow_poll, login_flow_start, password_login};
pub use push::{PushRegistration, ntfy_check};
pub use settings::DEVICE_ONLY;
pub use sync::SyncReport;

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub href: String,
    pub display_name: String,
    pub color: Option<String>,
    pub open_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub uid: String,
    pub list_href: String,
    pub summary: String,
    pub description: Option<String>,
    pub status: String,
    pub done: bool,
    pub completed_at: Option<i64>,
    pub category: Option<String>,
    pub parent_uid: Option<String>,
    pub priority: Option<i64>,
    /// Local time (see `store::to_local`); midnight for a date without a time.
    pub due: Option<i64>,
    pub due_all_day: bool,
    /// When the reminders go off, in local time, soonest first.
    pub reminders: Vec<i64>,
    pub sort_order: Option<i64>,
    pub created: Option<i64>,
    pub last_modified: Option<i64>,
}

/// Fields left `None` are untouched; `Some("")` clears description/category.
#[derive(Debug, Clone, Default)]
pub struct TaskPatch {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    /// Some(None) clears the due date, and its reminders with it.
    pub due: Option<Option<Due>>,
    /// Reminders, in seconds after the due time (negative: before); replaces all of them.
    pub reminders: Option<Vec<i64>>,
}

/// A due date in local time; `all_day`: a date without a time (`at` is its midnight).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Due {
    pub at: i64,
    pub all_day: bool,
}

#[derive(Debug, Clone)]
pub struct Account {
    pub url: String,
    pub user: String,
}

pub trait ChangeListener: Send + Sync {
    fn changed(&self, list_href: String, uids: Vec<String>);
    /// Settings changed by another device arrived.
    fn settings_changed(&self) {}
}

pub struct Client {
    db: Mutex<Connection>,
    dav: Mutex<Option<Arc<Dav>>>,
    listeners: Mutex<Vec<Box<dyn ChangeListener>>>,
    sync_lock: Mutex<()>,
    /// Bumped by `stop_listening`; a `listen` call returns once it differs from the value it started with.
    listen_gen: AtomicU64,
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    use ring::rand::SecureRandom;
    let mut b = [0u8; N];
    ring::rand::SystemRandom::new()
        .fill(&mut b)
        .expect("system rng");
    b
}

fn new_uid() -> String {
    let mut b = random_bytes::<16>();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

impl Client {
    pub fn open(data_dir: String) -> Result<Client> {
        std::fs::create_dir_all(&data_dir).map_err(|e| Error::Db(e.to_string()))?;
        let db = store::open(&format!("{data_dir}/core.sqlite"))?;
        Ok(Client {
            db: Mutex::new(db),
            dav: Mutex::new(None),
            listeners: Mutex::new(Vec::new()),
            sync_lock: Mutex::new(()),
            listen_gen: AtomicU64::new(0),
        })
    }

    /// The password is kept in memory only; front-ends keep it in the platform keyring.
    pub fn set_account(&self, url: String, user: String, app_password: String) -> Result<()> {
        let db = self.db.lock().unwrap();
        if store::kv_get(&db, "account_url")?.as_deref() != Some(&url)
            || store::kv_get(&db, "account_user")?.as_deref() != Some(&user)
        {
            // Different account: local data belongs to the old one.
            db.execute_batch(
                "DELETE FROM tasks; DELETE FROM lists;
                 DELETE FROM kv WHERE key LIKE 'config_%' OR key IN ('push_resource', 'conflict_log');",
            )?;
        }
        store::kv_set(&db, "account_url", &url)?;
        store::kv_set(&db, "account_user", &user)?;
        *self.dav.lock().unwrap() = Some(Arc::new(Dav::new(&url, &user, &app_password)));
        Ok(())
    }

    pub fn account(&self) -> Option<Account> {
        let db = self.db.lock().unwrap();
        Some(Account {
            url: store::kv_get(&db, "account_url").ok()??,
            user: store::kv_get(&db, "account_user").ok()??,
        })
    }

    /// Forget the account and all local data (keeps device keys).
    pub fn logout(&self) -> Result<()> {
        let _ = self.push_unregister();
        let db = self.db.lock().unwrap();
        db.execute_batch(
            "DELETE FROM tasks; DELETE FROM lists;
             DELETE FROM kv WHERE key LIKE 'config_%' OR key LIKE 'account_%' OR key IN ('push_resource', 'conflict_log');",
        )?;
        *self.dav.lock().unwrap() = None;
        Ok(())
    }

    fn dav(&self) -> Result<Arc<Dav>> {
        self.dav.lock().unwrap().clone().ok_or(Error::NoAccount)
    }

    pub fn lists(&self) -> Vec<List> {
        store::lists(&self.db.lock().unwrap()).unwrap_or_default()
    }

    pub fn tasks(&self, list_href: String, include_done: bool) -> Vec<Task> {
        store::tasks(&self.db.lock().unwrap(), &list_href, include_done).unwrap_or_default()
    }

    pub fn task(&self, uid: String) -> Option<Task> {
        store::task(&self.db.lock().unwrap(), &uid).ok()?
    }

    /// Open tasks with a due date in every list, soonest first.
    pub fn upcoming(&self) -> Vec<Task> {
        store::upcoming(&self.db.lock().unwrap()).unwrap_or_default()
    }

    /// Open tasks with a reminder going off after `from` and at `to` at the latest (local times).
    pub fn reminders(&self, from: i64, to: i64) -> Vec<Task> {
        store::reminders(&self.db.lock().unwrap(), from, to).unwrap_or_default()
    }

    /// Now in local time, as task dates are.
    pub fn local_now(&self) -> i64 {
        store::to_local(&self.db.lock().unwrap(), now()).unwrap_or_else(|_| now())
    }

    pub fn subscribe(&self, listener: Box<dyn ChangeListener>) {
        self.listeners.lock().unwrap().push(listener);
    }

    fn emit(&self, list_href: &str, uids: Vec<String>) {
        for l in self.listeners.lock().unwrap().iter() {
            l.changed(list_href.to_string(), uids.clone());
        }
    }

    pub fn add_task(
        &self,
        list_href: String,
        summary: String,
        category: Option<String>,
        parent_uid: Option<String>,
        at_start: bool,
    ) -> Result<Task> {
        let uid = new_uid();
        let t = now();
        let mut cal = Calendar::new_todo(&uid, t);
        cal.set_text("SUMMARY", Some(&summary));
        cal.set("STATUS", "", Some("NEEDS-ACTION"));
        cal.set_text("CATEGORIES", category.as_deref().filter(|c| !c.is_empty()));
        if let Some(p) = &parent_uid {
            cal.set("RELATED-TO", ";RELTYPE=PARENT", Some(&ical::escape(p)));
        }
        cal.set_time("LAST-MODIFIED", Some(t));
        {
            let db = self.db.lock().unwrap();
            // Before or after every task in the list, so also first or last among its siblings.
            let sql = if at_start {
                "SELECT COALESCE(MIN(sort_order), 0) - 1 FROM tasks WHERE list_href = ?"
            } else {
                "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tasks WHERE list_href = ?"
            };
            let next: i64 = db.query_row(sql, [&list_href], |r| r.get(0))?;
            set_order(&mut cal, next);
            store::save(&db, &list_href, None, None, &cal, store::MODIFIED)?;
        }
        self.emit(&list_href, vec![uid.clone()]);
        self.task(uid.clone()).ok_or(Error::NotFound(uid))
    }

    /// Load, patch and save a task as locally modified.
    fn modify(&self, uid: &str, f: impl FnOnce(&mut Calendar)) -> Result<()> {
        let list = {
            let db = self.db.lock().unwrap();
            let raw = store::raw(&db, uid)?.ok_or_else(|| Error::NotFound(uid.into()))?;
            let mut cal = Calendar::parse(&raw.raw_ics).ok_or_else(|| Error::Parse(uid.into()))?;
            f(&mut cal);
            touch(&mut cal);
            store::save(
                &db,
                &raw.list_href,
                raw.href.as_deref(),
                raw.etag.as_deref(),
                &cal,
                store::MODIFIED,
            )?;
            raw.list_href
        };
        self.emit(&list, vec![uid.to_string()]);
        Ok(())
    }

    pub fn set_done(&self, uid: String, done: bool) -> Result<()> {
        self.modify(&uid, |cal| set_done(cal, done, now()))
    }

    /// Make a subtask a top-level task (its own subtasks come along).
    pub fn promote(&self, uid: String) -> Result<()> {
        self.modify(&uid, |cal| cal.clear_parent())
    }

    /// Make `uid` a subtask of `parent_uid` (its own subtasks come along).
    /// Refuses a parent inside `uid`'s own tree, which would make a loop.
    pub fn set_parent(&self, uid: String, parent_uid: String) -> Result<()> {
        if self.within(parent_uid.clone(), uid.clone()) {
            return Err(Error::Parse(format!("{parent_uid} is inside {uid}")));
        }
        self.modify(&uid, |cal| cal.set_parent(&parent_uid))
    }

    /// Whether `uid` is `ancestor` or one of its (nested) subtasks.
    pub fn within(&self, uid: String, ancestor: String) -> bool {
        let mut p = Some(uid);
        // ponytail: 64 levels deep at most, so foreign data with a parent loop cannot hang us.
        for _ in 0..64 {
            match p {
                Some(cur) if cur == ancestor => return true,
                Some(cur) => p = self.task(cur).and_then(|t| t.parent_uid),
                None => break,
            }
        }
        false
    }

    pub fn update_task(&self, uid: String, patch: TaskPatch) -> Result<()> {
        // Times go out in UTC, dates as dates.
        let due_utc = match patch.due {
            Some(Some(d)) if !d.all_day => Some(store::to_utc(&self.db.lock().unwrap(), d.at)?),
            _ => None,
        };
        self.modify(&uid, |cal| {
            match patch.due {
                Some(None) => {
                    cal.set("DUE", "", None);
                    cal.set_alarms(&[]);
                }
                Some(Some(d)) if d.all_day => {
                    cal.set("DUE", ";VALUE=DATE", Some(&ical::fmt_utc(d.at)[..8]))
                }
                Some(Some(_)) => cal.set_time("DUE", due_utc),
                None => {}
            }
            if let Some(r) = patch.reminders.as_ref().filter(|_| patch.due != Some(None)) {
                let v: Vec<String> = r.iter().map(|o| ical::fmt_duration(*o)).collect();
                let t: Vec<(&str, &str)> = v.iter().map(|v| (";RELATED=END", v.as_str())).collect();
                cal.set_alarms(&t);
            }
            if let Some(s) = &patch.summary {
                cal.set_text("SUMMARY", Some(s));
            }
            if let Some(d) = &patch.description {
                cal.set_text("DESCRIPTION", Some(d.as_str()).filter(|d| !d.is_empty()));
            }
            if let Some(c) = &patch.category {
                // Keep secondary categories; replace the first one.
                let mut cats = cal.categories();
                if cats.is_empty() {
                    cats.push(String::new());
                }
                cats[0] = c.clone();
                let v: Vec<String> = cats
                    .iter()
                    .filter(|c| !c.is_empty())
                    .map(|c| ical::escape(c))
                    .collect();
                cal.set(
                    "CATEGORIES",
                    "",
                    Some(v.join(",").as_str()).filter(|v| !v.is_empty()),
                );
            }
        })
    }

    /// Returns the task's calendar data, for `restore_task`.
    pub fn delete_task(&self, uid: String) -> Result<String> {
        let raw = {
            let db = self.db.lock().unwrap();
            let raw = store::raw(&db, &uid)?.ok_or_else(|| Error::NotFound(uid.clone()))?;
            if raw.href.is_none() {
                store::delete(&db, &uid)?; // never reached the server
            } else {
                db.execute("UPDATE tasks SET dirty = 2 WHERE uid = ?", [&uid])?;
            }
            raw
        };
        self.emit(&raw.list_href, vec![uid]);
        Ok(raw.raw_ics)
    }

    /// Undo `delete_task`: bring the task back from the data it returned. A delete the server
    /// already has is re-created there as a new resource.
    pub fn restore_task(&self, list_href: String, raw_ics: String) -> Result<()> {
        let mut cal = Calendar::parse(&raw_ics).ok_or_else(|| Error::Parse(list_href.clone()))?;
        touch(&mut cal); // newer than the deleted copy, should another device still have it
        let uid = {
            let db = self.db.lock().unwrap();
            let (href, etag) = match store::raw(&db, &cal.text("UID").unwrap_or_default())? {
                Some(r) => (r.href, r.etag), // delete not pushed yet
                None => (None, None),
            };
            store::save(
                &db,
                &list_href,
                href.as_deref(),
                etag.as_deref(),
                &cal,
                store::MODIFIED,
            )?
        };
        self.emit(&list_href, vec![uid]);
        Ok(())
    }

    /// Move `uid` right before `before_uid` (or to the end) among its siblings in the list.
    pub fn reorder(&self, uid: String, before_uid: Option<String>) -> Result<()> {
        let task = self
            .task(uid.clone())
            .ok_or_else(|| Error::NotFound(uid.clone()))?;
        let mut order: Vec<Task> = self.tasks(task.list_href.clone(), true);
        order.retain(|t| t.uid != uid);
        let pos = before_uid
            .and_then(|b| order.iter().position(|t| t.uid == b))
            .unwrap_or(order.len());
        order.insert(pos, task);
        // ponytail: renumbers the whole list (one PUT per moved task); use gaps if lists get long.
        for (i, t) in order.iter().enumerate() {
            let i = i as i64 + 1;
            if t.sort_order != Some(i) {
                self.modify(&t.uid, |cal| set_order(cal, i))?;
            }
        }
        Ok(())
    }
}

/// Nextcloud Tasks' manual order, so both apps share one order; drops our old X-TODAV-ORDER.
fn set_order(cal: &mut Calendar, order: i64) {
    cal.set("X-APPLE-SORT-ORDER", "", Some(&order.to_string()));
    cal.set("X-TODAV-ORDER", "", None);
}

/// Bump LAST-MODIFIED, DTSTAMP and SEQUENCE after a local edit.
fn touch(cal: &mut Calendar) {
    let t = now();
    cal.set_time("LAST-MODIFIED", Some(t));
    cal.set_time("DTSTAMP", Some(t));
    let seq = cal
        .text("SEQUENCE")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    cal.set("SEQUENCE", "", Some(&(seq + 1).to_string()));
}

fn set_done(cal: &mut Calendar, done: bool, t: i64) {
    if done {
        cal.set("STATUS", "", Some("COMPLETED"));
        cal.set_time("COMPLETED", Some(t));
        cal.set("PERCENT-COMPLETE", "", Some("100"));
    } else {
        cal.set("STATUS", "", Some("NEEDS-ACTION"));
        cal.set_time("COMPLETED", None);
        cal.set("PERCENT-COMPLETE", "", None);
    }
}

#[derive(Debug)]
pub enum Error {
    Auth,
    Conflict,
    NoAccount,
    NotFound(String),
    Http(u16, String),
    Net(String),
    Parse(String),
    Db(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Auth => write!(f, "authentication failed"),
            Error::Conflict => write!(f, "precondition failed (changed on server)"),
            Error::NoAccount => write!(f, "no account configured"),
            Error::NotFound(s) => write!(f, "not found: {s}"),
            Error::Http(code, body) => write!(
                f,
                "HTTP {code}: {}",
                body.chars().take(300).collect::<String>()
            ),
            Error::Net(s) => write!(f, "network: {s}"),
            Error::Parse(s) => write!(f, "parse: {s}"),
            Error::Db(s) => write!(f, "database: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64; `url` = URL-safe alphabet without padding (as Web Push wants).
pub fn b64encode(data: &[u8], url: bool) -> String {
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..=c.len() {
            let ch = B64[(n >> (18 - 6 * i) & 63) as usize] as char;
            out.push(match (url, ch) {
                (true, '+') => '-',
                (true, '/') => '_',
                _ => ch,
            });
        }
        if !url {
            out.extend(std::iter::repeat_n('=', 3 - c.len()));
        }
    }
    out
}

/// Decodes both standard and URL-safe base64, padded or not.
pub fn b64decode(s: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|&c| c != b'=' && !c.is_ascii_whitespace()) {
        let v = match c {
            b'-' => 62,
            b'_' => 63,
            c => B64
                .iter()
                .position(|&x| x == c)
                .ok_or_else(|| Error::Parse("bad base64".into()))? as u32,
        };
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_parent_moves_trees_but_refuses_loops() {
        let dir = std::env::temp_dir().join(format!("todav-test-parent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let l = "/l/".to_string();
        c.db.lock()
            .unwrap()
            .execute(
                "INSERT INTO lists (href, display_name) VALUES (?1, 'L')",
                [&l],
            )
            .unwrap();
        let add = |s: &str, parent: Option<String>| {
            c.add_task(l.clone(), s.into(), None, parent, false)
                .unwrap()
                .uid
        };
        let (a, b) = (add("a", None), add("b", None));
        let b1 = add("b1", Some(b.clone()));
        assert!(
            c.set_parent(b.clone(), b1.clone()).is_err(),
            "b under its own subtask"
        );
        c.set_parent(b.clone(), a.clone()).unwrap();
        assert_eq!(c.task(b.clone()).unwrap().parent_uid, Some(a.clone()));
        assert!(c.within(b1, a.clone()) && !c.within(a, b));
    }

    #[test]
    fn due_dates_and_reminders_round_trip_in_local_time() {
        let dir = std::env::temp_dir().join(format!("todav-test-due-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let l = "/l/".to_string();
        c.db.lock()
            .unwrap()
            .execute(
                "INSERT INTO lists (href, display_name) VALUES (?1, 'L')",
                [&l],
            )
            .unwrap();
        let uid = c
            .add_task(l.clone(), "a".into(), None, None, false)
            .unwrap()
            .uid;
        let day = ical::parse_time("20261020").unwrap();
        let set = |due, reminders| {
            let patch = TaskPatch {
                due: Some(due),
                reminders,
                ..Default::default()
            };
            c.update_task(uid.clone(), patch).unwrap();
            c.task(uid.clone()).unwrap()
        };
        let raw = || {
            store::raw(&c.db.lock().unwrap(), &uid)
                .unwrap()
                .unwrap()
                .raw_ics
        };

        let all_day = Due {
            at: day,
            all_day: true,
        };
        let t = set(Some(all_day), Some(vec![9 * 3600, -15 * 3600]));
        assert_eq!((t.due, t.due_all_day), (Some(day), true));
        assert_eq!(
            t.reminders,
            [day - 15 * 3600, day + 9 * 3600],
            "soonest first"
        );
        assert!(raw().contains("DUE;VALUE=DATE:20261020"));
        assert!(
            raw().contains("TRIGGER;RELATED=END:PT9H")
                && raw().contains("TRIGGER;RELATED=END:-PT15H")
        );

        // A time goes out in UTC and comes back as the same local time; the reminders follow it.
        let at = day + 14 * 3600 + 30 * 60;
        let t = set(Some(Due { at, all_day: false }), None);
        assert_eq!((t.due, t.due_all_day), (Some(at), false));
        assert_eq!(t.reminders, [at - 15 * 3600, at + 9 * 3600]);
        assert_eq!(c.upcoming().len(), 1);
        assert_eq!(c.reminders(at, at + 9 * 3600).len(), 1);
        assert!(
            c.reminders(at + 9 * 3600, at + 10 * 3600).is_empty(),
            "fires once"
        );

        let t = set(Some(Due { at, all_day: false }), Some(vec![]));
        assert!(t.reminders.is_empty() && !raw().contains("VALARM"));

        set(Some(Due { at, all_day: false }), Some(vec![0]));
        let t = set(None, None);
        assert_eq!((t.due, t.reminders.len()), (None, 0));
        assert!(!raw().contains("VALARM"), "no due date, no reminders");
        assert!(c.upcoming().is_empty());
    }

    #[test]
    fn restore_brings_a_deleted_task_back() {
        let dir = std::env::temp_dir().join(format!("todav-test-restore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let l = "/l/".to_string();
        c.db.lock()
            .unwrap()
            .execute(
                "INSERT INTO lists (href, display_name) VALUES (?1, 'L')",
                [&l],
            )
            .unwrap();
        let uid = c
            .add_task(l.clone(), "a".into(), None, None, false)
            .unwrap()
            .uid;
        // On the server: the delete is only marked until pushed, and undo clears the mark.
        store::set_sync_state(&c.db.lock().unwrap(), &uid, Some("/l/a.ics"), Some("e"), 0).unwrap();
        let ics = c.delete_task(uid.clone()).unwrap();
        assert!(c.tasks(l.clone(), true).is_empty());
        c.restore_task(l.clone(), ics.clone()).unwrap();
        let raw = store::raw(&c.db.lock().unwrap(), &uid).unwrap().unwrap();
        assert_eq!(
            (raw.href.as_deref(), raw.dirty),
            (Some("/l/a.ics"), store::MODIFIED)
        );
        // Delete already pushed (row gone): re-created as new.
        store::delete(&c.db.lock().unwrap(), &uid).unwrap();
        c.restore_task(l.clone(), ics).unwrap();
        assert_eq!(c.task(uid).unwrap().summary, "a");
    }
}
