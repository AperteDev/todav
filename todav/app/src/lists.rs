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

/// The accounts new lists can go to now: switched on, and connected for a Nextcloud account
/// (Google's calendars are made in Google Calendar).
pub fn places(c: &Client) -> Vec<i64> {
    c.accounts()
        .into_iter()
        .filter(|a| match a.kind {
            AccountKind::Local => a.enabled,
            AccountKind::Nextcloud => a.enabled && c.connected(a.id),
            AccountKind::Google => false,
        })
        .map(|a| a.id)
        .collect()
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
