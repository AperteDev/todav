//! Runs against the real server from the workspace `.env`: `cargo test -p todav-core --features integration`.
//! Each test owns a calendar `it-<name>` (created once: Nextcloud rate-limits MKCALENDAR) and empties it first.
#![cfg(feature = "integration")]

use std::time::Duration;
use todav_core::caldav::{Dav, SyncResult};
use todav_core::{Client, now};

fn get(k: &str) -> Option<String> {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env"))
        .unwrap_or_default();
    std::env::var(k).ok().or_else(|| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")).map(str::to_string))
    })
}

fn env() -> (String, String, String) {
    (
        get("TODAV_URL").expect("TODAV_URL"),
        get("TODAV_USER").expect("TODAV_USER"),
        get("TODAV_PASS").expect("TODAV_PASS"),
    )
}

fn sql(db: &str, sql: &str) {
    rusqlite::Connection::open(db)
        .unwrap()
        .execute(sql, [])
        .unwrap();
}

/// A task written by "another client", straight to the server.
fn put_remote(dav: &Dav, list: &str, uid: &str, summary: &str) {
    let ics = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//it//EN\r\nBEGIN:VTODO\r\nUID:{uid}\r\nSUMMARY:{summary}\r\nEND:VTODO\r\nEND:VCALENDAR\r\n"
    );
    dav.put(&format!("{list}{uid}.ics"), &ics, None, None)
        .unwrap();
}

/// A task calendar of our own (emptied first) plus two independent "devices".
struct Fixture {
    dav: Dav,
    href: String,
    a: Client,
    b: Client,
    /// Device a's database, to poke at state the API does not expose.
    a_db: String,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let (url, user, pass) = env();
        let dav = Dav::new(&url, &user, &pass);
        let href = format!("/remote.php/dav/calendars/{user}/it-{name}/");
        let body = format!(
            r#"<?xml version="1.0"?><c:mkcalendar xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:set><d:prop><d:displayname>it-{name}</d:displayname><c:supported-calendar-component-set><c:comp name="VTODO"/></c:supported-calendar-component-set></d:prop></d:set></c:mkcalendar>"#
        );
        let existing = match dav.sync_collection(&href, "") {
            Ok(SyncResult::Changes { changed, .. }) => changed,
            _ => {
                let status = dav.request("MKCALENDAR", &href, &[], &body).unwrap().status;
                assert!(status == 201 || status == 405, "MKCALENDAR {status}");
                Vec::new()
            }
        };
        for (h, _, _) in existing {
            dav.delete(&h, None, None).unwrap();
        }
        let client = |dev: &str| {
            let dir = std::env::temp_dir().join(format!("todav-it-{name}-{dev}-{}", now()));
            let dir = dir.to_string_lossy().to_string();
            let c = Client::open(dir.clone()).unwrap();
            c.set_account(url.clone(), user.clone(), pass.clone())
                .unwrap();
            c.sync().unwrap();
            (c, format!("{dir}/core.sqlite"))
        };
        let ((a, a_db), (b, _)) = (client("a"), client("b"));
        Fixture {
            dav,
            href,
            a,
            b,
            a_db,
        }
    }
}

#[test]
fn add_on_a_shows_on_b_and_external_edits_arrive() {
    let f = Fixture::new("roundtrip");
    let t =
        f.a.add_task(
            f.href.clone(),
            "Żółty ser, mleko".into(),
            Some("Groceries".into()),
            None,
            false,
        )
        .unwrap();
    f.a.sync().unwrap();
    f.b.sync_list(f.href.clone()).unwrap();
    let on_b = f.b.task(t.uid.clone()).expect("task synced to b");
    assert_eq!(
        (on_b.summary.as_str(), on_b.category.as_deref()),
        ("Żółty ser, mleko", Some("Groceries"))
    );

    // Edit from "another client" (raw PUT), keeping unknown properties.
    let href = format!("{}{}.ics", f.href, t.uid);
    let (etag, ics) = f.dav.get(&href).unwrap().unwrap();
    let ics = ics.replace("SUMMARY:", "X-OTHER:keep\r\nSUMMARY:Edited ");
    f.dav.put(&href, &ics, etag.as_deref(), None).unwrap();
    f.a.sync_list(f.href.clone()).unwrap();
    assert!(
        f.a.task(t.uid.clone())
            .unwrap()
            .summary
            .starts_with("Edited")
    );

    // Tick on a: the foreign property survives the round trip.
    f.a.set_done(t.uid.clone(), true).unwrap();
    f.a.sync().unwrap();
    let (_, ics) = f.dav.get(&href).unwrap().unwrap();
    assert!(
        ics.contains("X-OTHER:keep")
            && ics.contains("STATUS:COMPLETED")
            && ics.contains("X-APPLE-SORT-ORDER:"),
        "{ics}"
    );

    // Delete on b reaches a.
    f.b.sync().unwrap();
    f.b.delete_task(t.uid.clone()).unwrap();
    f.b.sync().unwrap();
    f.a.sync().unwrap();
    assert!(f.a.task(t.uid).is_none());
}

