//! The system's calendars (Android's calendar store, filled by the device's accounts: Google,
//! Exchange, DAVx⁵…). The front-end reads the calendars the user turned on and hands
//! their occurrences over; each is stored as an event of its own, repeats, exceptions and time
//! zones already worked out by the system. A change made here to a calendar that allows it
//! waits in the dirty queue for the front-end to make it in the system's store.

use crate::ical::{escape, fmt_utc};
use crate::store::{self, Kind};
use crate::sync::SyncReport;
use crate::{AccountKind, Client, Error, Result};
use rusqlite::params;

/// A calendar of the system's, as the system account's list `href`.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemCalendar {
    pub href: String,
    pub name: String,
    /// "#rrggbb".
    pub color: String,
    /// Its events may be changed here: the front-end makes the changes in the system's store
    /// (`system_changes`).
    pub writable: bool,
}

/// One occurrence of an event in a system calendar.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemEvent {
    /// The same for the occurrence each time it is read (its event and its start).
    pub uid: String,
    /// Unix time; an all-day one from midnight UTC of its first day to that of the day after.
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub title: String,
    pub description: String,
    pub location: String,
    /// Its own colour ("#rrggbb"; empty: its calendar's).
    pub color: String,
}

/// A change made here to an event of a system calendar, for the front-end to make in the
/// system's store.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemChange {
    pub list_href: String,
    /// The event is gone (only its `uid` is set); otherwise it is new (a UID of Todav's own) or
    /// edited (the UID `set_system_events` gave it).
    pub deleted: bool,
    pub event: SystemEvent,
}

impl Client {
    /// The system account's calendars are these now; others it had go.
    pub fn set_system_calendars(&self, account: i64, calendars: Vec<SystemCalendar>) -> Result<()> {
        let kind = self.account(account).ok_or(Error::NoAccount)?.kind;
        if kind != AccountKind::System {
            return Err(Error::NoAccount);
        }
        let gone: Vec<String> = self
            .account_lists(account)
            .into_iter()
            .filter(|h| !calendars.iter().any(|c| c.href == *h))
            .collect();
        {
            let db = self.db.lock().unwrap();
            let tx = db.unchecked_transaction()?;
            for c in &calendars {
                tx.execute(
                    "INSERT INTO lists (href, display_name, color, has_tasks, has_events, writable, account)
                     VALUES (?1, ?2, ?3, 0, 1, ?5, ?4)
                     ON CONFLICT(href) DO UPDATE SET display_name = ?2, color = ?3, writable = ?5",
                    params![c.href, c.name, c.color, account, c.writable],
                )?;
            }
            for h in &gone {
                tx.execute("DELETE FROM lists WHERE href = ?", [h])?;
            }
            tx.commit()?;
        }
        for h in calendars.iter().map(|c| &c.href).chain(&gone) {
            self.emit(h, Vec::new());
        }
        Ok(())
    }

    /// System calendar `list_href` holds these occurrences now.
    pub fn set_system_events(&self, list_href: String, events: Vec<SystemEvent>) -> Result<()> {
        let changed = events
            .iter()
            .map(|e| {
                let ics = to_ics(e);
                (format!("{list_href}#{}", e.uid), hash(&ics), ics)
            })
            .collect();
        let _guard = self.sync_lock.lock().unwrap();
        self.apply(
            &list_href,
            changed,
            Vec::new(),
            true,
            &mut SyncReport::default(),
        )
    }

    /// What was changed here in the system calendars' events (added, edited, deleted) and is not
    /// in the system's store yet. The front-end makes each change there, then says so with
    /// `system_applied`; the next `set_system_events` brings the system's version back.
    pub fn system_changes(&self) -> Result<Vec<SystemChange>> {
        let rows = store::dirty_system(&self.db.lock().unwrap())?;
        let mut out = Vec::new();
        for r in rows {
            let event = if r.dirty == store::DELETED {
                Some(SystemEvent {
                    uid: r.uid.clone(),
                    start: 0,
                    end: 0,
                    all_day: false,
                    title: String::new(),
                    description: String::new(),
                    location: String::new(),
                    color: String::new(),
                })
            } else {
                self.event(r.uid.clone()).map(|e| SystemEvent {
                    uid: e.uid,
                    // Local wall-clock times here; the system's are instants (an all-day
                    // event's days are the same in both).
                    start: instant(self, e.start, e.all_day),
                    end: instant(self, e.end, e.all_day),
                    all_day: e.all_day,
                    title: e.summary,
                    description: e.description.unwrap_or_default(),
                    location: e.location.unwrap_or_default(),
                    color: e.color.unwrap_or_default(),
                })
            };
            if let Some(event) = event {
                out.push(SystemChange {
                    list_href: r.list_href,
                    deleted: r.dirty == store::DELETED,
                    event,
                });
            }
        }
        Ok(out)
    }

