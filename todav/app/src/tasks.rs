//! Task lists as the app shows them: trees of subtasks, the pages above the lists, and what
//! dragging a task somewhere does. Each drop is one change for undo.

use crate::prefs;
use std::collections::{HashMap, HashSet};
use todav_core::{Client, Result, Sort, Task, TaskPatch};

/// Pages shown above the lists while they have something.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Page {
    /// Open tasks with a due date in every list.
    Upcoming,
    /// Open tasks of high priority in every list (with stars on).
    Important,
}

/// The pages shown, with their counts: Upcoming while a task has a due date (counting those due
/// today or earlier), Important while one is of high priority. None without lists.
pub fn pages(c: &Client) -> Vec<(Page, usize)> {
    if c.lists().is_empty() {
        return Vec::new();
    }
    let now = c.local_now();
    let upcoming = c.upcoming();
    let important = if prefs::stars(c) {
        c.important().len()
    } else {
        0
    };
    let pressing = upcoming
        .iter()
        .filter(|t| crate::dates::pressing(t, now))
        .count();
    [
        (Page::Upcoming, !upcoming.is_empty(), pressing),
        (Page::Important, important > 0, important),
    ]
    .into_iter()
    .filter(|(_, shown, _)| *shown)
    .map(|(page, _, n)| (page, n))
    .collect()
}

/// Split a group's depth-first task list into root tasks, each with its (depth, subtask) list.
pub fn trees(tasks: &[Task]) -> Vec<(&Task, Vec<(usize, &Task)>)> {
    let mut depth = HashMap::new();
    let mut out: Vec<(&Task, Vec<(usize, &Task)>)> = Vec::new();
    for t in tasks {
        match t.parent_uid.as_ref().and_then(|p| depth.get(p).copied()) {
            Some(d) => {
                depth.insert(&t.uid, d + 1);
                out.last_mut().unwrap().1.push((d + 1, t)); // a parent always precedes its subtasks
            }
            None => {
                depth.insert(&t.uid, 0usize);
                out.push((t, Vec::new()));
            }
        }
    }
    out
}

/// (open, done) direct subtasks of each task, hidden ones included; theirs count on their own row.
pub fn subtask_counts(tasks: &[Task]) -> HashMap<String, (usize, usize)> {
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    for t in tasks {
        if let Some(parent) = &t.parent_uid {
            let c = counts.entry(parent.clone()).or_default();
            if t.done {
                c.1 += 1;
            } else {
                c.0 += 1;
            }
        }
    }
    counts
}

/// Where the "Add subtask" entry of subtask `uid` goes in a tree's depth-first `subs`: (the
/// row it goes in front of, `subs.len()` for last; `uid`'s index). First among its subtasks
/// with `start`, otherwise after them.
pub fn entry_at(subs: &[(usize, &Task)], uid: &str, start: bool) -> Option<(usize, usize)> {
    let p = subs.iter().position(|(_, t)| t.uid == uid)?;
    let below = subs[p + 1..].iter().take_while(|(d, _)| *d > subs[p].0);
    Some((if start { p + 1 } else { p + 1 + below.count() }, p))
}

/// Where a task's row sits, for drag and drop.
#[derive(Clone, Debug, Default)]
pub struct Slot {
    /// Category group the row is shown in; None is "Other". In the Pinned group: its own
    /// category.
    pub group: Option<String>,
    /// In the Pinned group (top-level tasks pinned to the top of the list) or under one there.
    pub pinned: bool,
    /// A top-level row (otherwise a subtask, movable only among its siblings).
    pub root: bool,
    /// The row after it that a drop below lands in front of; None means last.
    pub next: Option<String>,
}

/// `d` can become a subtask of `parent`, its own subtasks coming along, unless it already is
/// one or `parent` is in its tree.
pub fn can_adopt(c: &Client, d: &Task, parent: &Task) -> bool {
    d.parent_uid.as_ref() != Some(&parent.uid) && !c.within(parent.uid.clone(), d.uid.clone())
}

