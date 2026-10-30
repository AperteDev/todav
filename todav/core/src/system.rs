//! The system's calendars (Android's calendar store, filled by the device's accounts: Google,
//! Exchange, DAVx⁵…), read-only. The front-end reads the calendars the user turned on and hands
//! their occurrences over; each is stored as an event of its own, repeats, exceptions and time
//! zones already worked out by the system.

use crate::ical::{escape, fmt_utc};
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
}

impl Client {
    /// The system account's calendars are these now (read-only); others it had go.
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
                     VALUES (?1, ?2, ?3, 0, 1, 0, ?4)
                     ON CONFLICT(href) DO UPDATE SET display_name = ?2, color = ?3",
                    params![c.href, c.name, c.color, account],
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
    for (name, v) in [("LOCATION", &e.location), ("DESCRIPTION", &e.description)] {
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
}
