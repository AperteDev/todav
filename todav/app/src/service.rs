//! The background service every front-end runs: a sync worker taking requests from any
//! thread, the ntfy push listener (where the platform has no push service of its own), when to
//! sync without being asked, and which reminders went off.

use crate::accounts::syncing;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use todav_core::{ChangeListener, Client, Event, Refused, SyncReport, Task};

/// What syncing is doing.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Syncing,
    Failed(Vec<Failure>),
}

/// Why a sync failed.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    /// The account it failed for; None: not one account's (e.g. the push listener's).
    pub account: Option<i64>,
    pub error: String,
}

/// The front-end's side: told from the service's threads, so it hands these to its UI thread.
pub trait Shell: Send + Sync {
    fn state(&self, state: State);
    /// Items the server refused in a sync that otherwise went through.
    fn refused(&self, items: Vec<Refused>);
}

pub struct Service {
    client: Arc<Client>,
    shell: Box<dyn Shell>,
    worker: OnceLock<Sender<()>>,
    state: Mutex<State>,
    /// Why each account's last sync failed, by id; gone once one goes through.
    errors: Mutex<Option<HashMap<i64, String>>>,
    /// The ntfy server the push listener was started for.
    push_url: Mutex<String>,
    /// Started with the push listener (`start(listen)`); else pushes come through the platform.
    listens: std::sync::atomic::AtomicBool,
    /// What the push listener last failed with; cleared by its next success.
    push_error: Mutex<Option<String>>,
    /// Minutes `tick` has counted.
    ticks: AtomicU32,
}

impl Service {
    pub fn new(client: Arc<Client>, shell: Box<dyn Shell>) -> Arc<Service> {
        let service = Arc::new(Service {
            client,
            shell,
            worker: OnceLock::new(),
            state: Mutex::new(State::Idle),
            errors: Mutex::new(None),
            push_url: Mutex::new(String::new()),
            listens: std::sync::atomic::AtomicBool::new(false),
            push_error: Mutex::new(None),
            ticks: AtomicU32::new(0),
        });
        let settings = Settings(Arc::downgrade(&service));
        service.client.subscribe(Box::new(settings));
        service
    }

    /// `start` ran.
    pub fn running(&self) -> bool {
        self.worker.get().is_some()
    }

    /// Start the sync worker and (`listen`) the push listener, and sync; true the first time.
    /// Later calls just sync. Without `listen`, pushes come through the platform
    /// (`Client::push_decode`, `Client::sync_topic`).
    pub fn start(self: &Arc<Self>, listen: bool) -> bool {
        if self.running() {
            self.sync_now();
            return false;
        }
        self.listens.store(listen, Ordering::Relaxed);
        let (tx, rx) = channel::<()>();
        let _ = self.worker.set(tx);
        let me = self.clone();
        std::thread::spawn(move || {
            while rx.recv().is_ok() {
                std::thread::sleep(Duration::from_millis(300)); // let a burst of edits settle
                while rx.try_recv().is_ok() {}
                if !me.can_sync() {
                    continue; // no account, or no password for one: nothing to sync until one connects
                }
                me.set_state(State::Syncing);
                let report = me.client.sync();
                *me.errors.lock().unwrap() = Some(
                    report
                        .failed
                        .iter()
                        .map(|(id, e)| (*id, e.to_string()))
                        .collect(),
                );
                me.report(Ok(report));
            }
        });
        self.sync_now();
        if listen {
            self.restart_push();
        }
        true
    }

    /// Ask the worker for a full sync; requests arriving while one runs are coalesced.
    pub fn sync_now(&self) {
        if let Some(tx) = self.worker.get() {
            let _ = tx.send(());
        }
    }

    /// After an edit: sync if it left something for the server (edits in lists on this device
    /// leave nothing, so no sync runs for them).
    pub fn request_sync(&self) {
        if self.client.pending(None) {
            self.sync_now();
        }
    }

