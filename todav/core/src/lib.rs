pub mod caldav;
mod config;
mod event;
pub mod ical;
pub mod json;
mod login;
pub mod push;
pub mod rrule;
mod settings;
pub mod store;
mod sync;
pub mod tz;
mod undo;

use caldav::Dav;
use ical::Calendar;
use rusqlite::Connection;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use store::{Kind, RawObject};
use undo::{Before, Journal};

pub use config::{CategoryGroup, CategoryMeta, DoneView, Sort};
pub use event::{Event, EventPatch, EventTime, Repeat, Scope};
pub use login::{Credentials, LoginFlow, login_flow_poll, login_flow_start, password_login};
pub use push::{PushRegistration, ntfy_check};
pub use settings::DEVICE_ONLY;
pub use sync::{Refused, SyncReport};

/// A calendar collection: a task list, a calendar of events, or both (Nextcloud's "Personal").
#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub href: String,
    pub display_name: String,
    pub color: Option<String>,
    pub open_count: u32,
    pub holds_tasks: bool,
    pub holds_events: bool,
    /// We may change it (shared read-only calendars and Birthdays may not).
    pub writable: bool,
    /// Kept on this device only, never synced (`store::LOCAL`).
    pub local: bool,
}

/// Something in a list; hidden with it (see `shown`).
trait InList {
    fn list_href(&self) -> &str;
}

impl InList for List {
    fn list_href(&self) -> &str {
        &self.href
    }
}

impl InList for Task {
    fn list_href(&self) -> &str {
        &self.list_href
    }
}

impl InList for Event {
    fn list_href(&self) -> &str {
        &self.list_href
    }
}

