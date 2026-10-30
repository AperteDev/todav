//! Task lists and calendars: their colours and their order by account.

use todav_core::{AccountKind, Client, List, Result, TaskPatch};

/// Nextcloud's colour presets (its colour picker's palette), in its order.
pub const PRESETS: [&str; 12] = [
    "#B6469D", "#BF678B", "#C98879", "#D3A967", "#DDCB55", "#A5B872", "#6EA68F", "#3794AC",
    "#0082C9", "#2D73BE", "#5B64B3", "#8855A8",
];

/// A list's colour from the server when it is one ("#rrggbb", maybe with alpha), else blue.
pub fn color(l: &List) -> String {
    l.color
        .as_deref()
        .filter(|c| {
            c.len() <= 9 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
        })
        .unwrap_or("#3584e4")
        .to_string()
}

/// Where account `id` comes among the accounts (the user's order), for sorting.
pub fn rank(c: &Client) -> impl Fn(i64) -> usize + use<> {
    let ids: Vec<i64> = c.accounts().iter().map(|a| a.id).collect();
    move |id| ids.iter().position(|i| *i == id).unwrap_or(usize::MAX)
}

/// Lists grouped by account, in the user's order of the accounts, each group in the order
/// given.
pub fn by_place(c: &Client, mut lists: Vec<List>) -> Vec<List> {
    let rank = rank(c);
    lists.sort_by_key(|l| rank(l.account));
    lists
}

/// The accounts new task lists, or calendars (`events`), can go to now: switched on, and
/// connected for a Nextcloud or Google account (Google's calendars are made in Google
/// Calendar, the system's where they come from).
pub fn places(c: &Client, events: bool) -> Vec<i64> {
    c.accounts()
        .into_iter()
        .filter(|a| match a.kind {
            AccountKind::Local => a.enabled,
            AccountKind::Nextcloud => a.enabled && c.connected(a.id),
            AccountKind::Google => !events && a.enabled && c.connected(a.id),
            AccountKind::System => false,
        })
        .map(|a| a.id)
        .collect()
}

/// The task lists a task for an event can go to, by place: ones we may write to, but not
/// Google Tasks', which keep no link to the event.
pub fn event_task_lists(c: &Client) -> Vec<List> {
    let lists = c.lists().into_iter();
    by_place(
        c,
        lists
            .filter(|l| l.writable && !todav_core::gtasks::is_list(&l.href))
            .collect(),
    )
}

/// Where a task added for an event of `calendar` goes: for a read-only calendar the list picked
/// in `event_tasks_list`; else, or if that is gone, the last opened list; else the first.
pub fn event_task_list(c: &Client, calendar: &str) -> Option<String> {
    let lists = event_task_lists(c);
    let read_only = !c
        .calendars()
        .iter()
        .any(|l| l.href == calendar && l.writable);
    let picked = read_only
        .then(|| c.setting("event_tasks_list".into()))
        .flatten();
    [picked, c.setting("last_list".into())]
        .into_iter()
        .flatten()
        .find(|h| lists.iter().any(|l| &l.href == h))
        .or_else(|| lists.first().map(|l| l.href.clone()))
}

/// The account heading each of `lists` (as `by_place` orders them): on the first of a group,
/// when they are from more than one.
pub fn headings(lists: &[List]) -> Vec<Option<i64>> {
    let mixed = lists.iter().any(|l| l.account != lists[0].account);
    (0..lists.len())
        .map(|i| {
            let first = i == 0 || lists[i - 1].account != lists[i].account;
            (mixed && first).then_some(lists[i].account)
        })
        .collect()
}

/// What a new local account starts with, in the user's language.
pub struct Starter {
    /// The task list's name.
    pub tasks: String,
    /// Its task, and the task's note.
    pub task: String,
    pub note: String,
    /// The calendar's name.
    pub calendar: String,
}

/// A new local account's lists: a task list with a task to start from and an empty calendar.
pub fn start_local(c: &Client, account: i64, s: Starter) -> Result<()> {
    let list = c.create_list(account, s.tasks, PRESETS[8].into(), false)?;
    let uid = c.add_task(list, s.task, None, None, false)?.uid;
    let patch = TaskPatch {
        description: Some(s.note),
        ..Default::default()
    };
    c.update_task(uid, patch)?;
    c.create_list(account, s.calendar, PRESETS[0].into(), true)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use todav_core::SystemCalendar;

    #[test]
    fn a_read_only_calendars_event_tasks_go_to_the_picked_list() {
        let dir = std::env::temp_dir().join(format!("todav-app-event-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let add = |kind| {
            c.add_account(kind, "".into(), "".into(), "".into())
                .unwrap()
        };
        let (local, system) = (add(AccountKind::Local), add(AccountKind::System));
        let a = c
            .create_list(local, "A".into(), "#0082C9".into(), false)
            .unwrap();
        let b = c
            .create_list(local, "B".into(), "#0082C9".into(), false)
            .unwrap();
        let mine = c
            .create_list(local, "Mine".into(), "#B6469D".into(), true)
            .unwrap();
        let work = SystemCalendar {
            href: "system:work".into(),
            name: "Work".into(),
            color: "#B6469D".into(),
        };
        c.set_system_calendars(system, vec![work]).unwrap();
        let set = |k: &str, v: &str| c.set_setting(k.into(), v.into()).unwrap();

        // Nothing picked or opened: the first list.
        assert_eq!(event_task_list(&c, "system:work"), Some(a.clone()));
        set("last_list", &b);
        assert_eq!(event_task_list(&c, "system:work"), Some(b.clone()));
        // The picked list, for read-only calendars only.
        set("event_tasks_list", &a);
        assert_eq!(event_task_list(&c, "system:work"), Some(a.clone()));
        assert_eq!(event_task_list(&c, &mine), Some(b.clone()));
        // Gone: as if none was picked.
        set("event_tasks_list", "gone");
        assert_eq!(event_task_list(&c, "system:work"), Some(b));
    }
}