    /// Some account syncs: the sync buttons work.
    pub fn can_sync(&self) -> bool {
        self.client
            .accounts()
            .iter()
            .any(|a| syncing(&self.client, a))
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    /// Why account `id`'s last sync failed.
    pub fn sync_error(&self, id: i64) -> Option<String> {
        self.errors.lock().unwrap().as_ref()?.get(&id).cloned()
    }

    pub fn push_error(&self) -> Option<String> {
        self.push_error.lock().unwrap().clone()
    }

    fn set_state(&self, s: State) {
        *self.state.lock().unwrap() = s.clone();
        self.shell.state(s);
    }

    /// What a sync or a push did: items the server refused, and the state.
    fn report(&self, r: todav_core::Result<SyncReport>) {
        let report = match r {
            Ok(r) => r,
            Err(e) => {
                let error = e.to_string();
                return self.set_state(State::Failed(vec![Failure {
                    account: None,
                    error,
                }]));
            }
        };
        if !report.refused.is_empty() {
            self.shell.refused(report.refused);
        }
        self.set_state(if report.failed.is_empty() {
            State::Idle
        } else {
            State::Failed(
                report
                    .failed
                    .iter()
                    .map(|(id, e)| Failure {
                        account: Some(*id),
                        error: e.to_string(),
                    })
                    .collect(),
            )
        });
    }

    /// Stop the push listener and start a new one if an ntfy server is set (`ntfy_url`).
    pub fn restart_push(self: &Arc<Self>) {
        self.client.stop_listening();
        let url = self.client.setting("ntfy_url".into()).unwrap_or_default();
        *self.push_url.lock().unwrap() = url.clone();
        *self.push_error.lock().unwrap() = None;
        if url.is_empty() || !self.can_sync() {
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || {
            // Blocks until the next restart_push; reconnects internally.
            let _ = me.client.listen(url, &|r| {
                *me.push_error.lock().unwrap() = r.as_ref().err().map(|e| e.to_string());
                me.report(r);
            });
        });
    }

    /// Synced settings arrived: a new ntfy server is listened to at once (by a service that
    /// listens; on Android pushes come through UnifiedPush, whose registration a listener's
    /// would replace).
    fn settings_changed(self: &Arc<Self>) {
        let ntfy = self.client.setting("ntfy_url".into()).unwrap_or_default();
        let listens = self.listens.load(Ordering::Relaxed);
        if self.running() && listens && ntfy != *self.push_url.lock().unwrap() {
            self.restart_push();
        }
    }

    /// An account came or got its password: sync it, and register push for its lists now
    /// rather than within the hour.
    pub fn account_added(self: &Arc<Self>, listen: bool) {
        if !self.start(listen) && listen {
            self.restart_push();
        }
    }

    /// Account `id` is gone: its errors with it; sync the rest.
    pub fn account_removed(&self, id: i64) {
        if let Some(errors) = self.errors.lock().unwrap().as_mut() {
            errors.remove(&id);
        }
        if !self.can_sync() {
            self.set_state(State::Idle); // no sync left to fail
        } else {
            self.sync_now();
        }
    }

    /// Call once a minute: the safety net for lost pushes syncs every minute while the app is
    /// in front (`active`), every 30 minutes otherwise.
    pub fn tick(&self, active: bool) {
        let ticks = self.ticks.fetch_add(1, Ordering::Relaxed) + 1;
        if active || ticks.is_multiple_of(30) {
            self.sync_now();
        }
    }

    /// Reminders of tasks and events that went off since the last call (`due_reminders`).
    pub fn due_reminders(&self) -> (Vec<Task>, Vec<Event>) {
        due_reminders(&self.client)
    }
}

/// Reminders of tasks and events that went off since the last call: at most a day back, so a
/// device that was off does not replay old ones. The first call only marks the time. Tasks a
/// change gave a reminder already past come too (`Client::take_late_reminders`).
pub fn due_reminders(c: &Client) -> (Vec<Task>, Vec<Event>) {
    let now = c.local_now();
    let stored = c
        .setting("reminded_until".into())
        .and_then(|v| v.parse::<i64>().ok());
    let from = stored.unwrap_or(now).max(now - 86400);
    let mut tasks = c.reminders(from, now);
    for t in c.take_late_reminders() {
        if !tasks.iter().any(|x| x.uid == t.uid) {
            tasks.push(t);
        }
    }
    let due = (tasks, c.event_reminders(from, now));
    if stored != Some(now) {
        let _ = c.set_setting("reminded_until".into(), now.to_string());
    }
    due
}

/// Whether the reminder of `t` that went off by `now` was for its start (the latest of them up
/// to then counts from it), so its notification names the start rather than the due date.
pub fn reminds_of_start(t: &Task, now: i64) -> bool {
    let last = t.reminders.iter().filter(|r| **r <= now).max();
    last.is_some_and(|r| t.start_reminders.contains(r))
}

/// How far ahead `next_reminder` looks. A platform that sets an alarm for it sets it again after
/// a sync or a change, and at least this often.
pub const LOOKAHEAD: i64 = 7 * 86400;

/// When the next reminder of a task or event goes off after `now` (local time), within
/// `LOOKAHEAD`: for a platform that wakes up for it (an exact alarm on Android) instead of
/// checking every 30 seconds. Then `Service::due_reminders` says which went off. Now, when a
/// change set a reminder already past.
pub fn next_reminder(c: &Client, now: i64) -> Option<i64> {
    if c.has_late_reminders() {
        return Some(now);
    }
    let to = now + LOOKAHEAD;
    let tasks = c.reminders(now, to).into_iter().flat_map(|t| t.reminders);
    let events = c.event_reminders(now, to).into_iter();
    let events = events.flat_map(|e| e.reminders);
    tasks.chain(events).filter(|&r| r > now && r <= to).min()
}

/// Tells the service about settings other devices changed.
struct Settings(Weak<Service>);

impl ChangeListener for Settings {
    fn changed(&self, _list_href: String, _uids: Vec<String>) {}

