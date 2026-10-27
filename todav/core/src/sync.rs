//! Sync engine: push the dirty queue, then pull each list with sync-collection.

use crate::caldav::{Dav, RemoteList, SyncResult};
use crate::ical::Calendar;
use crate::store::Kind;
use crate::{AccountKind, Client, Error, Result, store};
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
        let remote = dav.discover()?;
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
        if kind == AccountKind::Google {
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
        if !href.starts_with(store::LOCAL) {
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
        if self
            .account(account)
            .is_some_and(|a| a.kind == AccountKind::Google)
        {
            return Err(Error::Http(
                405,
                "Google calendars are deleted in Google Calendar".into(),
            ));
        }
        if !href.starts_with(store::LOCAL) {
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
            // An order not sent yet stays ours, as ours does where the server keeps none.
            tx.execute(
                "INSERT INTO lists (href, display_name, color, ctag, push_topic, has_tasks, has_events, writable, account, position)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, COALESCE(?10, 0))
                 ON CONFLICT(href) DO UPDATE SET display_name = ?2, color = ?3, ctag = ?4, push_topic = ?5,
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
        let dirty = store::dirty(&self.db.lock().unwrap(), Some(account))?;
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
            let etag = match dav.put(&href, &t.raw_ics, t.etag.as_deref(), reg.as_deref()) {
                Ok(etag) => etag,
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
            // a later pull can tell it from edits made elsewhere.
            let (etag, server) = match etag {
                Some(e) => (Some(e), None),
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
        tx.execute(
            "UPDATE lists SET sync_token = ? WHERE href = ?",
            params![new_token, list_href],
        )?;
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