/// `d` can be dropped on `parent`'s "Add subtask" entry: as `can_adopt`, or to move to the end
/// of its siblings in manual order.
pub fn can_join(c: &Client, d: &Task, parent: &Task) -> bool {
    let manual = prefs::order(c).subtasks == Sort::Manual;
    (manual || d.parent_uid.as_ref() != Some(&parent.uid))
        && !c.within(parent.uid.clone(), d.uid.clone())
}

/// Make `d` a subtask of `parent` (if it is not one yet), first or last among its subtasks as
/// new subtasks would be.
pub fn adopt(c: &Client, d: &Task, parent: &Task) -> Result<()> {
    c.together(|| {
        if d.parent_uid.as_ref() != Some(&parent.uid) {
            c.set_parent(d.uid.clone(), parent.uid.clone())?;
            if d.category.is_some() {
                set_category(c, &d.uid, None)?; // the top-level task's category decides
            }
        }
        let order = prefs::order(c);
        if order.subtasks == Sort::Manual {
            let first = c
                .tasks(parent.list_href.clone(), true)
                .into_iter()
                .find(|t| t.parent_uid.as_ref() == Some(&parent.uid) && t.uid != d.uid);
            match first {
                Some(f) if order.sub_start => c.reorder(d.uid.clone(), Some(f.uid))?,
                Some(_) => c.reorder(d.uid.clone(), None)?,
                None => {} // the only subtask
            }
        }
        Ok(())
    })
}

/// Would dropping `d` above or below `t` leave it where it is? Then no line is drawn there.
pub fn stays_put(d: &Task, ds: &Slot, t: &Task, ts: &Slot, above: bool) -> bool {
    let before = if above {
        Some(&t.uid)
    } else {
        ts.next.as_ref()
    };
    ds.root == ts.root
        && ds.group == ts.group
        && ds.pinned == ts.pinned
        && d.parent_uid == t.parent_uid
        && (before == Some(&d.uid) || before == ds.next.as_ref())
}

/// Top-level tasks move within and between groups (changing category; into Pinned and out of it,
/// pinning and unpinning); a subtask dropped among them is promoted. A task dropped among
/// subtasks joins them under their parent, with its own subtasks, unless that is inside its
/// tree. Reordering only means something in manual order.
pub fn can_drop(c: &Client, d: &Task, ds: &Slot, t: &Task, ts: &Slot) -> bool {
    let order = prefs::order(c);
    if d.uid == t.uid {
        return false;
    }
    if ts.root {
        let elsewhere = ds.group != ts.group || ds.pinned != ts.pinned;
        return !ds.root || elsewhere || order.tasks == Sort::Manual;
    }
    if d.parent_uid == t.parent_uid {
        return order.subtasks == Sort::Manual;
    }
    !c.within(t.uid.clone(), d.uid.clone())
}

/// Would dropping `d` beside `t` only reorder it, which a sorted list puts back? `can_drop`
/// refuses those; a front-end says why: the sort (of tasks or of subtasks) is not Manual.
pub fn needs_manual(c: &Client, d: &Task, ds: &Slot, t: &Task, ts: &Slot) -> bool {
    let order = prefs::order(c);
    if d.uid == t.uid {
        return false;
    }
    if ts.root {
        let elsewhere = ds.group != ts.group || ds.pinned != ts.pinned;
        return ds.root && !elsewhere && order.tasks != Sort::Manual;
    }
    d.parent_uid == t.parent_uid && order.subtasks != Sort::Manual
}

