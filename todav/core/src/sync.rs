//! Sync engine: push the dirty queue, then pull each list with sync-collection.

use crate::caldav::{Dav, RemoteList, SyncResult};
use crate::ical::Calendar;
use crate::json::Json;
use crate::store::Kind;
use crate::{AccountKind, Client, Error, Result, gtasks, store};
use rusqlite::params;
use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct SyncReport {
    pub pushed: u32,
    pub pulled: u32,
    pub deleted: u32,
    pub conflicts: u32,
    /// What the server refused this time (not again until edited); the rest synced.
    pub refused: Vec<Refused>,
    /// Accounts that did not sync (`sync`), by id; the others did.
    pub failed: Vec<(i64, Error)>,
}

impl SyncReport {
    fn add(&mut self, other: SyncReport) {
        self.pushed += other.pushed;
        self.pulled += other.pulled;
        self.deleted += other.deleted;
        self.conflicts += other.conflicts;
        self.refused.extend(other.refused);
        self.failed.extend(other.failed);
    }

    /// The first account's error, if one failed.
    pub fn into_result(mut self) -> Result<SyncReport> {
        match self.failed.is_empty() {
            true => Ok(self),
            false => Err(self.failed.remove(0).1),
        }
    }
}

/// A task or event the server would not take.
#[derive(Debug, Clone)]
pub struct Refused {
    pub task: bool,
    pub uid: String,
    pub list_href: String,
    pub summary: String,
    /// A deletion: the item is back, as on the server.
    pub deleted: bool,
    pub error: String,
}

/// The server's refusal of this one item (what it says is wrong with it), rather than a reason
/// to stop syncing: network trouble, its own failure, signing in, a conflict.
fn refusal(e: &Error) -> Option<String> {
    match e {
        Error::Http(400 | 403 | 405 | 413 | 415 | 422, _) => Some(e.to_string()),
        _ => None,
    }
}

impl Client {
    /// Sync every switched-on account that is connected. One failing (`SyncReport::failed`)
    /// does not stop the others.
    pub fn sync(&self) -> SyncReport {
        let mut report = SyncReport::default();
        for a in self.accounts() {
            if !a.enabled || !self.connected(a.id) {
                continue;
            }
            match self.sync_account(a.id) {
                Ok(r) => report.add(r),
                Err(e) => report.failed.push((a.id, e)),
            }
        }
        report
    }

    /// Push the account's dirty tasks, refresh its calendars, then pull each of them; and the
    /// categories, and the settings when they sync through it.
    pub fn sync_account(&self, id: i64) -> Result<SyncReport> {
        let _guard = self.sync_lock.lock().unwrap();
        let dav = self.dav(id)?;
        let mut report = self.push_dirty(id, &dav)?;
        let remote = self.remote_lists(id, &dav)?;
        self.save_lists(id, &dav, &remote)?;
        for l in &remote {
            self.pull(&dav, &dav.absolute(&l.href), &mut report)?;
        }
        // The categories and settings are files in Nextcloud.
        if self
            .account(id)
            .is_some_and(|a| a.kind == AccountKind::Nextcloud)
        {
            self.sync_config(id, &dav)?;
            if self.settings_account() == Some(id) {
                self.sync_settings(id, &dav)?;
            }
        }
        self.mark_synced(id)?;
        drop(_guard);
        // ponytail: a failure waits for the listener's hourly registration, which reports it.
        let _ = self.register_new(id);
        Ok(report)
    }

    /// The account's calendars, and for a Google account its task lists too.
    fn remote_lists(&self, id: i64, dav: &Dav) -> Result<Vec<RemoteList>> {
        let mut remote = dav.discover()?;
        if self
            .account(id)
            .is_some_and(|a| a.kind == AccountKind::Google)
        {
            remote.extend(gtasks::lists(dav)?);
        }
        Ok(remote)
    }

    /// When a sync of account `id` last went through (Unix time).
    pub fn last_sync(&self, id: i64) -> Option<i64> {
        let db = self.db.lock().unwrap();
        let t = store::kv_get(&db, &crate::account::key(id, "last_sync"));
        t.ok()??.parse().ok()
    }