    fn settings_changed(&self) {
        if let Some(s) = self.0.upgrade() {
            s.settings_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use todav_core::{AccountKind, Due, EventTime, TaskPatch};

    #[test]
    fn the_next_reminder_is_the_soonest_of_tasks_and_events() {
        let dir = std::env::temp_dir().join(format!("todav-app-{}", std::process::id()));
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let account = c
            .add_account(
                AccountKind::Local,
                String::new(),
                String::new(),
                String::new(),
            )
            .unwrap();
        let list = c.create_list(account, "Tasks".into(), "#0082C9".into(), false);
        let cal = c.create_list(account, "Calendar".into(), "#B6469D".into(), true);
        let now = c.local_now();
        assert_eq!(next_reminder(&c, now), None);

        // A task due in two hours, reminded an hour before.
        let task = c.add_task(list.unwrap(), "Call".into(), None, None, false);
        let patch = TaskPatch {
            due: Some(Some(Due {
                at: now + 7200,
                all_day: false,
            })),
            reminders: Some(vec![-3600]),
            ..Default::default()
        };
        c.update_task(task.unwrap().uid, patch).unwrap();
        assert_eq!(next_reminder(&c, now), Some(now + 3600));

        // An event in 40 minutes, reminded 15 minutes before: sooner.
        let time = EventTime {
            start: now + 2400,
            end: now + 6000,
            all_day: false,
        };
        c.add_event(cal.unwrap(), "Meet".into(), time, vec![-900])
            .unwrap();
        assert_eq!(next_reminder(&c, now), Some(now + 1500));
        // Past it, the task's comes next; past both, none within the week.
        assert_eq!(next_reminder(&c, now + 1500), Some(now + 3600));
        assert_eq!(next_reminder(&c, now + 3600), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reminders_go_off_once_from_the_first_call() {
        let dir = std::env::temp_dir().join(format!("todav-app-due-{}", std::process::id()));
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let account = c
            .add_account(
                AccountKind::Local,
                String::new(),
                String::new(),
                String::new(),
            )
            .unwrap();
        let list = c
            .create_list(account, "Tasks".into(), "#0082C9".into(), false)
            .unwrap();
        // The first call marks the time, even with nothing due.
        assert!(due_reminders(&c).0.is_empty());
        let now = c.local_now();
        assert!(c.setting("reminded_until".into()).is_some());
        // A reminder after that goes off at the next call, and only then.
        c.set_setting("reminded_until".into(), (now - 100).to_string())
            .unwrap();
        let task = c.add_task(list, "Call".into(), None, None, false).unwrap();
        let patch = TaskPatch {
            due: Some(Some(Due {
                at: now - 50,
                all_day: false,
            })),
            reminders: Some(vec![0]),
            ..Default::default()
        };
        c.update_task(task.uid, patch).unwrap();
        assert_eq!(due_reminders(&c).0.len(), 1);
        assert!(due_reminders(&c).0.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_reminder_set_in_the_past_goes_off_at_once_while_the_task_is_ahead() {
        let dir = std::env::temp_dir().join(format!("todav-app-late-{}", std::process::id()));
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let account = c
            .add_account(
                AccountKind::Local,
                String::new(),
                String::new(),
                String::new(),
            )
            .unwrap();
        let list = c
            .create_list(account, "Tasks".into(), "#0082C9".into(), false)
            .unwrap();
        assert!(due_reminders(&c).0.is_empty());
        let now = c.local_now();
        let due = |at, reminders| TaskPatch {
            due: Some(Some(Due { at, all_day: false })),
            reminders: Some(reminders),
            ..Default::default()
        };
        // Due in ten minutes, reminded fifteen before: at once, and only once.
        let task = c.add_task(list.clone(), "Call".into(), None, None, false);
        let uid = task.unwrap().uid;
        c.update_task(uid.clone(), due(now + 600, vec![-900]))
            .unwrap();
        assert_eq!(next_reminder(&c, now), Some(now));
        assert_eq!(due_reminders(&c).0.len(), 1);
        assert!(due_reminders(&c).0.is_empty());
        // Another reminder: only a new one that is past counts, the old one went off already.
        c.update_task(uid.clone(), due(now + 600, vec![-900, 0]))
            .unwrap();
        assert!(!c.has_late_reminders());
        // A task already due is not reminded of it again.
        let task = c.add_task(list, "Late".into(), None, None, false);
        c.update_task(task.unwrap().uid, due(now - 600, vec![-900]))
            .unwrap();
        assert!(!c.has_late_reminders());
        let _ = std::fs::remove_dir_all(dir);
    }

    struct NoShell;

    impl Shell for NoShell {
        fn state(&self, _: State) {}
        fn refused(&self, _: Vec<todav_core::Refused>) {}
    }

    #[test]
    fn a_service_without_the_listener_does_not_start_it_for_synced_settings() {
        let dir = std::env::temp_dir().join(format!("todav-app-listen-{}", std::process::id()));
        let c = Arc::new(Client::open(dir.to_string_lossy().into()).unwrap());
        let s = Service::new(c.clone(), Box::new(NoShell));
        s.start(false);
        c.set_setting("ntfy_url".into(), "https://ntfy.example".into())
            .unwrap();
        s.settings_changed();
        assert!(s.push_url.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
