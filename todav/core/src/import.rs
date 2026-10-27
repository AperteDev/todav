//! Importing an iCalendar file (an invitation, an export from another app): its events and tasks
//! go into the calendar and the task list picked for them.

use crate::ical::{Calendar, split};
use crate::store::{self, Kind};
use crate::{Client, Error, Result};

/// How many events and tasks an iCalendar file holds, or were imported from it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct IcsCount {
    pub events: u32,
    pub tasks: u32,
}

impl Client {
    /// What iCalendar text `ics` holds.
    pub fn ics_count(&self, ics: String) -> IcsCount {
        count(&split(&ics, crate::new_uid))
    }

    /// Import iCalendar text `ics`: its events into calendar `events_to`, its tasks into list
    /// `tasks_to` (None: those are left out). An item already in that calendar or list is
    /// replaced; a task with the same UID in another list comes as a new task. One undo step.
    pub fn import_ics(
        &self,
        ics: String,
        events_to: Option<String>,
        tasks_to: Option<String>,
    ) -> Result<IcsCount> {
        for to in events_to.iter().chain(&tasks_to) {
            self.writable(to)?;
        }
        let objects = split(&ics, crate::new_uid);
        let mut done = IcsCount::default();
        let mut changed: Vec<(String, String)> = Vec::new();
        self.together(|| -> Result<()> {
            let db = self.db.lock().unwrap();
            for mut cal in objects {
                let kind = Kind::of(&cal);
                let Some(to) = (if kind == Kind::Event {
                    &events_to
                } else {
                    &tasks_to
                }) else {
                    continue;
                };
                let mut uid = cal.text("UID").unwrap_or_default();
                if kind == Kind::Task && store::task_elsewhere(&db, &uid, to)? {
                    uid = crate::new_uid();
                    cal.set("UID", "", Some(&uid));
                }
                let here = store::raw(&db, kind, to, &uid)?;
                self.remember(&db, kind, to, &uid)?;
                let (href, etag) = here.map_or((None, None), |r| (r.href, r.etag));
                store::save(
                    &db,
                    to,
                    href.as_deref(),
                    etag.as_deref(),
                    &cal,
                    store::MODIFIED,
                )?;
                if kind == Kind::Event {
                    done.events += 1;
                } else {
                    done.tasks += 1;
                }
                changed.push((to.clone(), uid));
            }
            Ok(())
        })?;
        for (list, uid) in changed {
            self.emit(&list, vec![uid]);
        }
        if done == IcsCount::default() {
            return Err(Error::Parse("nothing to import".into()));
        }
        Ok(done)
    }
}

fn count(objects: &[Calendar]) -> IcsCount {
    let events = objects
        .iter()
        .filter(|c| Kind::of(c) == Kind::Event)
        .count();
    IcsCount {
        events: events as u32,
        tasks: (objects.len() - events) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AccountKind;

    const FILE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Other//EN\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Warsaw\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nDTSTART:19961027T030000\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nDTSTART:19810329T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:standup\r\nDTSTART;TZID=Europe/Warsaw:20261019T090000\r\nDTEND;TZID=Europe/Warsaw:20261019T093000\r\nRRULE:FREQ=WEEKLY\r\nSUMMARY:Standup\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:standup\r\nRECURRENCE-ID;TZID=Europe/Warsaw:20261102T090000\r\nDTSTART;TZID=Europe/Warsaw:20261102T100000\r\nDTEND;TZID=Europe/Warsaw:20261102T103000\r\nSUMMARY:Standup, later\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261030\r\nSUMMARY:Trip\r\nEND:VEVENT\r\nBEGIN:VTODO\r\nUID:pack\r\nSUMMARY:Pack\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";

    #[test]
    fn imports_events_and_tasks_once_each() {
        let dir = std::env::temp_dir().join(format!("todav-test-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let here = c
            .add_account(AccountKind::Local, "".into(), "".into(), "".into())
            .unwrap();
        let cal = c
            .create_list(here, "Cal".into(), "#000".into(), true)
            .unwrap();
        let list = c
            .create_list(here, "Tasks".into(), "#000".into(), false)
            .unwrap();
        assert_eq!(
            c.ics_count(FILE.into()),
            IcsCount {
                events: 2,
                tasks: 1
            }
        );

        // The repeating event with its override is one event; the trip gets a UID.
        let n = c
            .import_ics(FILE.into(), Some(cal.clone()), Some(list.clone()))
            .unwrap();
        assert_eq!((n.events, n.tasks), (2, 1));
        let raw = c.db.lock().unwrap();
        let standup = store::raw(&raw, Kind::Event, &cal, "standup")
            .unwrap()
            .unwrap();
        assert!(standup.raw_ics.contains("RECURRENCE-ID") && standup.raw_ics.contains("VTIMEZONE"));
        drop(raw);
        assert_eq!(c.tasks(list.clone(), true).len(), 1);

        // Again: replaced, not doubled; tasks left out when no list is given.
        let n = c.import_ics(FILE.into(), Some(cal.clone()), None).unwrap();
        assert_eq!((n.events, n.tasks), (2, 0));
        let rows: i64 =
            c.db.lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM events WHERE list_href = ?",
                    [&cal],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(
            rows, 3,
            "standup and the trip once, the trip again with a new UID"
        );
        assert_eq!(c.tasks(list, true).len(), 1);
        assert!(matches!(
            c.import_ics("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n".into(), None, None),
            Err(Error::Parse(_))
        ));
    }
}