    fn mark_synced(&self, id: i64) -> Result<()> {
        let db = self.db.lock().unwrap();
        let k = crate::account::key(id, "last_sync");
        store::kv_set(&db, &k, &crate::now().to_string())
    }

    /// Something waits for the server: edits in synced lists, old copies of moved tasks, or the
    /// lists' new order (of account `id`, or of any). Edits in lists on this device never do.
    pub fn pending(&self, id: Option<i64>) -> bool {
        let db = self.db.lock().unwrap();
        let moved = db.query_row(
            "SELECT EXISTS (SELECT 1 FROM lists l LEFT JOIN accounts a ON a.id = l.account
               WHERE COALESCE(a.enabled, 1) AND (?1 IS NULL OR l.account = ?1)
                 AND (l.position_dirty OR l.href IN (SELECT list_href FROM moved)))",
            [id],
            |r| r.get(0),
        );
        moved.unwrap_or(false) || store::dirty(&db, id).is_ok_and(|d| !d.is_empty())
    }

    pub fn sync_list(&self, list_href: String) -> Result<SyncReport> {
        let _guard = self.sync_lock.lock().unwrap();
        let id = store::list_account(&self.db.lock().unwrap(), &list_href)?;
        if self.account(id).is_some_and(|a| !a.enabled) {
            return Ok(SyncReport::default()); // a late push for a switched-off account
        }
        let dav = self.dav(id)?;
        let mut report = self.push_dirty(id, &dav)?;
        self.pull(&dav, &list_href, &mut report)?;
        self.mark_synced(id)?;
        Ok(report)
    }

    /// Pull only the list a push message was about; unknown topic → full sync.
    pub fn sync_topic(&self, topic: String) -> Result<SyncReport> {
        let href: Option<String> = {
            let db = self.db.lock().unwrap();
            db.query_row(
                "SELECT href FROM lists WHERE push_topic = ?",
                [&topic],
                |r| r.get(0),
            )
            .ok()
        };
        match href {
            Some(h) => self.sync_list(h),
            None => self.sync().into_result(),
        }
    }

    /// A new task list, or calendar of events (`events`), in `account`: on its server, or on
    /// this device only for the local account; returns its href.
    pub fn create_list(
        &self,
        account: i64,
        name: String,
        color: String,
        events: bool,
    ) -> Result<String> {
        let kind = self.account(account).ok_or(Error::NoAccount)?.kind;
        if kind == AccountKind::Google && events {
            return Err(Error::Http(
                405,
                "Google calendars are made in Google Calendar".into(),
            ));
        }
        if kind == AccountKind::Local {
            let href = format!("{}{}/", store::LOCAL, crate::new_uid());
            self.db.lock().unwrap().execute(
                "INSERT INTO lists (href, display_name, color, has_tasks, has_events, account)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![href, name, color, !events, events, account],
            )?;
            self.emit(&href, Vec::new());
            return Ok(href);
        }
        let _guard = self.sync_lock.lock().unwrap();
        let dav = self.dav(account)?;
        // Google keeps no colour of a task list: it stays here.
        if kind == AccountKind::Google {
            let href = gtasks::create_list(&dav, &name)?;
            self.save_lists(account, &dav, &self.remote_lists(account, &dav)?)?;
            self.db.lock().unwrap().execute(
                "UPDATE lists SET color = ?2 WHERE href = ?1",
                params![href, color],
            )?;
            self.emit(&href, Vec::new());
            return Ok(href);
        }
        let path = format!(
            "{}/{}/",
            dav.home()?.trim_end_matches('/'),
            crate::new_uid()
        );
        dav.mkcalendar(&path, &name, &color, events)?;
        self.save_lists(account, &dav, &dav.discover()?)?;
        let href = dav.absolute(&path);
        self.emit(&href, Vec::new());
        Ok(href)
    }

    /// Rename and recolour list `href`, on the server and here.
    pub fn edit_list(&self, href: String, name: String, color: String) -> Result<()> {
        let _guard = self.sync_lock.lock().unwrap();
        let account = store::list_account(&self.db.lock().unwrap(), &href)?;
        if self
            .account(account)
            .is_some_and(|a| a.kind == AccountKind::System)
        {
            return Err(Error::ReadOnly); // named and coloured where they come from
        }
        if gtasks::is_list(&href) {
            gtasks::rename_list(&*self.list_dav(&href)?, &href, &name)?;
        } else if !href.starts_with(store::LOCAL) {
            self.list_dav(&href)?.rename(&href, &name, &color)?;
        }
        self.db.lock().unwrap().execute(
            "UPDATE lists SET display_name = ?2, color = ?3 WHERE href = ?1",
            params![href, name, color],
        )?;
        self.emit(&href, Vec::new());
        Ok(())
    }

    /// Put lists in this order (hrefs, those of one account as a sidebar shows them; the
    /// others keep their place): at once here, on a Nextcloud server (its calendar-order,
    /// shared with Nextcloud's apps) at the next sync; Google keeps none.
    pub fn reorder_lists(&self, hrefs: Vec<String>) -> Result<()> {
        {
            let db = self.db.lock().unwrap();
            let tx = db.unchecked_transaction()?;
            for (i, href) in hrefs.iter().enumerate() {
                tx.execute(
                    "UPDATE lists SET position = ?2, position_dirty = COALESCE(
                       (SELECT kind = 'nextcloud' FROM accounts WHERE id = lists.account), 0)
                     WHERE href = ?1 AND position != ?2",
                    params![href, i as i64],
                )?;
            }
            tx.commit()?;
        }
        if let Some(href) = hrefs.first() {
            self.emit(href, Vec::new());
        }
        Ok(())
    }

    /// Delete list `href` and everything in it, on the server (Nextcloud keeps it in its trash
    /// bin for a while) and here; changes to it can no longer be undone.
    pub fn delete_list(&self, href: String) -> Result<()> {
        let _guard = self.sync_lock.lock().unwrap();
        let account = store::list_account(&self.db.lock().unwrap(), &href)?;
        if gtasks::is_list(&href) {
            gtasks::delete_list(&*self.list_dav(&href)?, &href)?;
        } else if self
            .account(account)
            .is_some_and(|a| a.kind == AccountKind::Google)
        {
            return Err(Error::Http(
                405,
                "Google calendars are deleted in Google Calendar".into(),
            ));
        } else if !href.starts_with(store::LOCAL) {
            self.list_dav(&href)?.delete(&href, None, None)?;
        }
        {
            let db = self.db.lock().unwrap();
            db.execute("DELETE FROM moved WHERE list_href = ?", [&href])?;
            db.execute("DELETE FROM lists WHERE href = ?", [&href])?;
        }
        self.journal.lock().unwrap().forget_list(&href);
        self.emit(&href, Vec::new());
        Ok(())
    }

    /// The account's lists as the server has them now; ones it no longer has go.
    pub(crate) fn save_lists(&self, account: i64, dav: &Dav, remote: &[RemoteList]) -> Result<()> {
        let db = self.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        let keep: HashSet<String> = remote.iter().map(|l| dav.absolute(&l.href)).collect();
        for l in remote {
            // An order not sent yet stays ours, as ours does where the server keeps none; so
            // does a colour (Google Tasks keeps none).
            tx.execute(
                "INSERT INTO lists (href, display_name, color, ctag, push_topic, has_tasks, has_events, writable, account, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, COALESCE(?10, 0))
                 ON CONFLICT(href) DO UPDATE SET display_name = ?2, color = COALESCE(?3, color), ctag = ?4, push_topic = ?5,
                   has_tasks = ?6, has_events = ?7, writable = ?8, account = ?9,
                   position = CASE WHEN position_dirty OR ?10 IS NULL THEN position ELSE ?10 END",
                params![
                    dav.absolute(&l.href),
                    l.display_name,
                    l.color,
                    l.ctag,
                    l.push_topic,
                    l.tasks,
                    l.events,
                    l.writable,
                    account,
                    l.order
                ],
            )?;
        }
        let here: Vec<String> = tx
            .prepare("SELECT href FROM lists WHERE account = ?")?
            .query_map([account], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for h in here.iter().filter(|h| !keep.contains(*h)) {
            tx.execute("DELETE FROM lists WHERE href = ?", [h])?;
        }
        tx.commit()?;
        Ok(())
    }

    fn push_dirty(&self, account: i64, dav: &Dav) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        // Copies of moved tasks go first, so the new one can take the same UID elsewhere.
        let moved: Vec<(String, String)> = self
            .db
            .lock()
            .unwrap()
            .prepare(
                "SELECT href, list_href FROM moved
                 WHERE list_href IN (SELECT href FROM lists WHERE account = ?)",
            )?
            .query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (href, list) in moved {
            let reg = self.registration(&list);
            // ponytail: no If-Match, so an edit made elsewhere since is lost with the old copy;
            // fetch and merge first if that turns out to matter.
            match dav.delete(&href, None, reg.as_deref()) {
                Ok(()) => report.pushed += 1,
                Err(e) if refusal(&e).is_some() => {} // kept there; nothing more to try
                Err(e) => return Err(e),
            }
            let db = self.db.lock().unwrap();
            db.execute("DELETE FROM moved WHERE href = ?", [&href])?;
        }
        // The lists' order.
        let orders: Vec<(String, i64)> = self
            .db
            .lock()
            .unwrap()
            .prepare("SELECT href, position FROM lists WHERE account = ? AND position_dirty")?
            .query_map([account], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (href, position) in orders {
            match dav.set_order(&href, position) {
                Ok(()) => {}
                Err(e) if refusal(&e).is_some() => {} // the server's order comes back
                Err(e) => return Err(e),
            }
            // Unless moved again meanwhile.
            self.db.lock().unwrap().execute(
                "UPDATE lists SET position_dirty = 0 WHERE href = ?1 AND position = ?2",
                params![href, position],
            )?;
        }
        let mut dirty = store::dirty(&self.db.lock().unwrap(), Some(account))?;
        // Parents before their subtasks, and siblings in their order: Google places a task
        // under its parent after the one before it, which it must have by then.
        dirty.sort_by_cached_key(|t| {
            let db = self.db.lock().unwrap();
            let task = |uid: &str| store::task(&db, uid).ok().flatten();
            let order = task(&t.uid).and_then(|t| t.sort_order);
            let mut depth = 0;
            let mut up = task(&t.uid).and_then(|t| t.parent_uid);
            while let Some(p) = up.filter(|_| depth < 64) {
                depth += 1;
                up = task(&p).and_then(|t| t.parent_uid);
            }
            (depth, order)
        });
        for t in dirty {
            let reg = self.registration(&t.list_href);
            if t.dirty == store::DELETED {
                let href = t.href.as_deref().unwrap_or_default();
                match dav.delete(href, t.etag.as_deref(), reg.as_deref()) {
                    // 412: changed on the server since we saw it; the server copy wins and the next pull restores it.
                    Ok(()) | Err(Error::Conflict) => {
                        let db = self.db.lock().unwrap();
                        // Restored (undo) while the DELETE was on its way: upload it anew.
                        let now = store::raw(&db, t.kind, &t.list_href, &t.uid)?;
                        if now.is_some_and(|r| r.dirty == store::MODIFIED) {
                            store::set_sync_state(&db, &t, None, None, store::MODIFIED)?
                        } else {
                            store::delete(&db, &t)?
                        }
                    }
                    // Not deleted there: back here, as the server has it.
                    Err(e) => match refusal(&e) {
                        Some(error) => {
                            let db = self.db.lock().unwrap();
                            let summary = store::refuse(&db, &t, &error)?;
                            store::set_sync_state(&db, &t, t.href.as_deref(), None, store::CLEAN)?;
                            report.refused.push(Refused {
                                task: t.kind == Kind::Task,
                                uid: t.uid.clone(),
                                list_href: t.list_href.clone(),
                                summary,
                                deleted: true,
                                error,
                            });
                            continue;
                        }
                        None => return Err(e),
                    },
                }
                report.pushed += 1;
                continue;
            }
            let href = t
                .href
                .clone()
                .unwrap_or_else(|| format!("{}{}.ics", dav.path(&t.list_href), t.uid));
            let put = if gtasks::is_list(&t.list_href) {
                self.put_google(dav, &t)
            } else {
                let etag = dav.put(&href, &t.raw_ics, t.etag.as_deref(), reg.as_deref());
                etag.map(|e| (href.clone(), e, None))
            };
            let (href, etag, server) = match put {
                Ok(put) => put,
                Err(Error::Conflict) => {
                    report.conflicts += 1;
                    self.resolve_conflict(dav, &t, &href, reg.as_deref())?;
                    continue;
                }
                // Kept here as edited, waiting for a version the server takes.
                Err(e) => match refusal(&e) {
                    Some(error) => {
                        let summary = store::refuse(&self.db.lock().unwrap(), &t, &error)?;
                        report.refused.push(Refused {
                            task: t.kind == Kind::Task,
                            uid: t.uid.clone(),
                            list_href: t.list_href.clone(),
                            summary,
                            deleted: false,
                            error,
                        });
                        continue;
                    }
                    None => return Err(e),
                },
            };
            // No ETag: the server stored something else than it was sent. Its copy gives one, so
            // a later pull can tell it from edits made elsewhere. Google's comes with its copy.
            let (etag, server) = match etag {
                Some(e) => (Some(e), server),
                None => match dav.get(&href)? {
                    Some((e, ics)) => (e, Calendar::parse(&ics)),
                    None => (None, None),
                },
            };
            let db = self.db.lock().unwrap();
            // Only clear the flag if nobody edited the row while we were uploading.
            let now = store::raw(&db, t.kind, &t.list_href, &t.uid)?;
            let still_same = now.is_some_and(|r| r.raw_ics == t.raw_ics);
            match server.filter(|_| still_same) {
                Some(cal) => {
                    store::save(
                        &db,
                        &t.list_href,
                        Some(&href),
                        etag.as_deref(),
                        &cal,
                        store::CLEAN,
                    )?;
                }
                None => {
                    let dirty = if still_same {
                        store::CLEAN
                    } else {
                        store::MODIFIED
                    };
                    store::set_sync_state(&db, &t, Some(&href), etag.as_deref(), dirty)?;
                }
            }
            report.pushed += 1;
        }
        Ok(report)
    }

    fn registration(&self, list_href: &str) -> Option<String> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT push_registration_href FROM lists WHERE href = ?",
            [list_href],
            |r| r.get(0),
        )
        .ok()?
    }

    /// Write task `o` to Google: anew, or over Google's copy (merged by the conflict rule when
    /// it changed there too), then where it is here when it was moved. Returns its href, ETag
    /// and Google's copy.
    fn put_google(
        &self,
        dav: &Dav,
        o: &store::RawObject,
    ) -> Result<(String, Option<String>, Option<Calendar>)> {
        let mut cal = Calendar::parse(&o.raw_ics).ok_or_else(|| Error::Parse(o.uid.clone()))?;
        // Where it is here: Google's ids of its parent and of the sibling before it.
        let (parent, previous, place) = {
            let db = self.db.lock().unwrap();
            let gid = |uid: &str| {
                let href = store::raw(&db, Kind::Task, "", uid).ok()??.href?;
                Some(gtasks::id(&href).to_string())
            };
            let task = store::task(&db, &o.uid)?.ok_or_else(|| Error::NotFound(o.uid.clone()))?;
            let siblings: Vec<_> = store::tasks(&db, &o.list_href, true)?
                .into_iter()
                .filter(|s| s.parent_uid == task.parent_uid)
                .collect();
            let at = siblings.iter().position(|s| s.uid == o.uid).unwrap_or(0);
            let previous = siblings[..at].iter().rev().find_map(|s| gid(&s.uid));
            let place = gtasks::place(task.sort_order, task.parent_uid.as_deref());
            (task.parent_uid.as_deref().and_then(gid), previous, place)
        };
        let moved = cal.text(gtasks::PLACE).is_some_and(|p| p != place);
        let body = |cal: &Calendar| -> Result<Json> {
            let due = store::local_time(&self.db.lock().unwrap(), cal, cal.prop("DUE"))?;
            Ok(gtasks::body(cal, due.map(|(d, _)| d)))
        };
        let new = |b: Json| {
            gtasks::insert(
                dav,
                &o.list_href,
                &b,
                parent.as_deref(),
                previous.as_deref(),
            )
        };
        let (mut j, placed) = match o.href.as_deref() {
            None => (new(body(&cal)?)?, true),
            Some(href) => match gtasks::update(dav, href, &body(&cal)?, o.etag.as_deref()) {
                // Deleted there: anew.
                Err(Error::NotFound(_)) => (new(body(&cal)?)?, true),
                Err(Error::Conflict) => match gtasks::get(dav, href)? {
                    None => (new(body(&cal)?)?, true),
                    Some(s) => {
                        let server = self.google_cal(&o.list_href, &s, Some(&o.uid))?;
                        let merged = resolve(&cal, &server);
                        self.log_conflict(&o.uid, &cal, &server)?;
                        let etag = s.get("etag").and_then(Json::str).map(str::to_string);
                        cal = merged;
                        (
                            gtasks::update(dav, href, &body(&cal)?, etag.as_deref())?,
                            false,
                        )
                    }
                },
                r => (r?, false),
            },
        };
        if moved && !placed {
            let href = gtasks::task_href(&o.list_href, &j);
            j = gtasks::move_to(dav, &href, parent.as_deref(), previous.as_deref())?;
        }
        let server = self.google_cal(&o.list_href, &j, Some(&o.uid))?;
        let etag = j.get("etag").and_then(Json::str).map(str::to_string);
        Ok((gtasks::task_href(&o.list_href, &j), etag, Some(server)))
    }

    /// Google's task `j` in list `list` as a VTODO: with UID `uid`, else that of the task here
    /// at its href, else Google's id; its parent's likewise.
    fn google_cal(&self, list: &str, j: &Json, uid: Option<&str>) -> Result<Calendar> {
        let db = self.db.lock().unwrap();
        let here = |id: &str| store::raw_by_href(&db, list, &format!("{list}tasks/{id}"));
        let id = j.get("id").and_then(Json::str).unwrap_or_default();
        let mine = match uid {
            Some(u) => store::raw(&db, Kind::Task, "", u)?,
            None => here(id)?,
        };
        let parent = match j.get("parent").and_then(Json::str) {
            Some(p) => Some(here(p)?.map_or_else(|| p.to_string(), |r| r.uid)),
            None => None,
        };
        let created = mine
            .as_ref()
            .and_then(|r| Calendar::parse(&r.raw_ics)?.time("CREATED"));
        let uid = uid
            .map(str::to_string)
            .or(mine.map(|r| r.uid))
            .unwrap_or_else(|| id.to_string());
        Ok(gtasks::to_ics(j, &uid, parent.as_deref(), created))
    }

    /// 412 on PUT: fetch the server copy, merge by the conflict rule, upload the result once.
    fn resolve_conflict(
        &self,
        dav: &Dav,
        local: &store::RawObject,
        href: &str,
        reg: Option<&str>,
    ) -> Result<()> {
        let l = Calendar::parse(&local.raw_ics).ok_or_else(|| Error::Parse(href.into()))?;
        let (merged, etag) = match dav.get(href)? {
            None => (l.clone(), dav.put(href, &local.raw_ics, None, reg)?), // gone on the server: re-create
            Some((server_etag, server_ics)) => {
                let s = Calendar::parse(&server_ics).ok_or_else(|| Error::Parse(href.into()))?;
                let merged = resolve(&l, &s);
                self.log_conflict(&local.uid, &l, &s)?;
                if merged.to_ics() == s.to_ics() {
                    (merged, server_etag)
                } else {
                    let etag = dav.put(href, &merged.to_ics(), server_etag.as_deref(), reg)?;
                    (merged, etag)
                }
            }
        };
        let db = self.db.lock().unwrap();
        store::save(
            &db,
            &local.list_href,
            Some(href),
            etag.as_deref(),
            &merged,
            store::CLEAN,
        )?;
        Ok(())
    }

    fn log_conflict(&self, uid: &str, local: &Calendar, server: &Calendar) -> Result<()> {
        let db = self.db.lock().unwrap();
        let mut log = store::kv_get(&db, "conflict_log")?.unwrap_or_default();
        log += &format!(
            "{} {uid} local={:?}/{:?} server={:?}/{:?}\n",
            crate::ical::fmt_utc(crate::now()),
            local.text("LAST-MODIFIED"),
            local.text("STATUS"),
            server.text("LAST-MODIFIED"),
            server.text("STATUS"),
        );
        // ponytail: unbounded log in kv; trim when it gets big enough to matter.
        store::kv_set(&db, "conflict_log", &log)
    }

    fn pull(&self, dav: &Dav, list_href: &str, report: &mut SyncReport) -> Result<()> {
        let token: String = {
            let db = self.db.lock().unwrap();
            db.query_row(
                "SELECT COALESCE(sync_token, '') FROM lists WHERE href = ?",
                [list_href],
                |r| r.get(0),
            )
            .map_err(|_| Error::NotFound(list_href.into()))?
        };
        if gtasks::is_list(list_href) {
            return self.pull_google(dav, list_href, &token, report);
        }
        let (full, res) = match dav.sync_collection(list_href, &token)? {
            SyncResult::InvalidToken => (true, dav.sync_collection(list_href, "")?),
            // Nextcloud answers a token it never issued (calendar recreated under the same URL)
            // with its current token and no changes, as if nothing happened.
            SyncResult::Changes {
                token: t,
                changed,
                removed,
            } if !token.is_empty() && t != token && changed.is_empty() && removed.is_empty() => {
                (true, dav.sync_collection(list_href, "")?)
            }
            r => (token.is_empty(), r),
        };
        let SyncResult::Changes {
            token: new_token,
            changed,
            removed,
        } = res
        else {
            return Err(Error::Parse("sync-token rejected twice".into()));
        };
        self.apply(list_href, changed, removed, full, report)?;
        self.db.lock().unwrap().execute(
            "UPDATE lists SET sync_token = ? WHERE href = ?",
            params![new_token, list_href],
        )?;
        Ok(())
    }

    /// Pull a Google task list: what changed since `since` (its sync token), all of it the
    /// first time.
    fn pull_google(
        &self,
        dav: &Dav,
        list_href: &str,
        since: &str,
        report: &mut SyncReport,
    ) -> Result<()> {
        let (items, next) = gtasks::changes(dav, list_href, since)?;
        let (mut changed, mut removed) = (Vec::new(), Vec::new());
        for j in &items {
            let href = gtasks::task_href(list_href, j);
            if j.get("deleted") == Some(&Json::Bool(true)) {
                removed.push(href);
            } else {
                let etag = j.get("etag").and_then(Json::str).unwrap_or_default();
                let cal = self.google_cal(list_href, j, None)?;
                changed.push((href, etag.to_string(), cal.to_ics()));
            }
        }
        self.apply(list_href, changed, removed, since.is_empty(), report)?;
        self.db.lock().unwrap().execute(
            "UPDATE lists SET sync_token = ? WHERE href = ?",
            params![next, list_href],
        )?;
        Ok(())
    }

    /// Store what a list holds now: `changed` (href, etag, calendar data) and `removed` (hrefs);
    /// with `full`, everything else of it is gone. Edits waiting here are kept.
    pub(crate) fn apply(
        &self,
        list_href: &str,
        changed: Vec<(String, String, String)>,
        removed: Vec<String>,
        full: bool,
        report: &mut SyncReport,
    ) -> Result<()> {
        let mut uids = Vec::new();
        let db = self.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        let mut seen = HashSet::new();
        for (href, etag, data) in changed {
            let Some(server) = Calendar::parse(&data) else {
                continue;
            };
            let uid = server.text("UID").unwrap_or_default();
            seen.insert(uid.clone());
            let kind = Kind::of(&server);
            if kind == Kind::Task && store::task_elsewhere(&tx, &uid, list_href)? {
                continue;
            }
            let local = store::raw(&tx, kind, list_href, &uid)?;
            if unchanged(local.as_ref(), &etag) {
                continue;
            }
            match local {
                Some(l) if l.dirty == store::MODIFIED => {
                    // Edited locally during this sync: merge, keep dirty so the next push uploads it.
                    let local_cal =
                        Calendar::parse(&l.raw_ics).ok_or_else(|| Error::Parse(uid.clone()))?;
                    let merged = resolve(&local_cal, &server);
                    store::save(
                        &tx,
                        list_href,
                        Some(&href),
                        Some(&etag),
                        &merged,
                        store::MODIFIED,
                    )?;
                }
                Some(l) if l.dirty == store::DELETED => {
                    store::set_sync_state(&tx, &l, Some(&href), Some(&etag), store::DELETED)?;
                }
                _ => {
                    store::save(
                        &tx,
                        list_href,
                        Some(&href),
                        Some(&etag),
                        &server,
                        store::CLEAN,
                    )?;
                }
            }
            report.pulled += 1;
            uids.push(uid);
        }
        let mut gone: Vec<store::RawObject> = Vec::new();
        for href in removed {
            gone.extend(store::raw_by_href(&tx, list_href, &href)?);
        }
        if full {
            // Full resync: anything the server had that we did not see no longer exists.
            let all = store::synced(&tx, list_href)?;
            gone.extend(all.into_iter().filter(|o| !seen.contains(&o.uid)));
        }
        for t in gone {
            if t.dirty == store::MODIFIED {
                // Deleted remotely but edited here: re-push as a new resource.
                store::set_sync_state(&tx, &t, None, None, store::MODIFIED)?;
            } else {
                store::delete(&tx, &t)?;
                report.deleted += 1;
            }
            uids.push(t.uid);
        }
        tx.commit()?;
        drop(db);
        if !uids.is_empty() {
            self.emit(list_href, uids);
        }
        Ok(())
    }
}