#[test]
fn offline_tick_vs_edit_resolves_to_done() {
    let f = Fixture::new("conflict");
    let t =
        f.a.add_task(f.href.clone(), "Buy bread".into(), None, None, false)
            .unwrap();
    f.a.sync().unwrap();
    f.b.sync().unwrap();

    // a ticks, b renames later (newer LAST-MODIFIED) — both offline, then both sync.
    f.a.set_done(t.uid.clone(), true).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));
    f.b.update_task(
        t.uid.clone(),
        todav_core::TaskPatch {
            summary: Some("Buy rye bread".into()),
            ..Default::default()
        },
    )
    .unwrap();
    f.a.sync().unwrap();
    let r = f.b.sync().unwrap();
    assert_eq!(r.conflicts, 1);
    f.a.sync().unwrap();
    for c in [&f.a, &f.b] {
        let t = c.task(t.uid.clone()).unwrap();
        assert_eq!((t.summary.as_str(), t.done), ("Buy rye bread", true));
    }
}

#[test]
fn removed_remotely_disappears() {
    let f = Fixture::new("removed");
    let t =
        f.a.add_task(f.href.clone(), "Gone soon".into(), None, None, false)
            .unwrap();
    f.a.sync().unwrap();
    f.dav
        .delete(&format!("{}{}.ics", f.href, t.uid), None, None)
        .unwrap();
    f.a.sync().unwrap();
    assert!(f.a.task(t.uid).is_none());
}

#[test]
fn invalid_sync_token_falls_back_to_full_resync() {
    let f = Fixture::new("token");
    // Malformed (403 valid-sync-token), then well-formed but never issued (207, no changes).
    for bad in ["bogus", "http://sabre.io/ns/sync/999999999"] {
        let t =
            f.a.add_task(f.href.clone(), "Stale".into(), None, None, false)
                .unwrap();
        f.a.sync().unwrap();
        f.dav
            .delete(&format!("{}{}.ics", f.href, t.uid), None, None)
            .unwrap();
        let fresh = format!("fresh-{}", now());
        put_remote(&f.dav, &f.href, &fresh, "Fresh");

        sql(&f.a_db, &format!("UPDATE lists SET sync_token = '{bad}'"));
        f.a.sync().unwrap();
        assert!(f.a.task(t.uid).is_none(), "{bad}: deleted task survived");
        assert_eq!(f.a.task(fresh).unwrap().summary, "Fresh", "{bad}");
    }
}

/// Needs `NTFY_URL`: a web edit reaches a listening client through dav_push → ntfy, also
/// after the registration was refreshed for being close to expiry.
#[test]
fn push_delivers_changes_and_refreshes_registration() {
    let Some(ntfy) = get("NTFY_URL") else {
        eprintln!("NTFY_URL not set, skipping");
        return;
    };
    let f = Fixture::new("push");
    let (tx, rx) = std::sync::mpsc::channel();
    // Leaked: `listen` only returns at the next ntfy keepalive after `stop_listening`.
    let a: &'static Client = Box::leak(Box::new(f.a));
    std::thread::spawn(move || {
        a.listen(ntfy, &|r| {
            let _ = tx.send(r.map(|r| r.pulled).map_err(|e| e.to_string()));
        })
    });
    // First event: the registration done on start.
    rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
    let pulled_within = |secs| {
        let end = std::time::Instant::now() + Duration::from_secs(secs);
        while let Ok(r) = rx.recv_timeout(end.saturating_duration_since(std::time::Instant::now()))
        {
            if r.unwrap() > 0 {
                return true;
            }
        }
        false
    };

    let uid = format!("pushed-{}", now());
    put_remote(&f.dav, &f.href, &uid, "From the web");
    assert!(pulled_within(15), "no push within 15 s");
    assert_eq!(a.task(uid.clone()).unwrap().summary, "From the web");

    // An unexpired registration is kept; one close to expiry is renewed on the server.
    let resource = |regs: &[todav_core::push::PushRegistration]| {
        regs.iter()
            .find(|r| r.list_href == f.href)
            .cloned()
            .unwrap()
    };
    let topic = rusqlite::Connection::open(&f.a_db)
        .unwrap()
        .query_row("SELECT value FROM kv WHERE key = 'ntfy_topic'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap();
    let push_resource = format!(
        "{}/{topic}?up=1",
        get("NTFY_URL").unwrap().trim_end_matches('/')
    );
    let before = resource(&a.push_register(push_resource.clone(), 7 * 86400).unwrap());
    let same = resource(&a.push_register(push_resource.clone(), 7 * 86400).unwrap());
    assert_eq!(
        same.expires, before.expires,
        "fresh registration was redone"
    );
    sql(
        &f.a_db,
        "UPDATE lists SET push_expires = strftime('%s','now') + 60",
    );
    let renewed = resource(&a.push_register(push_resource, 7 * 86400).unwrap());
    assert!(
        renewed.expires > now() + 6 * 86400,
        "not renewed: {renewed:?}"
    );

    let uid = format!("pushed-again-{}", now());
    put_remote(&f.dav, &f.href, &uid, "After renewal");
    assert!(pulled_within(15), "no push after renewal");
    assert!(a.task(uid).is_some());

    a.stop_listening();
    a.push_unregister().unwrap();
}
