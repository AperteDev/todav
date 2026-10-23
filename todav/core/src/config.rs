//! `todav/config.json` in Nextcloud Files: category order and icons per list, plus grouping.

use crate::json::Json;
use crate::{Client, Error, Result, Task, store};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct CategoryMeta {
    pub name: String,
    pub icon: Option<String>,
}

/// Display order of top-level tasks or of subtasks among their siblings.
/// `Manual` is the stored order (X-APPLE-SORT-ORDER, shared with Nextcloud Tasks); `Newest` and
/// `Oldest` sort by creation time, `DueSoonest` and `DueLatest` by due date with undated tasks
/// last, in manual order; `Priority` highest first (1), tasks without one last.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Sort {
    #[default]
    Manual,
    Newest,
    Oldest,
    DueSoonest,
    DueLatest,
    Priority,
}

impl Sort {
    /// Stable, so equal creation times keep the manual order.
    fn apply(self, tasks: &mut [Task]) {
        match self {
            Sort::Manual => {}
            Sort::Newest => tasks.sort_by_key(|t| std::cmp::Reverse(t.created.unwrap_or(0))),
            Sort::Oldest => tasks.sort_by_key(|t| t.created.unwrap_or(0)),
            Sort::DueSoonest => tasks.sort_by_key(|t| (t.due.is_none(), t.due)),
            Sort::DueLatest => tasks.sort_by_key(|t| (t.due.is_none(), std::cmp::Reverse(t.due))),
            Sort::Priority => tasks.sort_by_key(|t| t.priority.filter(|p| *p > 0).unwrap_or(10)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CategoryGroup {
    /// None = tasks without a category ("Other").
    pub name: Option<String>,
    pub icon: Option<String>,
    /// Top-level tasks, each followed by its (flattened) subtasks.
    pub tasks: Vec<Task>,
}

/// Config key of a list: last path segment of its href (`.../calendars/user/shopping/` → `shopping`).
fn list_key(list_href: &str) -> &str {
    list_href
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
}

impl Client {
    /// Fetch config.json if it changed (ETag), or create it from the categories in use when missing.
    pub(crate) fn sync_config(&self) -> Result<()> {
        let dav = self.dav()?;
        let (href, etag) = {
            let db = self.db.lock().unwrap();
            (
                store::kv_get(&db, "config_href")?,
                store::kv_get(&db, "config_etag")?,
            )
        };
        let href = match href {
            Some(h) => h,
            None => {
                let principal = dav.principal()?;
                let h = format!(
                    "/remote.php/dav/files/{}/todav/config.json",
                    list_key(&principal)
                );
                store::kv_set(&self.db.lock().unwrap(), "config_href", &h)?;
                h
            }
        };
        let headers: Vec<(&str, &str)> = etag
            .as_deref()
            .map(|e| ("If-None-Match", e))
            .into_iter()
            .collect();
        let r = dav.request("GET", &href, &headers, "")?;
        let (body, etag) = match r.status {
            304 => return Ok(()),
            200 => (r.body, r.etag),
            404 => {
                let body = self.default_config().to_string();
                let dir = &href[..href.rfind('/').unwrap() + 1];
                dav.request("MKCOL", dir, &[], "")?; // 405 if it exists: fine
                let r = dav.request(
                    "PUT",
                    &href,
                    &[("Content-Type", "application/json"), ("If-None-Match", "*")],
                    &body,
                )?;
                match r.status {
                    200..=299 => {}
                    // Another device is creating it right now; fetched on the next sync.
                    404 | 412 => return Ok(()),
                    s => return Err(Error::Http(s, r.body)),
                }
                (body, r.etag)
            }
            s => return Err(Error::Http(s, r.body)),
        };
        {
            let db = self.db.lock().unwrap();
            store::kv_set(&db, "config_json", &body)?;
            store::kv_set(&db, "config_etag", etag.as_deref().unwrap_or_default())?;
        }
        for l in self.lists() {
            self.emit(&l.href, Vec::new());
        }
        Ok(())
    }

    /// Categories in use per list, in order of first appearance.
    fn default_config(&self) -> Json {
        let mut lists = BTreeMap::new();
        for l in self.lists() {
            let mut seen = HashSet::new();
            let cats: Vec<Json> = self
                .tasks(l.href.clone(), true)
                .into_iter()
                .filter_map(|t| t.category)
                .filter(|c| seen.insert(c.clone()))
                .enumerate()
                .map(|(i, c)| {
                    Json::Obj(BTreeMap::from([
                        ("name".into(), Json::Str(c)),
                        ("order".into(), Json::Num(i as f64)),
                    ]))
                })
                .collect();
            if cats.is_empty() {
                continue;
            }
            let entry = BTreeMap::from([("categories".to_string(), Json::Arr(cats))]);
            lists.insert(list_key(&l.href).to_string(), Json::Obj(entry));
        }
        Json::Obj(BTreeMap::from([
            ("version".into(), Json::Num(1.0)),
            ("lists".into(), Json::Obj(lists)),
        ]))
    }

    /// Configured categories of a list, in configured order.
    pub fn categories(&self, list_href: String) -> Vec<CategoryMeta> {
        let raw = store::kv_get(&self.db.lock().unwrap(), "config_json")
            .ok()
            .flatten();
        let Some(cfg) = raw.as_deref().and_then(Json::parse) else {
            return Vec::new();
        };
        let Some(list) = cfg.get("lists").and_then(|l| l.get(list_key(&list_href))) else {
            return Vec::new();
        };
        let mut cats: Vec<(f64, CategoryMeta)> = list
            .get("categories")
            .map(Json::arr)
            .unwrap_or_default()
            .iter()
            .filter_map(|c| {
                let meta = CategoryMeta {
                    name: c.get("name")?.str()?.to_string(),
                    icon: c.get("icon").and_then(Json::str).map(str::to_string),
                };
                Some((c.get("order").and_then(Json::num).unwrap_or(f64::MAX), meta))
            })
            .collect();
        cats.sort_by(|a, b| a.0.total_cmp(&b.0));
        cats.into_iter().map(|(_, m)| m).collect()
    }

    /// Open tasks grouped by category: configured categories first (in order), then unknown
    /// categories alphabetically, then uncategorised. Subtasks follow their top-level ancestor,
    /// done ones included while that ancestor is open.
    pub fn grouped(&self, list_href: String) -> Vec<CategoryGroup> {
        self.grouped_by(list_href, Sort::Manual, Sort::Manual, DoneView::default())
    }

    /// Open tasks as one list, no category grouping: each top-level task followed by its
    /// subtasks, in the given orders (done tasks as `done` says).
    pub fn ungrouped_by(
        &self,
        list_href: String,
        tasks: Sort,
        subtasks: Sort,
        done: DoneView,
    ) -> Vec<Task> {
        forest(
            split(self.tasks(list_href, true), done).0,
            tasks,
            subtasks,
            done,
        )
        .concat()
    }

    /// `grouped` with top-level tasks and subtasks (among siblings) in the given orders, and
    /// done tasks as `done` says.
    pub fn grouped_by(
        &self,
        list_href: String,
        tasks: Sort,
        subtasks: Sort,
        done: DoneView,
    ) -> Vec<CategoryGroup> {
        let meta = self.categories(list_href.clone());
        group(
            split(self.tasks(list_href, true), done).0,
            &meta,
            tasks,
            subtasks,
            done,
        )
    }

    /// Done tasks not shown by `grouped_by`: done top-level tasks and everything done below
    /// them (none when they are shown in place).
    pub fn finished(&self, list_href: String, done: DoneView) -> Vec<Task> {
        split(self.tasks(list_href, true), done).1
    }
}

/// How completed tasks are shown. The default moves completed top-level tasks (with everything
/// below them) to `finished` and keeps completed subtasks of open tasks in their tree.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DoneView {
    /// Completed top-level tasks stay in place among the open ones; `finished` is empty.
    pub in_place: bool,
    /// With `in_place`: completed top-level tasks go after the open ones.
    pub tasks_last: bool,
    /// Completed subtasks are left out everywhere.
    pub hide_subtasks: bool,
    /// Completed subtasks go after their open siblings.
    pub subtasks_last: bool,
}

/// (shown in the groups, finished) for `view`.
fn split(tasks: Vec<Task>, view: DoneView) -> (Vec<Task>, Vec<Task>) {
    let (mut tree, mut finished) = if view.in_place {
        (tasks, Vec::new())
    } else {
        split_done(tasks)
    };
    if view.hide_subtasks {
        for v in [&mut tree, &mut finished] {
            let uids: HashSet<String> = v.iter().map(|t| t.uid.clone()).collect();
            v.retain(|t| !(t.done && t.parent_uid.as_ref().is_some_and(|p| uids.contains(p))));
        }
    }
    (tree, finished)
}

/// (open tree, finished): a done task stays in the open tree while its parent does.
fn split_done(tasks: Vec<Task>) -> (Vec<Task>, Vec<Task>) {
    let by_uid: HashMap<&str, &Task> = tasks.iter().map(|t| (t.uid.as_str(), t)).collect();
    let in_tree = |t: &Task| {
        let mut t = t;
        for _ in 0..=by_uid.len() {
            // bounded: RELATED-TO from a server can form a cycle
            if !t.done {
                return true;
            }
            match t.parent_uid.as_deref().and_then(|p| by_uid.get(p)) {
                Some(p) => t = p,
                None => return false,
            }
        }
        false
    };
    let keep: HashSet<String> = tasks
        .iter()
        .filter(|t| in_tree(t))
        .map(|t| t.uid.clone())
        .collect();
    tasks.into_iter().partition(|t| keep.contains(&t.uid))
}

/// Each top-level task followed by its (flattened, depth-first) subtasks; completed ones last on
/// a level if `view` says so.
fn forest(tasks: Vec<Task>, sort: Sort, sub_sort: Sort, view: DoneView) -> Vec<Vec<Task>> {
    let uids: HashSet<String> = tasks.iter().map(|t| t.uid.clone()).collect();
    let mut children: HashMap<String, Vec<Task>> = HashMap::new();
    let mut top = Vec::new();
    for t in tasks {
        match t.parent_uid.clone().filter(|p| uids.contains(p)) {
            Some(p) => children.entry(p).or_default().push(t),
            None => top.push(t),
        }
    }
    sort.apply(&mut top);
    if view.tasks_last {
        top.sort_by_key(|t| t.done); // stable: keeps the sort within open and completed
    }
    for kids in children.values_mut() {
        sub_sort.apply(kids);
        if view.subtasks_last {
            kids.sort_by_key(|t| t.done);
        }
    }
    top.into_iter()
        .map(|t| {
            let mut flat = Vec::new();
            let mut stack = vec![t];
            while let Some(t) = stack.pop() {
                if let Some(mut kids) = children.remove(&t.uid) {
                    kids.reverse();
                    stack.extend(kids);
                }
                flat.push(t);
            }
            flat
        })
        .collect()
}

fn group(
    tasks: Vec<Task>,
    meta: &[CategoryMeta],
    sort: Sort,
    sub_sort: Sort,
    view: DoneView,
) -> Vec<CategoryGroup> {
    let mut groups: Vec<CategoryGroup> = meta
        .iter()
        .map(|m| CategoryGroup {
            name: Some(m.name.clone()),
            icon: m.icon.clone(),
            tasks: Vec::new(),
        })
        .collect();
    let mut unknown: BTreeMap<Option<String>, Vec<Task>> = BTreeMap::new();
    for flat in forest(tasks, sort, sub_sort, view) {
        let cat = flat[0].category.clone();
        match groups.iter_mut().find(|g| g.name == cat && cat.is_some()) {
            Some(g) => g.tasks.extend(flat),
            None => unknown.entry(cat).or_default().extend(flat),
        }
    }
    // BTreeMap orders None first; uncategorised goes last.
    let other = unknown.remove(&None);
    groups.extend(unknown.into_iter().map(|(name, tasks)| CategoryGroup {
        name,
        icon: None,
        tasks,
    }));
    groups.extend(other.map(|tasks| CategoryGroup {
        name: None,
        icon: None,
        tasks,
    }));
    groups.retain(|g| !g.tasks.is_empty());
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(uid: &str, cat: Option<&str>, parent: Option<&str>) -> Task {
        Task {
            uid: uid.into(),
            list_href: String::new(),
            summary: uid.into(),
            description: None,
            status: "NEEDS-ACTION".into(),
            done: false,
            completed_at: None,
            category: cat.map(Into::into),
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
        }
    }

    #[test]
    fn groups_in_config_order_with_subtasks_and_leftovers() {
        let meta = [
            CategoryMeta {
                name: "Home".into(),
                icon: Some("house".into()),
            },
            CategoryMeta {
                name: "Groceries".into(),
                icon: None,
            },
            CategoryMeta {
                name: "Empty".into(),
                icon: None,
            },
        ];
        let tasks = vec![
            task("milk", Some("Groceries"), None),
            task("lamp", Some("Home"), None),
            task("bulb", None, Some("lamp")),
            task("socket", Some("Car"), Some("bulb")), // deeper nesting, own category ignored
            task("loose", None, None),
            task("orphan", Some("Car"), Some("gone")), // parent not open → top level
            task("tyres", Some("Car"), None),
        ];
        let g = group(
            tasks,
            &meta,
            Sort::Manual,
            Sort::Manual,
            DoneView::default(),
        );
        let shape: Vec<(Option<&str>, Vec<&str>)> = g
            .iter()
            .map(|g| {
                (
                    g.name.as_deref(),
                    g.tasks.iter().map(|t| t.uid.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            [
                (Some("Home"), vec!["lamp", "bulb", "socket"]),
                (Some("Groceries"), vec!["milk"]),
                (Some("Car"), vec!["orphan", "tyres"]),
                (None, vec!["loose"]),
            ]
        );
        assert_eq!(g[0].icon.as_deref(), Some("house"));
    }

    #[test]
    fn due_sorts_put_undated_tasks_last_in_manual_order() {
        let due = |uid, due| Task {
            due,
            ..task(uid, None, None)
        };
        let mut tasks = vec![
            due("x", None),
            due("late", Some(9)),
            due("y", None),
            due("soon", Some(1)),
        ];
        let ids = |t: &[Task]| t.iter().map(|t| t.uid.clone()).collect::<Vec<_>>();
        Sort::DueSoonest.apply(&mut tasks);
        assert_eq!(ids(&tasks), ["soon", "late", "x", "y"]);
        Sort::DueLatest.apply(&mut tasks);
        assert_eq!(ids(&tasks), ["late", "soon", "x", "y"]);
    }

    #[test]
    fn priority_sort_puts_the_highest_first_and_none_last() {
        let p = |uid, priority| Task {
            priority,
            ..task(uid, None, None)
        };
        let mut tasks = vec![
            p("x", None),
            p("low", Some(9)),
            p("high", Some(1)),
            p("mid", Some(5)),
        ];
        Sort::Priority.apply(&mut tasks);
        let ids: Vec<_> = tasks.iter().map(|t| t.uid.as_str()).collect();
        assert_eq!(ids, ["high", "mid", "low", "x"]);
    }

    #[test]
    fn sorts_tasks_and_subtasks_separately() {
        let at = |uid, parent, created| Task {
            created: Some(created),
            ..task(uid, None, parent)
        };
        // Manual (input) order differs from creation order on both levels.
        let tasks = vec![
            at("old", None, 1),
            at("old_b", Some("old"), 5),
            at("old_a", Some("old"), 4),
            at("new", None, 9),
        ];
        let order = |s, sub| {
            group(tasks.clone(), &[], s, sub, DoneView::default())[0]
                .tasks
                .iter()
                .map(|t| t.uid.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            order(Sort::Manual, Sort::Manual),
            ["old", "old_b", "old_a", "new"]
        );
        assert_eq!(
            order(Sort::Newest, Sort::Oldest),
            ["new", "old", "old_a", "old_b"]
        );
        assert_eq!(
            order(Sort::Oldest, Sort::Newest),
            ["old", "old_b", "old_a", "new"]
        );
    }

    #[test]
    fn ungrouped_keeps_overall_order_across_categories() {
        let tasks = vec![
            task("b1", Some("B"), None),
            task("a1", Some("A"), None),
            task("a1_sub", None, Some("a1")),
            task("b2", Some("B"), None),
        ];
        let flat: Vec<String> = forest(tasks, Sort::Manual, Sort::Manual, DoneView::default())
            .concat()
            .into_iter()
            .map(|t| t.uid)
            .collect();
        assert_eq!(flat, ["b1", "a1", "a1_sub", "b2"]);
    }

    #[test]
    fn done_subtasks_stay_under_open_parents() {
        let done = |uid, parent| Task {
            done: true,
            ..task(uid, None, parent)
        };
        let tasks = vec![
            task("root", None, None),
            done("sub", Some("root")),
            done("subsub", Some("sub")),
            done("closed", None),
            done("closed_sub", Some("closed")),
            task("open_under_closed", None, Some("closed")),
            done("loop_a", Some("loop_b")),
            done("loop_b", Some("loop_a")),
        ];
        let (tree, finished) = split_done(tasks.clone());
        let uids = |v: &[Task]| v.iter().map(|t| t.uid.clone()).collect::<Vec<_>>();
        assert_eq!(uids(&tree), ["root", "sub", "subsub", "open_under_closed"]);
        assert_eq!(
            uids(&finished),
            ["closed", "closed_sub", "loop_a", "loop_b"]
        );

        // In place: nothing finished. Hidden subtasks: done tasks under a shown parent go.
        let (all, none) = split(
            tasks.clone(),
            DoneView {
                in_place: true,
                ..Default::default()
            },
        );
        assert_eq!((all.len(), none.len()), (tasks.len(), 0));
        let hide = |in_place| DoneView {
            in_place,
            hide_subtasks: true,
            ..Default::default()
        };
        let (tree, finished) = split(tasks.clone(), hide(false));
        assert_eq!(uids(&tree), ["root", "open_under_closed"]);
        assert_eq!(uids(&finished), ["closed"]); // the loop has no top: both hide each other
        let (tree, _) = split(tasks, hide(true));
        assert_eq!(uids(&tree), ["root", "closed", "open_under_closed"]);
    }

    #[test]
    fn done_tasks_and_subtasks_go_last_on_their_level() {
        let done = |uid, parent| Task {
            done: true,
            ..task(uid, None, parent)
        };
        let tasks = vec![
            done("a", None),
            done("a1", Some("a")),
            task("a2", None, Some("a")),
            task("b", None, None),
            done("b1", Some("b")),
            task("b2", None, Some("b")),
            task("c", None, None),
        ];
        let order = |tasks_last, subtasks_last| {
            let view = DoneView {
                in_place: true,
                tasks_last,
                subtasks_last,
                ..Default::default()
            };
            forest(tasks.clone(), Sort::Manual, Sort::Manual, view)
                .concat()
                .into_iter()
                .map(|t| t.uid)
                .collect::<Vec<_>>()
        };
        assert_eq!(order(false, false), ["a", "a1", "a2", "b", "b1", "b2", "c"]);
        assert_eq!(order(true, false), ["b", "b1", "b2", "c", "a", "a1", "a2"]);
        assert_eq!(order(false, true), ["a", "a2", "a1", "b", "b2", "b1", "c"]);
        assert_eq!(order(true, true), ["b", "b2", "b1", "c", "a", "a2", "a1"]);
    }
}
