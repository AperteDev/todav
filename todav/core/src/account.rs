//! Accounts: where lists are kept. A Nextcloud account syncs over CalDAV, so does one of another
//! CalDAV server, and a Google one (its calendars' events only); the local account (one at most) keeps its lists on this device only. Each list belongs to one
//! (`lists.account`); a switched-off account's lists are hidden and not synced, nothing is
//! deleted.

use crate::caldav::Dav;
use crate::{Client, Error, Result, store};
use rusqlite::{OptionalExtension, params};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccountKind {
    Nextcloud,
    /// Any other CalDAV server (mailbox.org, Radicale, Fastmail…), signed in to with a login name
    /// and a password. Nothing of Nextcloud's: no push (the service's timer checks for
    /// changes), the categories and synced settings stay on this device.
    CalDav,
    /// Lists on this device only (`store::LOCAL`), never synced.
    Local,
    /// Google Calendar over CalDAV with an OAuth access token as the password: events only;
    /// its calendars can be renamed and recoloured, not created or deleted, and their order
    /// stays on this device.
    Google,
    /// The system's calendars (Android's calendar store), one at most: read-only copies of
    /// those the user turned on, refreshed by the front-end (`set_system_calendars`).
    System,
}

impl AccountKind {
    fn as_str(self) -> &'static str {
        match self {
            AccountKind::Nextcloud => "nextcloud",
            AccountKind::CalDav => "caldav",
            AccountKind::Local => "local",
            AccountKind::Google => "google",
            AccountKind::System => "system",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub id: i64,
    pub kind: AccountKind,
    /// Given by the user; empty: the front-end names it ("user@host", "On This Device").
    pub name: String,
    /// The server, as signed in to (Nextcloud); another CalDAV server: the address its calendars
    /// are found from (`login::caldav_login`); Google: the user's CalDAV principal.
    pub url: String,
    pub user: String,
    /// The front-end's note on where the password comes from: on Linux the GNOME Online
    /// Accounts id; empty when signed in in Todav, the password in its keyring.
    pub source: String,
    /// Off: its lists are hidden and not synced.
    pub enabled: bool,
}

const COLS: &str = "id, kind, name, url, user, source, enabled";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        kind: match r.get::<_, String>(1)?.as_str() {
            "caldav" => AccountKind::CalDav,
            "local" => AccountKind::Local,
            "google" => AccountKind::Google,
            "system" => AccountKind::System,
            _ => AccountKind::Nextcloud,
        },
        name: r.get(2)?,
        url: r.get(3)?,
        user: r.get(4)?,
        source: r.get(5)?,
        enabled: r.get(6)?,
    })
}

/// A per-account `kv` key.
pub(crate) fn key(account: i64, name: &str) -> String {
    format!("a{account}_{name}")
}

impl Client {
    /// Every account, in the user's order (`reorder_accounts`; the order added until then).
    pub fn accounts(&self) -> Vec<Account> {
        let db = self.db.lock().unwrap();
        let st = db.prepare(&format!(
            "SELECT {COLS} FROM accounts ORDER BY position, id"
        ));
        st.and_then(|mut st| st.query_map([], row)?.collect())
            .unwrap_or_default()
    }

    pub fn account(&self, id: i64) -> Option<Account> {
        let db = self.db.lock().unwrap();
        db.query_row(
            &format!("SELECT {COLS} FROM accounts WHERE id = ?"),
            [id],
            row,
        )
        .optional()
        .ok()?
    }