/// The server copy (`etag`) is the one `local` last synced with, often the upload this sync just
/// made coming back: nothing changed there. An edit made here since (an undo while the upload
/// was on its way) is newer and goes up with the next push; merged with its own base, the old
/// copy could win parts of it back (a reopened task completed again).
fn unchanged(local: Option<&store::RawObject>, etag: &str) -> bool {
    local.is_some_and(|l| l.etag.as_deref() == Some(etag))
}

/// Conflict rule: newer LAST-MODIFIED wins the whole VTODO; a completed status always sticks.
pub fn resolve(local: &Calendar, server: &Calendar) -> Calendar {
    let lm = |c: &Calendar| c.time("LAST-MODIFIED").unwrap_or(0);
    let (winner, loser) = if lm(local) > lm(server) {
        (local, server)
    } else {
        (server, local)
    };
    let mut out = winner.clone();
    let done = |c: &Calendar| c.text("STATUS").as_deref() == Some("COMPLETED");
    if done(loser) && !done(winner) {
        for p in ["STATUS", "COMPLETED", "PERCENT-COMPLETE"] {
            let line = loser.prop(p).cloned();
            out.set(
                p,
                line.as_ref().map_or("", |l| &l.params),
                line.as_ref().map(|l| l.value.as_str()),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cal(lm: &str, status: &str, summary: &str) -> Calendar {
        Calendar::parse(&format!(
            "BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:u\nSUMMARY:{summary}\nSTATUS:{status}\nLAST-MODIFIED:{lm}\nEND:VTODO\nEND:VCALENDAR\n"
        ))
        .unwrap()
    }

    #[test]
    fn a_server_copy_with_the_rows_etag_is_unchanged_even_with_edits_waiting() {
        let row = |etag: Option<&str>, dirty| store::RawObject {
            kind: Kind::Task,
            uid: "u".into(),
            list_href: "/l/".into(),
            href: Some("/l/u.ics".into()),
            etag: etag.map(Into::into),
            raw_ics: String::new(),
            dirty,
        };
        // Reopened (undo) after this sync uploaded it completed: the upload comes back.
        assert!(unchanged(Some(&row(Some("e1"), store::MODIFIED)), "e1"));
        assert!(unchanged(Some(&row(Some("e1"), store::CLEAN)), "e1"));
        assert!(
            !unchanged(Some(&row(Some("e0"), store::MODIFIED)), "e1"),
            "edited there too"
        );
        assert!(!unchanged(Some(&row(None, store::MODIFIED)), "e1"));
        assert!(!unchanged(None, "e1"));
    }

    #[test]
    fn newer_wins_but_completed_sticks() {
        let old_done = cal("20261001T000000Z", "COMPLETED", "old");
        let new_open = cal("20261002T000000Z", "NEEDS-ACTION", "new");
        let r = resolve(&old_done, &new_open);
        assert_eq!(r.text("SUMMARY").unwrap(), "new");
        assert_eq!(r.text("STATUS").unwrap(), "COMPLETED");

        let r = resolve(&new_open, &cal("20261001T000000Z", "NEEDS-ACTION", "old"));
        assert_eq!(r.text("SUMMARY").unwrap(), "new");
        assert_eq!(r.text("STATUS").unwrap(), "NEEDS-ACTION");
    }
}