/// Move dragged task `d` above or below target `t`.
pub fn drop_task(c: &Client, d: &Task, ds: &Slot, t: &Task, ts: &Slot, above: bool) -> Result<()> {
    c.together(|| {
        let order = prefs::order(c);
        let promote = !ds.root && ts.root;
        if promote {
            c.promote(d.uid.clone())?;
        }
        if ts.root && d.pinned != ts.pinned {
            let patch = TaskPatch {
                pinned: Some(ts.pinned),
                ..Default::default()
            };
            c.update_task(d.uid.clone(), patch)?;
        }
        // Pinned tasks keep their category.
        if ts.root && !ts.pinned && (promote || ds.group != ts.group) {
            set_category(c, &d.uid, ts.group.clone())?;
        }
        if !ts.root && d.parent_uid != t.parent_uid {
            let parent = t.parent_uid.clone().unwrap_or_default();
            c.set_parent(d.uid.clone(), parent)?;
            if d.category.is_some() {
                set_category(c, &d.uid, None)?; // the new top-level task's category decides
            }
        }
        let sort = if ts.root { order.tasks } else { order.subtasks };
        let mut before = if above {
            Some(t.uid.clone())
        } else {
            ts.next.clone()
        };
        // Completed tasks shown last sit anywhere in the stored order: an open task dropped in
        // front of them goes last, a completed one dropped among open tasks keeps its place.
        let view = prefs::done_view(c);
        let mut keep = false;
        if if ts.root {
            view.tasks_last
        } else {
            view.subtasks_last
        } {
            let done = before
                .clone()
                .and_then(|b| c.task(b))
                .is_some_and(|b| b.done);
            if done && !d.done {
                before = None;
            }
            keep = d.done && !done && before.is_some();
        }
        // Dropping right below the task above it leaves `d` where it is.
        if sort == Sort::Manual && !keep && before.as_ref() != Some(&d.uid) {
            c.reorder(d.uid.clone(), before)?;
        }
        Ok(())
    })
}

pub fn set_category(c: &Client, uid: &str, category: Option<String>) -> Result<()> {
    let patch = TaskPatch {
        category: Some(category.unwrap_or_default()),
        ..Default::default()
    };
    c.update_task(uid.into(), patch)
}

/// A task dropped on a category: into it (a subtask becomes top-level), first or last in it as
/// new tasks would be.
pub fn drop_on_category(c: &Client, d: &Task, ds: &Slot, category: Option<String>) -> Result<()> {
    c.together(|| {
        if !ds.root {
            c.promote(d.uid.clone())?;
        }
        set_category(c, &d.uid, category)?;
        let order = prefs::order(c);
        if order.tasks == Sort::Manual {
            // Before every task of the list puts it first in its category too.
            let first = c
                .tasks(d.list_href.clone(), true)
                .first()
                .map(|t| t.uid.clone());
            match first {
                _ if !order.task_start => c.reorder(d.uid.clone(), None)?,
                Some(f) if f != d.uid => c.reorder(d.uid.clone(), Some(f))?,
                _ => {} // already first
            }
        }
        Ok(())
    })
}

/// The subtasks of task `uid`, at any depth, that are `done` or open.
pub fn subtasks(c: &Client, uid: &str, done: bool) -> Vec<Task> {
    let Some(t) = c.task(uid.into()) else {
        return Vec::new();
    };
    let all = c.tasks(t.list_href, true);
    // Breadth-first; `seen` bounds it, as RELATED-TO from a server can form a cycle.
    let mut seen = HashSet::from([uid.to_string()]);
    let mut queue = vec![uid.to_string()];
    let mut found = Vec::new();
    while let Some(p) = queue.pop() {
        for k in all.iter().filter(|k| k.parent_uid.as_deref() == Some(&p)) {
            if seen.insert(k.uid.clone()) {
                queue.push(k.uid.clone());
                if k.done == done {
                    found.push(k.clone());
                }
            }
        }
    }
    found
}

/// Complete or reopen task `uid`, as one change for undo. With `subtasks`, its subtasks follow
/// (the open ones completed, or the completed ones reopened). With `parents`, reopening it
/// reopens its completed parents, so a completed task has no open subtasks
/// (`SubtaskCompletion::Complete`).
pub fn set_done(c: &Client, uid: &str, done: bool, subtasks: bool, parents: bool) -> Result<()> {
    let Some(t) = c.task(uid.into()) else {
        return Ok(());
    };
    let mut also: Vec<String> = Vec::new();
    if subtasks {
        also.extend(self::subtasks(c, uid, !done).into_iter().map(|t| t.uid));
    }
    if parents && !done {
        let mut seen = HashSet::from([t.uid.clone()]);
        let mut parent = t.parent_uid.clone();
        while let Some(p) = parent
            .and_then(|p| c.task(p))
            .filter(|p| seen.insert(p.uid.clone()))
        {
            if p.done {
                also.push(p.uid.clone());
            }
            parent = p.parent_uid;
        }
    }
    if t.done == done && also.is_empty() {
        return Ok(());
    }
    c.together(|| {
        if t.done != done {
            c.set_done(t.uid.clone(), done)?;
        }
        for u in also {
            c.set_done(u, done)?;
        }
        Ok(())
    })
}