    /// A new account; returns its id. Refuses (`Error::Exists`) a Nextcloud or CalDAV account
    /// already here (same server and user) and a second local or system one.
    pub fn add_account(
        &self,
        kind: AccountKind,
        url: String,
        user: String,
        source: String,
    ) -> Result<i64> {
        let url = url.trim_end_matches('/').to_string();
        let db = self.db.lock().unwrap();
        let same: bool = db.query_row(
            "SELECT EXISTS (SELECT 1 FROM accounts WHERE kind = ?1
               AND (kind IN ('local', 'system') OR (url = ?2 AND user = ?3)))",
            params![kind.as_str(), url, user],
            |r| r.get(0),
        )?;
        if same {
            return Err(Error::Exists);
        }
        db.execute(
            "INSERT INTO accounts (kind, url, user, source, position)
             VALUES (?1, ?2, ?3, ?4, (SELECT COALESCE(MAX(position), 0) + 1 FROM accounts))",
            params![kind.as_str(), url, user, source],
        )?;
        Ok(db.last_insert_rowid())
    }

    /// Put the accounts in this order (ids; ones left out keep their place after them).
    pub fn reorder_accounts(&self, ids: Vec<i64>) -> Result<()> {
        let db = self.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        tx.execute(
            "UPDATE accounts SET position = position + ?1",
            [ids.len() as i64],
        )?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE accounts SET position = ?2 WHERE id = ?1",
                params![id, i as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Name it; empty gives back the front-end's name.
    pub fn rename_account(&self, id: i64, name: String) -> Result<()> {
        let db = self.db.lock().unwrap();
        db.execute(
            "UPDATE accounts SET name = ?2 WHERE id = ?1",
            params![id, name.trim()],
        )?;
        Ok(())
    }

    /// Switch an account on or off: off hides its lists, stops syncing them and its server's
    /// pushes (the front-end's next `push_register` brings them back).
    pub fn set_account_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        if !enabled {
            let _ = self.unregister(id);
        }
        self.db.lock().unwrap().execute(
            "UPDATE accounts SET enabled = ?2 WHERE id = ?1",
            params![id, enabled],
        )?;
        for l in self.account_lists(id) {
            self.emit(&l, Vec::new());
        }
        Ok(())
    }

    /// The password is kept in memory only; front-ends keep it in the platform keyring (or
    /// get it from where `Account::source` says). Google's is an access token: connect again
    /// with a fresh one before it expires.
    pub fn connect(&self, id: i64, password: String) -> Result<()> {
        let a = self.account(id).ok_or(Error::NoAccount)?;
        let dav = match a.kind {
            AccountKind::Google => Dav::bearer(&a.url, &password),
            AccountKind::CalDav => Dav::caldav(&a.url, &a.user, &password),
            _ => Dav::new(&a.url, &a.user, &password),
        };
        self.dav.lock().unwrap().insert(id, Arc::new(dav));
        Ok(())
    }

    /// Connected for this run: `connect` gave the password. An account stays without one,
    /// e.g. when the keyring no longer has it; its lists are there, nothing syncs.
    pub fn connected(&self, id: i64) -> bool {
        self.dav.lock().unwrap().contains_key(&id)
    }

    /// The server moved: same user and password at `url`. Its lists are fetched again from
    /// there (edits waiting for the old server are lost: sync first).
    pub fn move_account(&self, id: i64, url: String, password: String) -> Result<()> {
        let _ = self.unregister(id);
        self.drop_lists(id)?;
        self.db.lock().unwrap().execute(
            "UPDATE accounts SET url = ?2 WHERE id = ?1",
            params![id, url.trim_end_matches('/')],
        )?;
        self.connect(id, password)
    }

    /// Remove an account and its lists from this device; a local account's are gone for
    /// good. Server pushes for it stop (edits waiting for the server are lost: sync first).
    pub fn remove_account(&self, id: i64) -> Result<()> {
        let _ = self.unregister(id);
        self.drop_lists(id)?;
        {
            let db = self.db.lock().unwrap();
            db.execute("DELETE FROM accounts WHERE id = ?", [id])?;
        }
        if self.settings_account() == Some(id) {
            self.set_settings_account(None)?;
        }
        self.dav.lock().unwrap().remove(&id);
        Ok(())
    }

    /// Forget the lists of an account and what it keeps in `kv`.
    fn drop_lists(&self, id: i64) -> Result<()> {
        let lists = self.account_lists(id);
        {
            let db = self.db.lock().unwrap();
            db.execute(
                "DELETE FROM moved WHERE list_href IN (SELECT href FROM lists WHERE account = ?)",
                [id],
            )?;
            db.execute("DELETE FROM lists WHERE account = ?", [id])?;
            db.execute(
                "DELETE FROM kv WHERE substr(key, 1, ?2) = ?1",
                params![key(id, ""), key(id, "").len() as i64],
            )?;
        }
        let mut journal = self.journal.lock().unwrap();
        for l in &lists {
            journal.forget_list(l);
        }
        drop(journal);
        for l in &lists {
            self.emit(l, Vec::new());
        }
        Ok(())
    }

    /// The hrefs of an account's lists, hidden ones too.
    pub(crate) fn account_lists(&self, id: i64) -> Vec<String> {
        let db = self.db.lock().unwrap();
        let st = db.prepare("SELECT href FROM lists WHERE account = ? ORDER BY href");
        st.and_then(|mut st| st.query_map([id], |r| r.get(0))?.collect())
            .unwrap_or_default()
    }

    /// The account whose server synced settings go through (the device-only
    /// `settings_account` setting); None: settings stay on this device.
    pub fn settings_account(&self) -> Option<i64> {
        self.setting("settings_account".into())?.parse().ok()
    }

    pub fn set_settings_account(&self, id: Option<i64>) -> Result<()> {
        let v = id.map(|i| i.to_string()).unwrap_or_default();
        self.set_setting("settings_account".into(), v)
    }

    /// The connection of account `id`.
    pub(crate) fn dav(&self, id: i64) -> Result<Arc<Dav>> {
        self.dav
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or(Error::NoAccount)
    }

    /// The connection of the account list `href` belongs to.
    pub(crate) fn list_dav(&self, href: &str) -> Result<Arc<Dav>> {
        let id = store::list_account(&self.db.lock().unwrap(), href)?;
        self.dav(id)
    }

    /// Ask the server what it is (no sign-in needed): reachable, its version, maintenance mode.
    pub fn server_info(&self, id: i64) -> Result<crate::ServerInfo> {
        self.dav(id)?.server_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caldav::RemoteList;

    fn remote(href: &str) -> RemoteList {
        RemoteList {
            href: href.into(),
            display_name: "Tasks".into(),
            color: None,
            ctag: None,
            sync_token: None,
            push_topic: None,
            push_vapid: None,
            tasks: true,
            events: false,
            writable: true,
            order: None,
            subscribed: false,
        }
    }

    #[test]
    fn lists_keep_the_users_order() {
        let dir = std::env::temp_dir().join(format!("todav-test-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let id = c
            .add_account(
                AccountKind::Nextcloud,
                "https://one.example".into(),
                "kris".into(),
                "".into(),
            )
            .unwrap();
        c.connect(id, "p".into()).unwrap();
        let dav = c.dav(id).unwrap();
        let list = |name: &str, order| RemoteList {
            display_name: name.into(),
            order: Some(order),
            ..remote(&format!("/cal/{name}/"))
        };
        let names = |c: &Client| {
            c.lists()
                .into_iter()
                .map(|l| l.display_name)
                .collect::<Vec<_>>()
        };

        // The server's order, then names.
        c.save_lists(id, &dav, &[list("b", 1), list("a", 1), list("c", 0)])
            .unwrap();
        assert_eq!(names(&c), ["c", "a", "b"]);
        assert!(!c.pending(Some(id)));

        // Ours waits for the server, and a sync before it is sent does not undo it.
        let href = |n: &str| dav.absolute(&format!("/cal/{n}/"));
        c.reorder_lists(vec![href("b"), href("c"), href("a")])
            .unwrap();
        assert_eq!(names(&c), ["b", "c", "a"]);
        assert!(c.pending(Some(id)));
        c.save_lists(id, &dav, &[list("b", 1), list("a", 1), list("c", 0)])
            .unwrap();
        assert_eq!(names(&c), ["b", "c", "a"]);

        // Google keeps no order: ours stays here, nothing for the server.
        let g = c
            .add_account(
                AccountKind::Google,
                "https://g.example/caldav/v2/kris/user".into(),
                "kris".into(),
                "".into(),
            )
            .unwrap();
        c.connect(g, "token".into()).unwrap();
        let gdav = c.dav(g).unwrap();
        let unordered = |name: &str| RemoteList {
            display_name: name.into(),
            ..remote(&format!("/g/{name}/"))
        };
        c.save_lists(g, &gdav, &[unordered("y"), unordered("x")])
            .unwrap();
        let gl = |n: &str| gdav.absolute(&format!("/g/{n}/"));
        c.reorder_lists(vec![gl("y"), gl("x")]).unwrap();
        assert!(!c.pending(Some(g)));
        c.save_lists(g, &gdav, &[unordered("x"), unordered("y")])
            .unwrap();
        let google: Vec<String> = c
            .lists()
            .into_iter()
            .filter(|l| l.account == g)
            .map(|l| l.display_name)
            .collect();
        assert_eq!(google, ["y", "x"]);

        // Lists on this device: kept here, nothing for a server.
        let here = c
            .add_account(AccountKind::Local, "".into(), "".into(), "".into())
            .unwrap();
        let x = c
            .create_list(here, "x".into(), "#000".into(), false)
            .unwrap();
        let y = c
            .create_list(here, "y".into(), "#000".into(), false)
            .unwrap();
        c.reorder_lists(vec![y.clone(), x.clone()]).unwrap();
        let local: Vec<String> = c
            .lists()
            .into_iter()
            .filter(|l| l.account == here)
            .map(|l| l.href)
            .collect();
        assert_eq!(local, [y, x]);
        assert!(!c.pending(Some(here)));
    }

    /// A CalDAV server whose calendar home holds what `home` says (the `<d:response>`s of its
    /// collections); returns its address. Any calendar is empty.
    fn serve(home: Arc<std::sync::Mutex<String>>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/dav/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut s = stream.unwrap();
                // The head, then the body it announces: closing before it is read resets the
                // connection under the client.
                let (mut buf, mut chunk) = (Vec::new(), [0; 4096]);
                let head_end = loop {
                    let n = s.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                    assert!(n > 0, "request cut short");
                };
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let length = head
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length: ")?
                            .parse()
                            .ok()
                    })
                    .unwrap_or(0);
                while buf.len() < head_end + length {
                    let n = s.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                }
                let line = head.lines().next().unwrap_or_default();
                let ms = |body: String| {
                    format!(
                        "<?xml version=\"1.0\"?><d:multistatus xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\">{body}</d:multistatus>"
                    )
                };
                let prop = |href: &str, props: &str| {
                    format!(
                        "<d:response><d:href>{href}</d:href><d:propstat><d:prop>{props}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
                    )
                };
                let body = if line.starts_with("PROPFIND /dav/ ") {
                    Some(ms(prop(
                        "/dav/",
                        "<d:current-user-principal><d:href>/dav/me/</d:href></d:current-user-principal>",
                    )))
                } else if line.starts_with("PROPFIND /dav/me/ ") {
                    Some(ms(prop(
                        "/dav/me/",
                        "<c:calendar-home-set><d:href>/dav/cals/</d:href></c:calendar-home-set>",
                    )))
                } else if line.starts_with("PROPFIND /dav/cals/ ") {
                    // The home itself and a scheduling inbox are no calendars.
                    let own = prop(
                        "/dav/cals/",
                        "<d:resourcetype><d:collection/></d:resourcetype>",
                    );
                    let inbox = prop(
                        "/dav/cals/inbox/",
                        "<d:resourcetype><d:collection/><c:schedule-inbox/></d:resourcetype>",
                    );
                    Some(ms(format!("{own}{inbox}{}", home.lock().unwrap())))
                } else if line.starts_with("REPORT /dav/cals/") {
                    Some(ms("<d:sync-token>t</d:sync-token>".into()))
                } else {
                    None
                };
                let reply = match body {
                    Some(b) => format!(
                        "HTTP/1.1 207 Multi-Status\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                        b.len()
                    ),
                    // Calendars are made in the server's own web interface.
                    None if line.starts_with("MKCALENDAR") => {
                        "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .into()
                    }
                    None => {
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .into()
                    }
                };
                let _ = s.write_all(reply.as_bytes());
            }
        });
        url
    }

    /// A calendar collection named `name` for the components `comps`.
    fn calendar(name: &str, comps: &[&str]) -> String {
        // None named: the server says nothing of components, and the calendar takes any.
        let set = if comps.is_empty() {
            String::new()
        } else {
            let comps: String = comps
                .iter()
                .map(|c| format!("<c:comp name=\"{c}\"/>"))
                .collect();
            format!(
                "<c:supported-calendar-component-set>{comps}</c:supported-calendar-component-set>"
            )
        };
        format!(
            "<d:response><d:href>/dav/cals/{name}/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>{name}</d:displayname>{set}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
        )
    }

    /// Whatever a CalDAV server holds, an account takes it: no lists, calendars only, task lists
    /// only, both, and the one for both kinds shows in each.
    #[test]
    fn a_caldav_account_may_have_any_mix_of_lists() {
        let dir = std::env::temp_dir().join(format!("todav-test-mix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let home = Arc::new(std::sync::Mutex::new(String::new()));
        let url = serve(home.clone());
        let id = c
            .add_account(AccountKind::CalDav, url, "me".into(), "".into())
            .unwrap();
        c.connect(id, "pw".into()).unwrap();
        let names = |l: Vec<crate::List>| l.into_iter().map(|l| l.display_name).collect::<Vec<_>>();
        let state = |c: &Client| (names(c.lists()), names(c.calendars()));
        let sync = |c: &Client| c.sync_account(id).unwrap();

        // Nothing at all: a sync that goes through, with nothing to show.
        sync(&c);
        assert_eq!(state(&c), (vec![], vec![]));
        assert!(c.last_sync(id).is_some());

        // Calendars only: no task list.
        *home.lock().unwrap() = calendar("Events", &["VEVENT"]);
        sync(&c);
        assert_eq!(state(&c), (vec![], vec!["Events".to_string()]));

        // Task lists only: no calendar.
        *home.lock().unwrap() = calendar("Todo", &["VTODO"]);
        sync(&c);
        assert_eq!(state(&c), (vec!["Todo".to_string()], vec![]));

        // A calendar for both kinds, and one that names no components (any).
        *home.lock().unwrap() = calendar("Both", &["VEVENT", "VTODO"]) + &calendar("Any", &[]);
        sync(&c);
        let (tasks, events) = state(&c);
        assert_eq!((tasks.len(), events.len()), (2, 2));

        // A server that keeps its calendars to itself says so, not "HTTP 405".
        let made = c.create_list(id, "New".into(), "#000000".into(), false);
        assert!(matches!(&made, Err(Error::Http(405, m)) if m.contains("does not allow")));

        // Everything deleted on the server: back to nothing.
        home.lock().unwrap().clear();
        sync(&c);
        assert_eq!(state(&c), (vec![], vec![]));
    }

    #[test]
    fn a_caldav_account_connects_at_its_own_address() {
        let dir = std::env::temp_dir().join(format!("todav-test-caldav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let add = |kind| {
            c.add_account(
                kind,
                "https://dav.example/caldav/".into(),
                "me".into(),
                "".into(),
            )
        };
        let id = add(AccountKind::CalDav).unwrap();
        assert_eq!(c.account(id).unwrap().kind, AccountKind::CalDav);
        assert_eq!(c.account(id).unwrap().url, "https://dav.example/caldav");
        // Another account for the same server and user, of another kind, is another account.
        assert!(matches!(add(AccountKind::CalDav), Err(Error::Exists)));
        assert!(add(AccountKind::Nextcloud).is_ok());
        c.connect(id, "pw".into()).unwrap();
        assert!(c.connected(id));
        // Its CalDAV root is the address itself, not Nextcloud's `/remote.php/dav/` under it.
        let dav = Dav::caldav("https://dav.example/caldav", "me", "pw");
        assert_eq!(dav.url("/caldav/work/"), "https://dav.example/caldav/work/");
        assert_eq!(
            dav.absolute("/caldav/work/"),
            "https://dav.example/caldav/work/"
        );
    }

    #[test]
    fn accounts_keep_their_own_lists() {
        let dir = std::env::temp_dir().join(format!("todav-test-accounts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = Client::open(dir.to_string_lossy().into()).unwrap();
        let add =
            |url: &str| c.add_account(AccountKind::Nextcloud, url.into(), "kris".into(), "".into());
        let (a, b) = (
            add("https://one.example").unwrap(),
            add("https://two.example/").unwrap(),
        );
        assert!(matches!(add("https://one.example/"), Err(Error::Exists)));
        let here = c
            .add_account(AccountKind::Local, "".into(), "".into(), "".into())
            .unwrap();
        let second = c.add_account(AccountKind::Local, "".into(), "".into(), "".into());
        assert!(matches!(second, Err(Error::Exists)), "one local account");

        // The same path on two servers: two lists.
        let path = "/remote.php/dav/calendars/kris/personal/";
        for id in [a, b] {
            c.connect(id, "p".into()).unwrap();
            c.save_lists(id, &c.dav(id).unwrap(), &[remote(path)])
                .unwrap();
        }
        let (one, two) = (
            format!("https://one.example{path}"),
            format!("https://two.example{path}"),
        );
        let mut lists: Vec<(String, i64)> =
            c.lists().into_iter().map(|l| (l.href, l.account)).collect();
        lists.sort();
        assert_eq!(lists, [(one.clone(), a), (two.clone(), b)]);

        // A task both servers have shows once, in the account that had it first.
        let uid = c
            .add_task(one.clone(), "Shared".into(), None, None, false)
            .unwrap()
            .uid;
        let db = c.db.lock().unwrap();
        assert!(store::task_elsewhere(&db, &uid, &two).unwrap());
        assert!(!store::task_elsewhere(&db, &uid, &one).unwrap());
        drop(db);
        assert!(c.pending(Some(a)) && !c.pending(Some(b)));

        // Switched off: hidden and not synced, nothing deleted.
        c.set_account_enabled(a, false).unwrap();
        assert_eq!(c.lists().len(), 1);
        assert!(c.task(uid.clone()).is_none() && !c.pending(None));
        c.set_account_enabled(a, true).unwrap();
        assert!(c.task(uid.clone()).is_some());

        // Removed: its lists go, the others stay.
        c.set_settings_account(Some(a)).unwrap();
        c.remove_account(a).unwrap();
        assert!(c.task(uid).is_none());
        assert_eq!(c.lists().iter().map(|l| l.account).collect::<Vec<_>>(), [b]);
        assert_eq!(c.settings_account(), None);
        assert_eq!(
            c.accounts().iter().map(|a| a.id).collect::<Vec<_>>(),
            [b, here]
        );

        // The user's order, new ones last.
        c.reorder_accounts(vec![here]).unwrap();
        let three = add("https://three.example").unwrap();
        let order: Vec<i64> = c.accounts().iter().map(|a| a.id).collect();
        assert_eq!(order, [here, b, three]);
    }
}