/// Local lists and what is in them are hidden while local mode (the `local_mode` setting) is
/// off; nothing is deleted, turning it on shows them again.
fn shown<T: InList>(db: &Connection, mut v: Vec<T>) -> Vec<T> {
    let on = store::kv_get(db, "setting_local_mode").ok().flatten();
    if on.as_deref() != Some("true") {
        v.retain(|x| !x.list_href().starts_with(store::LOCAL));
    }
    v
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
    /// DTSTART in local time (a task scheduled into a time slot starts then and ends at `due`).
    pub start: Option<i64>,
    pub start_all_day: bool,
    /// UID of the event this task is for (RELATED-TO, see `store::EVENT_LINK`).
    pub event_uid: Option<String>,
    /// Why the server refused this version of the task; it is not uploaded until edited.
    pub sync_error: Option<String>,
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
    /// DTSTART, when the task is planned; Some(None) clears it.
    pub start: Option<Option<Due>>,
    /// The event (UID) the task is for; Some(None) unlinks it.
    pub event: Option<Option<String>>,
    /// PRIORITY, 1 (highest) to 9 (lowest); Some(None) clears it.
    pub priority: Option<Option<i64>>,
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
    /// The system's zone ("Europe/Warsaw"), for writing new event times; see `set_local_zone`.
    local_zone: Mutex<Option<String>>,
    /// What local edits replaced, for `undo`.
    journal: Mutex<Journal>,
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
            local_zone: Mutex::new(None),
            journal: Mutex::new(Journal::default()),
        })
    }

    /// The system's time zone name. New event times are written in it when some stored event
    /// carries its VTIMEZONE, else as floating local times.
    pub fn set_local_zone(&self, tzid: String) {
        *self.local_zone.lock().unwrap() = Some(tzid).filter(|z| !z.is_empty());
    }

    /// How new times in `cal` are written: in the local zone (its VTIMEZONE added to `cal`
    /// when missing) if one is known, else floating.
    fn local_like(&self, db: &Connection, cal: &mut Calendar) -> tz::Stamp {
        let Some(z) = self.local_zone.lock().unwrap().clone() else {
            return tz::Stamp::Floating(0);
        };
        if cal.zone(&z).is_none() {
            match store::vtimezone(db, &z) {
                Ok(Some(lines)) => cal.insert_comp(0, lines),
                _ => return tz::Stamp::Floating(0),
            }
        }
        tz::Stamp::Zoned(0, z)
    }

    /// A new event; returns its UID.
    pub fn add_event(
        &self,
        list_href: String,
        summary: String,
        time: EventTime,
        reminders: Vec<i64>,
    ) -> Result<String> {
        let uid = new_uid();
        let t = ical::fmt_utc(now());
        let mut cal = Calendar::parse(&format!(
            "BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//Todav//EN\nBEGIN:VEVENT\nUID:{uid}\nCREATED:{t}\nDTSTAMP:{t}\nEND:VEVENT\nEND:VCALENDAR\n"
        ))
        .ok_or_else(|| Error::Parse(uid.clone()))?;
        {
            let db = self.db.lock().unwrap();
            let like = self.local_like(&db, &mut cal);
            let patch = EventPatch {
                summary: Some(summary),
                time: Some(time),
                reminders: Some(reminders),
                ..Default::default()
            };
            event::edit(&db, &mut cal, None, Scope::All, &patch, &like)?;
            cal.set("SEQUENCE", "", Some("0"));
            self.remember(&db, Kind::Event, &list_href, &uid)?;
            store::save(&db, &list_href, None, None, &cal, store::MODIFIED)?;
        }
        self.emit(&list_href, vec![uid.clone()]);
        Ok(uid)
    }

    /// Change occurrence `recurrence_id` (None: an event that does not repeat) of an event, or
    /// the occurrences `scope` says.
    pub fn update_event(
        &self,
        list_href: String,
        uid: String,
        recurrence_id: Option<i64>,
        scope: Scope,
        patch: EventPatch,
    ) -> Result<()> {
        {
            let db = self.db.lock().unwrap();
            let raw = store::raw(&db, Kind::Event, &list_href, &uid)?
                .ok_or_else(|| Error::NotFound(uid.clone()))?;
            let mut cal = Calendar::parse(&raw.raw_ics).ok_or_else(|| Error::Parse(uid.clone()))?;
            let like = self.local_like(&db, &mut cal);
            let next = event::edit(&db, &mut cal, recurrence_id, scope, &patch, &like)?;
            self.remember(&db, Kind::Event, &list_href, &uid)?;
            store::save(
                &db,
                &list_href,
                raw.href.as_deref(),
                raw.etag.as_deref(),
                &cal,
                store::MODIFIED,
            )?;
            if let Some(next) = next {
                let uid = next.text("UID").unwrap_or_default();
                self.remember(&db, Kind::Event, &list_href, &uid)?;
                store::save(&db, &list_href, None, None, &next, store::MODIFIED)?;
            }
        }
        self.emit(&list_href, vec![uid]);
        Ok(())
    }

    /// Delete occurrence `recurrence_id` (None: the event), or the occurrences `scope` says.
    pub fn delete_event(
        &self,
        list_href: String,
        uid: String,
        recurrence_id: Option<i64>,
        scope: Scope,
    ) -> Result<()> {
        {
            let db = self.db.lock().unwrap();
            let raw = store::raw(&db, Kind::Event, &list_href, &uid)?
                .ok_or_else(|| Error::NotFound(uid.clone()))?;
            let mut cal = Calendar::parse(&raw.raw_ics).ok_or_else(|| Error::Parse(uid.clone()))?;
            let like = self.local_like(&db, &mut cal);
            self.remember(&db, Kind::Event, &list_href, &uid)?;
            if !event::delete(&db, &mut cal, recurrence_id, scope, &like)? {
                store::save(
                    &db,
                    &list_href,
                    raw.href.as_deref(),
                    raw.etag.as_deref(),
                    &cal,
                    store::MODIFIED,
                )?;
            } else {
                forget(&db, &raw)?;
            }
        }
        self.emit(&list_href, vec![uid]);
        Ok(())
    }

    /// Move an event (all of it) to another calendar.
    pub fn move_event(&self, from: String, uid: String, to: String) -> Result<()> {
        if from == to {
            return Ok(());
        }
        {
            let db = self.db.lock().unwrap();
            let raw = store::raw(&db, Kind::Event, &from, &uid)?
                .ok_or_else(|| Error::NotFound(uid.clone()))?;
            let cal = Calendar::parse(&raw.raw_ics).ok_or_else(|| Error::Parse(uid.clone()))?;
            self.together(|| {
                self.remember(&db, Kind::Event, &from, &uid)?;
                self.remember(&db, Kind::Event, &to, &uid)?;
                store::save(&db, &to, None, None, &cal, store::MODIFIED)?;
                forget(&db, &raw)
            })?;
        }
        self.emit(&from, vec![uid.clone()]);
        self.emit(&to, vec![uid]);
        Ok(())
    }

    /// Move a task, with its subtasks, to another list. Each goes there as a new item; a server
    /// copy left behind is deleted on the next push (`moved`).
    pub fn move_task(&self, uid: String, to: String) -> Result<()> {
        let from = {
            let db = self.db.lock().unwrap();
            let raw = store::raw(&db, Kind::Task, "", &uid)?
                .ok_or_else(|| Error::NotFound(uid.clone()))?;
            if raw.list_href == to {
                return Ok(());
            }
            let all = store::tasks(&db, &raw.list_href, true)?;
            let mut tree = vec![uid.clone()];
            let mut i = 0;
            while i < tree.len() {
                let p = tree[i].clone();
                let kids = all.iter().filter(|t| t.parent_uid.as_ref() == Some(&p));
                tree.extend(kids.map(|t| t.uid.clone()));
                i += 1;
            }
            self.together(|| -> Result<()> {
                for u in &tree {
                    let Some(r) = store::raw(&db, Kind::Task, "", u)? else {
                        continue;
                    };
                    let cal = Calendar::parse(&r.raw_ics).ok_or_else(|| Error::Parse(u.clone()))?;
                    self.remember(&db, Kind::Task, "", u)?;
                    if let Some(href) = &r.href {
                        db.execute(
                            "INSERT OR REPLACE INTO moved (href, etag, list_href, uid) VALUES (?1, ?2, ?3, ?4)",
                            rusqlite::params![href, r.etag, r.list_href, u],
                        )?;
                    }
                    store::save(&db, &to, None, None, &cal, store::MODIFIED)?;
                }
                Ok(())
            })?;
            raw.list_href
        };
        self.emit(&from, vec![uid.clone()]);
        self.emit(&to, vec![uid]);
        Ok(())
    }

    /// The password is kept in memory only; front-ends keep it in the platform keyring.
    pub fn set_account(&self, url: String, user: String, app_password: String) -> Result<()> {
        let db = self.db.lock().unwrap();
        if store::kv_get(&db, "account_url")?.as_deref() != Some(&url)
            || store::kv_get(&db, "account_user")?.as_deref() != Some(&user)
        {
            // Different account: its copy belongs to the old one (lists on this device stay).
            db.execute_batch(&format!(
                "DELETE FROM tasks WHERE list_href NOT LIKE '{0}%'; DELETE FROM lists WHERE href NOT LIKE '{0}%';
                 DELETE FROM moved;
                 DELETE FROM kv WHERE key LIKE 'config_%' OR key IN ('push_resource', 'conflict_log');",
                store::LOCAL
            ))?;
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

    /// Forget the account and its local copy (keeps device keys and the local lists).
    pub fn logout(&self) -> Result<()> {
        let _ = self.push_unregister();
        let db = self.db.lock().unwrap();
        db.execute_batch(&format!(
            "DELETE FROM tasks WHERE list_href NOT LIKE '{0}%'; DELETE FROM lists WHERE href NOT LIKE '{0}%';
             DELETE FROM moved;
             DELETE FROM kv WHERE key LIKE 'config_%' OR key LIKE 'account_%' OR key IN ('push_resource', 'conflict_log');",
            store::LOCAL
        ))?;
        *self.dav.lock().unwrap() = None;
        Ok(())
    }

    fn dav(&self) -> Result<Arc<Dav>> {
        self.dav.lock().unwrap().clone().ok_or(Error::NoAccount)
    }

    /// Task lists.
    pub fn lists(&self) -> Vec<List> {
        let db = self.db.lock().unwrap();
        let all = shown(&db, store::lists(&db).unwrap_or_default());
        all.into_iter().filter(|l| l.holds_tasks).collect()
    }

    /// Calendars of events.
    pub fn calendars(&self) -> Vec<List> {
        let db = self.db.lock().unwrap();
        let all = shown(&db, store::lists(&db).unwrap_or_default());
        all.into_iter().filter(|l| l.holds_events).collect()
    }

    /// Occurrences of every event overlapping `[from, to)` (local times), soonest first.
    pub fn events(&self, from: i64, to: i64) -> Vec<Event> {
        let db = self.db.lock().unwrap();
        shown(&db, store::events(&db, from, to).unwrap_or_default())
    }

    /// Event occurrences with a reminder going off after `from` and at `to` at the latest
    /// (local times). ponytail: looks a week ahead, so reminders set further before an event
    /// are missed; widen if someone uses them.
    pub fn event_reminders(&self, from: i64, to: i64) -> Vec<Event> {
        let mut all = self.events(from, to + 8 * 86400);
        all.retain(|e| !e.cancelled && e.reminders.iter().any(|r| *r > from && *r <= to));
        all
    }

    pub fn tasks(&self, list_href: String, include_done: bool) -> Vec<Task> {
        let db = self.db.lock().unwrap();
        shown(
            &db,
            store::tasks(&db, &list_href, include_done).unwrap_or_default(),
        )
    }

    pub fn task(&self, uid: String) -> Option<Task> {
        let db = self.db.lock().unwrap();
        let t = store::task(&db, &uid).ok()??;
        shown(&db, vec![t]).pop()
    }

    /// Open tasks with a due date in every list, soonest first.
    pub fn upcoming(&self) -> Vec<Task> {
        let db = self.db.lock().unwrap();
        shown(&db, store::upcoming(&db).unwrap_or_default())
    }

    /// Tasks, done ones too, due or starting in `[from, to)` (local times): what the calendar shows.
    pub fn scheduled(&self, from: i64, to: i64) -> Vec<Task> {
        let db = self.db.lock().unwrap();
        shown(&db, store::scheduled(&db, from, to).unwrap_or_default())
    }

    /// The event with this UID: its next occurrence from now (within a year), else its first.
    pub fn event(&self, uid: String) -> Option<Event> {
        let db = self.db.lock().unwrap();
        let now = store::to_local(&db, now()).unwrap_or_else(|_| now());
        let e = store::event_by_uid(&db, &uid, now).ok()??;
        shown(&db, vec![e]).pop()
    }

    /// Tasks (done ones too) for the event with this UID.
    pub fn event_tasks(&self, event_uid: String) -> Vec<Task> {
        let db = self.db.lock().unwrap();
        shown(&db, store::event_tasks(&db, &event_uid).unwrap_or_default())
    }

    /// Open tasks with no time slot yet: undated, or due by a date before `until`.
    pub fn unscheduled(&self, until: i64) -> Vec<Task> {
        let db = self.db.lock().unwrap();
        shown(&db, store::unscheduled(&db, until).unwrap_or_default())
    }

    /// Open tasks with a reminder going off after `from` and at `to` at the latest (local times).
    pub fn reminders(&self, from: i64, to: i64) -> Vec<Task> {
        let db = self.db.lock().unwrap();
        shown(&db, store::reminders(&db, from, to).unwrap_or_default())
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
            self.remember(&db, Kind::Task, &list_href, &uid)?;
            store::save(&db, &list_href, None, None, &cal, store::MODIFIED)?;
        }
        self.emit(&list_href, vec![uid.clone()]);
        self.task(uid.clone()).ok_or(Error::NotFound(uid))
    }

    /// Load, patch and save a task as locally modified.
    fn modify(&self, uid: &str, f: impl FnOnce(&mut Calendar)) -> Result<()> {
        let list = {
            let db = self.db.lock().unwrap();
            let raw =
                store::raw(&db, Kind::Task, "", uid)?.ok_or_else(|| Error::NotFound(uid.into()))?;
            let mut cal = Calendar::parse(&raw.raw_ics).ok_or_else(|| Error::Parse(uid.into()))?;
            f(&mut cal);
            touch(&mut cal);
            self.remember(&db, Kind::Task, "", uid)?;
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
        let utc = |d: Option<Option<Due>>| -> Result<Option<i64>> {
            match d {
                Some(Some(d)) if !d.all_day => {
                    Ok(Some(store::to_utc(&self.db.lock().unwrap(), d.at)?))
                }
                _ => Ok(None),
            }
        };
        let (due_utc, start_utc) = (utc(patch.due)?, utc(patch.start)?);
        let set_date = |cal: &mut Calendar, name: &str, d: Option<Due>, utc: Option<i64>| match d {
            None => cal.set(name, "", None),
            Some(d) if d.all_day => cal.set(name, ";VALUE=DATE", Some(&ical::fmt_utc(d.at)[..8])),
            Some(_) => cal.set_time(name, utc),
        };
        self.modify(&uid, |cal| {
            if let Some(d) = patch.due {
                set_date(cal, "DUE", d, due_utc);
                if d.is_none() {
                    cal.set_alarms(&[]);
                }
            }
            if let Some(s) = patch.start {
                set_date(cal, "DTSTART", s, start_utc);
            }
            if let Some(e) = &patch.event {
                cal.set_related(store::EVENT_LINK, e.as_deref());
            }
            if let Some(p) = patch.priority {
                cal.set("PRIORITY", "", p.map(|p| p.to_string()).as_deref());
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

    pub fn delete_task(&self, uid: String) -> Result<()> {
        let list = {
            let db = self.db.lock().unwrap();
            let raw = store::raw(&db, Kind::Task, "", &uid)?
                .ok_or_else(|| Error::NotFound(uid.clone()))?;
            self.remember(&db, Kind::Task, "", &uid)?;
            forget(&db, &raw)?;
            raw.list_href
        };
        self.emit(&list, vec![uid]);
        Ok(())
    }

    /// Changes `f` makes are undone together, as one action.
    pub fn together<R>(&self, f: impl FnOnce() -> R) -> R {
        self.journal.lock().unwrap().begin();
        let r = f();
        self.journal.lock().unwrap().end();
        r
    }

    /// The latest change, to undo just that one later (a toast's Undo button).
    pub fn last_change(&self) -> Option<u64> {
        self.journal.lock().unwrap().last()
    }

    /// Put back what change `id` (else the latest) replaced; false when there is none. A delete
    /// the server already has comes back as a new resource there.
    pub fn undo(&self, id: Option<u64>) -> Result<bool> {
        let Some(before) = self.journal.lock().unwrap().take(id) else {
            return Ok(false);
        };
        let mut changed = Vec::new();
        {
            let db = self.db.lock().unwrap();
            for b in before.into_iter().rev() {
                let now = store::raw(&db, b.kind, &b.list_href, &b.uid)?
                    .filter(|r| r.dirty != store::DELETED);
                match (b.raw_ics, now) {
                    (Some(ics), Some(r)) if r.raw_ics == ics && r.list_href == b.list_href => {
                        continue;
                    }
                    (Some(ics), _) => {
                        let mut cal =
                            Calendar::parse(&ics).ok_or_else(|| Error::Parse(b.uid.clone()))?;
                        touch(&mut cal); // newer than what it replaces, wherever that went
                        // A pending delete still holds the resource's href.
                        let r = store::raw(&db, b.kind, &b.list_href, &b.uid)?;
                        let (mut href, mut etag) = r
                            .as_ref()
                            .map_or((None, None), |r| (r.href.clone(), r.etag.clone()));
                        // A task moved to another list comes back: its copy there goes, and
                        // its server copy here stays if not deleted yet.
                        if let Some(r) = r.filter(|r| r.list_href != b.list_href) {
                            if let Some(h) = &r.href {
                                db.execute(
                                    "INSERT OR REPLACE INTO moved (href, etag, list_href, uid) VALUES (?1, ?2, ?3, ?4)",
                                    rusqlite::params![h, r.etag, r.list_href, r.uid],
                                )?;
                            }
                            (href, etag) = db
                                .query_row(
                                    "SELECT href, etag FROM moved WHERE list_href = ?1 AND uid = ?2",
                                    [&b.list_href, &b.uid],
                                    |row| Ok((row.get(0)?, row.get(1)?)),
                                )
                                .map_or((None, None), |(h, e)| (Some(h), e));
                            db.execute(
                                "DELETE FROM moved WHERE list_href = ?1 AND uid = ?2",
                                [&b.list_href, &b.uid],
                            )?;
                            changed.push((r.list_href, b.uid.clone()));
                        }
                        store::save(
                            &db,
                            &b.list_href,
                            href.as_deref(),
                            etag.as_deref(),
                            &cal,
                            store::MODIFIED,
                        )?;
                    }
                    (None, Some(r)) => forget(&db, &r)?,
                    (None, None) => continue,
                }
                changed.push((b.list_href, b.uid));
            }
        }
        for (list, uid) in changed {
            self.emit(&list, vec![uid]);
        }
        Ok(true)
    }

    /// Note an item as it is before a change, for `undo`; `list_href` may be empty for a task.
    fn remember(&self, db: &Connection, kind: Kind, list_href: &str, uid: &str) -> Result<()> {
        let raw = store::raw(db, kind, list_href, uid)?.filter(|r| r.dirty != store::DELETED);
        self.journal.lock().unwrap().record(Before {
            kind,
            list_href: raw.as_ref().map_or(list_href, |r| &r.list_href).to_string(),
            uid: uid.to_string(),
            raw_ics: raw.map(|r| r.raw_ics),
        });
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
        self.together(|| {
            for (i, t) in order.iter().enumerate() {
                let i = i as i64 + 1;
                if t.sort_order != Some(i) {
                    self.modify(&t.uid, |cal| set_order(cal, i))?;
                }
            }
            Ok(())
        })
    }
}

/// Delete `o`: at once if the server never had it, else with the next sync.
fn forget(db: &Connection, o: &RawObject) -> Result<()> {
    if o.href.is_none() {
        store::delete(db, o)
    } else {
        store::set_sync_state(db, o, o.href.as_deref(), o.etag.as_deref(), store::DELETED)
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
            // A WebDAV error body is XML: its <s:message> is the part worth reading.
            Error::Http(code, body) => {
                let message = body
                    .split_once("<s:message>")
                    .and_then(|(_, m)| m.split_once("</s:message>"))
                    .map_or(body.as_str(), |(m, _)| m);
                let short: String = message.chars().take(300).collect();
                write!(f, "HTTP {code}: {}", short.trim())
            }
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
    fn undo_puts_back_edits_deletes_and_additions_newest_first() {
        let dir = std::env::temp_dir().join(format!("todav-test-undo-{}", std::process::id()));
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
        let a = c
            .add_task(l.clone(), "a".into(), None, None, false)
            .unwrap();
        let added = c.last_change();
        let b = c
            .add_task(l.clone(), "b".into(), None, None, false)
            .unwrap();
        c.set_done(a.uid.clone(), true).unwrap();
        c.delete_task(b.uid.clone()).unwrap();
        // Done and deleted, then the order changed in one action.
        c.reorder(a.uid.clone(), None).unwrap();
        c.together(|| c.set_done(a.uid.clone(), false)).unwrap();

        assert!(c.undo(None).unwrap());
        assert!(c.task(a.uid.clone()).unwrap().done);
        c.undo(None).unwrap(); // the reorder (nothing moved: no step)
        assert_eq!(c.task(b.uid.clone()).unwrap().summary, "b", "b is back");
        c.undo(None).unwrap();
        assert!(!c.task(a.uid.clone()).unwrap().done);
        // The first addition, out of turn.
        assert!(c.undo(added).unwrap());
        assert!(c.task(a.uid.clone()).is_none());
        c.undo(None).unwrap();
        assert!(c.task(b.uid).is_none());
        assert!(!c.undo(None).unwrap(), "nothing left");
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
            store::raw(&c.db.lock().unwrap(), Kind::Task, "", &uid)
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
    fn http_errors_show_the_servers_message() {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:error xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns">
  <s:exception>Sabre\DAV\Exception\UnsupportedMediaType</s:exception>
  <s:message>Validation error in iCalendar: DUE must occur after DTSTART</s:message>
</d:error>"#;
        assert_eq!(
            Error::Http(415, body.into()).to_string(),
            "HTTP 415: Validation error in iCalendar: DUE must occur after DTSTART"
        );
        assert_eq!(
            Error::Http(500, "oops".into()).to_string(),
            "HTTP 500: oops"
        );
    }

    #[test]
    fn planning_a_task_into_a_time_slot() {
        let dir = std::env::temp_dir().join(format!("todav-test-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        c.db.lock()
            .unwrap()
            .execute(
                "INSERT INTO lists (href, display_name) VALUES ('/l/', 'L')",
                [],
            )
            .unwrap();
        let uid = c
            .add_task("/l/".into(), "a".into(), None, None, false)
            .unwrap()
            .uid;
        let day = ical::parse_time("20261021").unwrap();
        let week = day + 7 * 86400;
        assert_eq!(c.unscheduled(week).len(), 1, "undated: to plan");
        let at = |t: i64| {
            Some(Due {
                at: t,
                all_day: false,
            })
        };
        let (from, to) = (day + 10 * 3600, day + 11 * 3600);
        let patch = TaskPatch {
            start: Some(at(from)),
            due: Some(at(to)),
            ..Default::default()
        };
        c.update_task(uid.clone(), patch).unwrap();
        let t = c.task(uid.clone()).unwrap();
        assert_eq!(
            (t.start, t.start_all_day, t.due),
            (Some(from), false, Some(to))
        );
        assert_eq!(c.scheduled(day, day + 86400).len(), 1);
        assert!(c.unscheduled(week).is_empty(), "planned");
        // Back to a plain due date: to plan again.
        let all_day = Some(Due {
            at: day,
            all_day: true,
        });
        let patch = TaskPatch {
            start: Some(None),
            due: Some(all_day),
            ..Default::default()
        };
        c.update_task(uid.clone(), patch).unwrap();
        assert_eq!(c.task(uid.clone()).unwrap().start, None);
        assert_eq!(c.unscheduled(week).len(), 1);
        assert!(c.unscheduled(day).is_empty(), "due later than asked");
        // Priority is set and cleared on its own.
        let priority = |p| TaskPatch {
            priority: Some(p),
            ..Default::default()
        };
        c.update_task(uid.clone(), priority(Some(1))).unwrap();
        assert_eq!(c.task(uid.clone()).unwrap().priority, Some(1));
        c.update_task(uid.clone(), priority(None)).unwrap();
        assert_eq!(c.task(uid).unwrap().priority, None);
    }

    #[test]
    fn tasks_link_to_an_event() {
        let dir = std::env::temp_dir().join(format!("todav-test-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        c.db.lock()
            .unwrap()
            .execute(
                "INSERT INTO lists (href, display_name) VALUES ('/l/', 'L')",
                [],
            )
            .unwrap();
        let uid = c
            .add_task("/l/".into(), "Slides".into(), None, None, false)
            .unwrap()
            .uid;
        let link = |e: Option<&str>| {
            let patch = TaskPatch {
                event: Some(e.map(String::from)),
                ..Default::default()
            };
            c.update_task(uid.clone(), patch).unwrap();
        };
        link(Some("meeting-1"));
        let t = c.task(uid.clone()).unwrap();
        assert_eq!(
            (t.event_uid.as_deref(), t.parent_uid),
            (Some("meeting-1"), None),
            "not a parent"
        );
        assert_eq!(c.event_tasks("meeting-1".into()).len(), 1);
        link(None);
        assert!(c.event_tasks("meeting-1".into()).is_empty());
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
        let raw = || {
            store::raw(&c.db.lock().unwrap(), Kind::Task, "", &uid)
                .unwrap()
                .unwrap()
        };
        let row = raw();
        store::set_sync_state(&c.db.lock().unwrap(), &row, Some("/l/a.ics"), Some("e"), 0).unwrap();
        c.delete_task(uid.clone()).unwrap();
        assert!(c.tasks(l.clone(), true).is_empty());
        c.undo(None).unwrap();
        assert_eq!(
            (raw().href.as_deref(), raw().dirty),
            (Some("/l/a.ics"), store::MODIFIED)
        );
        // Delete already pushed (row gone): re-created as new.
        c.delete_task(uid.clone()).unwrap();
        let row = raw();
        store::delete(&c.db.lock().unwrap(), &row).unwrap();
        c.undo(None).unwrap();
        assert_eq!(c.task(uid).unwrap().summary, "a");
    }

    #[test]
    fn local_lists_stay_on_the_device() {
        let dir = std::env::temp_dir().join(format!("todav-test-local-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let list = c
            .create_list("L".into(), "#fff".into(), false, true)
            .unwrap();
        assert!(c.lists().is_empty(), "hidden while local mode is off");
        c.set_setting("local_mode".into(), "true".into()).unwrap();
        assert!(c.lists()[0].local);
        let uid = c
            .add_task(list.clone(), "a".into(), None, None, false)
            .unwrap()
            .uid;
        let queue = || store::dirty(&c.db.lock().unwrap()).unwrap();
        assert!(queue().is_empty() && !c.pending(), "never uploaded");

        // Signing in and out drops the server copy only.
        c.set_account("https://x".into(), "u".into(), "p".into())
            .unwrap();
        c.logout().unwrap();
        assert_eq!(c.tasks(list.clone(), true).len(), 1);

        // Deleted outright, nothing waits for the server.
        c.delete_task(uid.clone()).unwrap();
        assert!(
            store::raw(&c.db.lock().unwrap(), Kind::Task, "", &uid)
                .unwrap()
                .is_none()
        );

        // An event moved to a server calendar is uploaded, and one moved back is deleted there.
        let cal = c
            .create_list("C".into(), "#fff".into(), true, true)
            .unwrap();
        c.db.lock()
            .unwrap()
            .execute(
                "INSERT INTO lists (href, display_name) VALUES ('/r/', 'R')",
                [],
            )
            .unwrap();
        let time = EventTime {
            start: 0,
            end: 3600,
            all_day: false,
        };
        let ev = c.add_event(cal.clone(), "e".into(), time, vec![]).unwrap();
        c.move_event(cal.clone(), ev.clone(), "/r/".into()).unwrap();
        assert_eq!(queue()[0].list_href, "/r/");
        let row = queue().remove(0);
        store::set_sync_state(&c.db.lock().unwrap(), &row, Some("/r/e.ics"), Some("x"), 0).unwrap();
        c.move_event("/r/".into(), ev, cal.clone()).unwrap();
        assert_eq!(
            queue().iter().map(|r| r.dirty).collect::<Vec<_>>(),
            [store::DELETED]
        );

        c.delete_list(list).unwrap();
        c.delete_list(cal).unwrap();
        assert!(c.calendars().iter().all(|l| !l.local));
    }

    #[test]
    fn a_moved_task_takes_its_subtasks_and_undo_brings_it_back() {
        let dir = std::env::temp_dir().join(format!("todav-test-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        c.db.lock()
            .unwrap()
            .execute_batch(
                "INSERT INTO lists (href, display_name) VALUES ('/a/', 'A'), ('/b/', 'B')",
            )
            .unwrap();
        let add = |s: &str, parent: Option<String>| {
            c.add_task("/a/".into(), s.into(), None, parent, false)
                .unwrap()
                .uid
        };
        let parent = add("p", None);
        let sub = add("s", Some(parent.clone()));
        // The parent is on the server already, the subtask not yet.
        let raw = |uid: &str| {
            store::raw(&c.db.lock().unwrap(), Kind::Task, "", uid)
                .unwrap()
                .unwrap()
        };
        let row = raw(&parent);
        store::set_sync_state(&c.db.lock().unwrap(), &row, Some("/a/p.ics"), Some("e"), 0).unwrap();

        c.move_task(parent.clone(), "/b/".into()).unwrap();
        assert_eq!(
            (raw(&parent).list_href, raw(&sub).list_href),
            ("/b/".into(), "/b/".into())
        );
        assert_eq!(raw(&parent).href, None, "a new item there");
        let moved = || -> Vec<String> {
            let db = c.db.lock().unwrap();
            let mut st = db.prepare("SELECT href FROM moved").unwrap();
            st.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(moved(), ["/a/p.ics"]);
        assert!(c.pending());

        // Back where it was, with its server copy, which is no longer deleted.
        c.undo(None).unwrap();
        assert_eq!(raw(&parent).list_href, "/a/");
        assert_eq!(raw(&parent).href.as_deref(), Some("/a/p.ics"));
        assert_eq!(raw(&sub).list_href, "/a/");
        assert!(moved().is_empty());
    }
}