/// Complete or reopen task `uid` where nobody can be asked about its subtasks (a reminder's
/// "Mark as Done", the extension, a widget): they follow only as Preferences complete them
/// (`SubtaskCompletion::Complete`, its parents reopening with it too); else they stay as they are.
pub fn set_done_unasked(c: &Client, uid: &str, done: bool) -> Result<()> {
    let tree = prefs::subtask_completion(c) == prefs::SubtaskCompletion::Complete;
    set_done(c, uid, done, done && tree, tree)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn task(uid: &str, parent: Option<&str>) -> Task {
        Task {
            uid: uid.into(),
            list_href: String::new(),
            summary: uid.into(),
            description: None,
            status: String::new(),
            done: false,
            completed_at: None,
            category: None,
            parent_uid: parent.map(Into::into),
            priority: None,
            due: None,
            due_all_day: false,
            reminders: Vec::new(),
            sort_order: None,
            created: None,
            last_modified: None,
            start: None,
            start_all_day: false,
            event_uid: None,
            sync_error: None,
            pinned: false,
            start_reminders: Vec::new(),
        }
    }

    #[test]
    fn counts_open_and_done_direct_subtasks() {
        let done = |uid, parent| Task {
            done: true,
            ..task(uid, parent)
        };
        let tasks = [
            task("a", None),
            task("a1", Some("a")),
            done("a1x", Some("a1")),
            task("a1y", Some("a1")),
            done("a2", Some("a")),
            task("b", None),
        ];
        let c = subtask_counts(&tasks);
        assert_eq!(c.get("a"), Some(&(1, 1)), "a1x and a1y count on a1 only");
        assert_eq!(c.get("a1"), Some(&(1, 1)));
        assert_eq!(c.get("b"), None);
    }

    #[test]
    fn trees_nest_depth_first_lists() {
        let tasks = [
            task("a", None),
            task("a1", Some("a")),
            task("a1x", Some("a1")),
            task("a2", Some("a")),
            task("b", None),
            task("c", Some("gone")), // parent closed or deleted: shown as a root
        ];
        let got: Vec<(&str, Vec<(usize, &str)>)> = trees(&tasks)
            .into_iter()
            .map(|(r, s)| {
                (
                    r.uid.as_str(),
                    s.iter().map(|(d, t)| (*d, t.uid.as_str())).collect(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("a", vec![(1, "a1"), (2, "a1x"), (1, "a2")]),
                ("b", vec![]),
                ("c", vec![]),
            ]
        );
    }

    #[test]
    fn a_subtasks_entry_goes_first_among_its_subtasks_or_after_them() {
        let (a1, a1x, a1xy, a2) = (
            task("a1", Some("a")),
            task("a1x", Some("a1")),
            task("a1xy", Some("a1x")),
            task("a2", Some("a")),
        );
        let subs = [(1, &a1), (2, &a1x), (3, &a1xy), (1, &a2)];
        assert_eq!(entry_at(&subs, "a1", true), Some((1, 0)));
        assert_eq!(
            entry_at(&subs, "a1", false),
            Some((3, 0)),
            "after a1x and a1xy"
        );
        assert_eq!(
            entry_at(&subs, "a1xy", false),
            Some((3, 2)),
            "no subtasks yet"
        );
        assert_eq!(entry_at(&subs, "a2", false), Some((4, 3)), "last");
        assert_eq!(entry_at(&subs, "b", false), None, "another tree");
    }

    #[test]
    fn reordering_siblings_needs_manual_sort_moving_elsewhere_does_not() {
        let dir = std::env::temp_dir().join(format!("todav-app-manual-{}", std::process::id()));
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let slot = |group: Option<&str>| Slot {
            group: group.map(Into::into),
            pinned: false,
            root: true,
            next: None,
        };
        let (a, b) = (task("a", None), task("b", None));
        c.set_setting("task_sort".into(), "due_soonest".into())
            .unwrap();
        assert!(needs_manual(&c, &a, &slot(None), &b, &slot(None)));
        assert!(!can_drop(&c, &a, &slot(None), &b, &slot(None)));
        assert!(
            !needs_manual(&c, &a, &slot(None), &b, &slot(Some("Work"))),
            "another category"
        );
        c.set_setting("task_sort".into(), "manual".into()).unwrap();
        assert!(!needs_manual(&c, &a, &slot(None), &b, &slot(None)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn completing_and_reopening_take_subtasks_and_parents_along_when_asked() {
        use todav_core::AccountKind;
        let dir = std::env::temp_dir().join(format!("todav-app-tree-{}", std::process::id()));
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
        let add = |name: &str, parent: Option<&str>| {
            let t = c.add_task(
                list.clone(),
                name.into(),
                None,
                parent.map(Into::into),
                false,
            );
            t.unwrap().uid
        };
        let a = add("a", None);
        let a1 = add("a1", Some(&a));
        let a1x = add("a1x", Some(&a1));
        let a2 = add("a2", Some(&a));
        let done = |uid: &str| c.task(uid.into()).unwrap().done;
        c.set_done(a2.clone(), true).unwrap();
        assert_eq!(
            subtasks(&c, &a, false).len(),
            2,
            "a1 and a1x, not the done a2"
        );
        assert_eq!(subtasks(&c, &a, true).len(), 1, "a2");

        set_done(&c, &a, true, false, false).unwrap();
        assert!(done(&a) && !done(&a1) && !done(&a1x), "alone");
        c.undo(None).unwrap();

        set_done(&c, &a, true, true, false).unwrap();
        assert!(done(&a) && done(&a1) && done(&a1x) && done(&a2));
        c.undo(None).unwrap();
        assert!(
            !done(&a) && !done(&a1) && !done(&a1x),
            "one undo reverts it all"
        );
        assert!(done(&a2), "done before");

        // Reopening a subtask reopens its parents, not its siblings.
        set_done(&c, &a, true, true, false).unwrap();
        set_done(&c, &a1x, false, false, true).unwrap();
        assert!(
            !done(&a1x) && !done(&a1) && !done(&a),
            "its parents reopened"
        );
        assert!(done(&a2), "a sibling stays done");

        // Reopening a task alone, or with its completed subtasks.
        set_done(&c, &a, true, true, false).unwrap();
        set_done(&c, &a, false, false, true).unwrap();
        assert!(!done(&a) && done(&a1) && done(&a1x) && done(&a2), "alone");
        set_done(&c, &a, true, false, false).unwrap();
        set_done(&c, &a, false, true, true).unwrap();
        assert!(
            !done(&a) && !done(&a1) && !done(&a1x) && !done(&a2),
            "all of them"
        );

        // Nobody asked: as Preferences complete subtasks (by default, all along).
        set_done_unasked(&c, &a, true).unwrap();
        assert!(done(&a) && done(&a1) && done(&a1x) && done(&a2));
        set_done_unasked(&c, &a1x, false).unwrap();
        assert!(
            !done(&a1x) && !done(&a1) && !done(&a),
            "its parents reopened"
        );
        for mode in ["keep", "ask"] {
            c.set_setting("complete_subtasks".into(), mode.into())
                .unwrap();
            set_done(&c, &a, false, true, false).unwrap();
            set_done_unasked(&c, &a, true).unwrap();
            assert!(done(&a) && !done(&a1) && !done(&a2), "{mode}: alone");
            set_done(&c, &a1, true, false, false).unwrap();
            set_done_unasked(&c, &a1, false).unwrap();
            assert!(done(&a) && !done(&a1), "{mode}: its parent stays done");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