    /// The change to event `uid` of system calendar `list_href` is in the system's store now (or
    /// cannot be made): its copy here goes, as the system's own comes with the next
    /// `set_system_events`.
    pub fn system_applied(&self, list_href: String, uid: String) -> Result<()> {
        let _guard = self.sync_lock.lock().unwrap();
        {
            let db = self.db.lock().unwrap();
            if let Some(raw) = store::raw(&db, Kind::Event, &list_href, &uid)? {
                store::delete(&db, &raw)?;
            }
        }
        self.emit(&list_href, vec![uid]);
        Ok(())
    }

    /// Refuse to change list `href` when it is read-only: nothing would take the change (the
    /// system's calendars), or the server would refuse it.
    pub(crate) fn writable(&self, href: &str) -> Result<()> {
        let db = self.db.lock().unwrap();
        let writable: bool = db
            .query_row("SELECT writable FROM lists WHERE href = ?", [href], |r| {
                r.get(0)
            })
            .map_err(|_| Error::NotFound(href.into()))?;
        if writable {
            Ok(())
        } else {
            Err(Error::ReadOnly)
        }
    }
}

/// The instant of a local wall-clock time (an all-day event's midnight stays as it is).
fn instant(c: &Client, local: i64, all_day: bool) -> i64 {
    if all_day {
        return local;
    }
    store::to_utc(&c.db.lock().unwrap(), local).unwrap_or(local)
}

/// An occurrence as an iCalendar object, the same text each time for the same occurrence.
fn to_ics(e: &SystemEvent) -> String {
    let time = |name: &str, t: i64| {
        if e.all_day {
            format!("{name};VALUE=DATE:{}\r\n", &fmt_utc(t)[..8])
        } else {
            format!("{name}:{}\r\n", fmt_utc(t))
        }
    };
    let mut s = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Todav//EN\r\nBEGIN:VEVENT\r\nUID:{}\r\nDTSTAMP:{}\r\n",
        escape(&e.uid),
        fmt_utc(e.start)
    );
    s += &time("DTSTART", e.start);
    s += &time("DTEND", e.end.max(e.start));
    s += &format!("SUMMARY:{}\r\n", escape(&e.title));
    // Its colour as is: the system's events are read only here, and COLOR is read in hex too.
    for (name, v) in [
        ("LOCATION", &e.location),
        ("DESCRIPTION", &e.description),
        ("COLOR", &e.color),
    ] {
        if !v.is_empty() {
            s += &format!("{name}:{}\r\n", escape(v));
        }
    }
    s + "END:VEVENT\r\nEND:VCALENDAR\r\n"
}

/// An ETag for the text: FNV-1a.
fn hash(s: &str) -> String {
    let h = s.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_calendars_are_read_only_copies() {
        let dir = std::env::temp_dir().join(format!("todav-test-system-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let add = || c.add_account(AccountKind::System, "".into(), "".into(), "".into());
        let id = add().unwrap();
        assert!(matches!(add(), Err(Error::Exists)), "one system account");
        let cal = |n: &str| SystemCalendar {
            href: format!("content://com.android.calendar/calendars/{n}"),
            name: n.into(),
            color: "#3584e4".into(),
            writable: false,
        };
        c.set_system_calendars(id, vec![cal("1"), cal("2")])
            .unwrap();
        let href = cal("1").href;
        let ev = |uid: &str, title: &str| SystemEvent {
            uid: uid.into(),
            start: 1_793_181_600,
            end: 1_793_185_200,
            all_day: false,
            title: title.into(),
            description: String::new(),
            location: "Kraków; room 2".into(),
            color: String::new(),
        };
        c.set_system_events(href.clone(), vec![ev("a", "One"), ev("b", "Two")])
            .unwrap();
        let titles = |c: &Client| {
            let mut t: Vec<String> = c
                .events(0, i64::MAX / 4)
                .into_iter()
                .map(|e| e.summary)
                .collect();
            t.sort();
            t
        };
        assert_eq!(titles(&c), ["One", "Two"]);
        let one = c.event("a".into()).unwrap();
        assert_eq!(
            (one.end - one.start, one.location.as_deref()),
            (3600, Some("Kraków; room 2"))
        );

        // Read again: changed, gone; nothing waits for a server.
        c.set_system_events(href.clone(), vec![ev("a", "One, moved")])
            .unwrap();
        assert_eq!(titles(&c), ["One, moved"]);
        assert!(!c.pending(None));

        // Not to be changed here.
        let r = c.delete_event(href.clone(), "a".into(), None, crate::Scope::All);
        assert!(matches!(r, Err(Error::ReadOnly)), "{r:?}");
        let time = crate::EventTime {
            start: one.start,
            end: one.end,
            all_day: false,
        };
        assert!(matches!(
            c.add_event(href.clone(), "x".into(), time, vec![]),
            Err(Error::ReadOnly)
        ));

        // A calendar turned off goes with its events.
        c.set_system_calendars(id, vec![cal("2")]).unwrap();
        assert!(titles(&c).is_empty());
        assert_eq!(c.calendars().len(), 1);
    }

    #[test]
    fn a_system_calendar_is_renamed_and_recoloured_here_only() {
        let dir = std::env::temp_dir().join(format!("todav-test-sysname-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let id = c
            .add_account(AccountKind::System, "".into(), "".into(), "".into())
            .unwrap();
        let cal = |name: &str| SystemCalendar {
            href: "content://com.android.calendar/calendars/1".into(),
            name: name.into(),
            color: "#3584e4".into(),
            writable: false,
        };
        let shown = |c: &Client| {
            let l = c.calendars().remove(0);
            (l.display_name, l.color.unwrap_or_default())
        };
        c.set_system_calendars(id, vec![cal("Work")]).unwrap();
        c.edit_list(cal("").href, "Office".into(), "#B6469D".into())
            .unwrap();
        assert_eq!(shown(&c), ("Office".into(), "#B6469D".into()));
        // The system renaming it changes nothing here.
        c.set_system_calendars(id, vec![cal("Job")]).unwrap();
        assert_eq!(shown(&c), ("Office".into(), "#B6469D".into()));
        // Given the system's name and colour again, it follows the system's.
        c.edit_list(cal("").href, "Job".into(), "#3584e4".into())
            .unwrap();
        c.set_system_calendars(id, vec![cal("Work")]).unwrap();
        assert_eq!(shown(&c), ("Work".into(), "#3584e4".into()));
        // Reset: both the system's again.
        c.edit_list(cal("").href, "Office".into(), "#B6469D".into())
            .unwrap();
        c.reset_list(cal("").href).unwrap();
        assert_eq!(shown(&c), ("Work".into(), "#3584e4".into()));
    }

    #[test]
    fn changes_to_a_writable_system_calendar_wait_for_the_front_end() {
        let dir = std::env::temp_dir().join(format!("todav-test-syswrite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let id = c
            .add_account(AccountKind::System, "".into(), "".into(), "".into())
            .unwrap();
        let href = "content://com.android.calendar/calendars/1".to_string();
        let cal = SystemCalendar {
            href: href.clone(),
            name: "Work".into(),
            color: "#3584e4".into(),
            writable: true,
        };
        let ev = SystemEvent {
            uid: "android-7-3600000".into(),
            start: 3600,
            end: 7200,
            all_day: false,
            title: "One".into(),
            description: String::new(),
            location: String::new(),
            color: String::new(),
        };
        c.set_system_calendars(id, vec![cal]).unwrap();
        c.set_system_events(href.clone(), vec![ev.clone()]).unwrap();
        assert!(c.system_changes().unwrap().is_empty());

        // Edited: the change waits, and no server is told.
        let patch = crate::EventPatch {
            summary: Some("One, moved".into()),
            ..Default::default()
        };
        c.update_event(href.clone(), ev.uid.clone(), None, crate::Scope::All, patch)
            .unwrap();
        assert!(!c.pending(None));
        let changes = c.system_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert!(!changes[0].deleted);
        assert_eq!(changes[0].event.uid, ev.uid);
        assert_eq!(changes[0].event.title, "One, moved");
        assert_eq!((changes[0].event.start, changes[0].event.end), (3600, 7200));

        // Added: a UID of Todav's own.
        let time = crate::EventTime {
            start: 10_000,
            end: 13_600,
            all_day: false,
        };
        let new = c
            .add_event(href.clone(), "New".into(), time, vec![])
            .unwrap();
        assert_eq!(c.system_changes().unwrap().len(), 2);

        // Made in the system's store: both copies go, the system's own come back.
        for ch in c.system_changes().unwrap() {
            c.system_applied(ch.list_href, ch.event.uid).unwrap();
        }
        assert!(c.system_changes().unwrap().is_empty());
        assert!(c.event(new).is_none());
        let moved = SystemEvent {
            title: "One, moved".into(),
            ..ev.clone()
        };
        c.set_system_events(href.clone(), vec![moved]).unwrap();
        assert_eq!(c.event(ev.uid.clone()).unwrap().summary, "One, moved");

        // The calendar itself is the account's: it is turned off, not deleted here.
        assert!(matches!(c.delete_list(href.clone()), Err(Error::ReadOnly)));
        assert_eq!(c.calendars().len(), 1);

        // Deleted.
        c.delete_event(href.clone(), ev.uid.clone(), None, crate::Scope::All)
            .unwrap();
        let changes = c.system_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert!(changes[0].deleted);
        assert_eq!(changes[0].event.uid, ev.uid);
        c.system_applied(href, ev.uid.clone()).unwrap();
        assert!(c.system_changes().unwrap().is_empty());
        assert!(c.event(ev.uid).is_none());
    }
}
