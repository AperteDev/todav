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
        Fixture::with(name, &["VTODO"])
    }

    /// A calendar for components `comps` (VTODO, VEVENT).
    fn with(name: &str, comps: &[&str]) -> Fixture {
        let (url, user, pass) = env();
        let dav = Dav::new(&url, &user, &pass);
        let href = format!("/remote.php/dav/calendars/{user}/it-{name}/");
        let comps: String = comps
            .iter()
            .map(|c| format!(r#"<c:comp name="{c}"/>"#))
            .collect();
        let body = format!(
            r#"<?xml version="1.0"?><c:mkcalendar xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:set><d:prop><d:displayname>it-{name}</d:displayname><c:supported-calendar-component-set>{comps}</c:supported-calendar-component-set></d:prop></d:set></c:mkcalendar>"#
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
fn a_refused_task_waits_for_an_edit_and_the_rest_syncs() {
    use todav_core::{Due, TaskPatch};
    let f = Fixture::new("refused");
    let add = |s: &str| {
        f.a.add_task(f.href.clone(), s.into(), None, None, false)
            .unwrap()
            .uid
    };
    let (bad, good) = (add("Bad dates"), add("Fine"));
    // A dated start with a timed due: the server refuses mixed kinds.
    let day = now() - now() % 86400;
    let dates = |start_all_day| TaskPatch {
        start: Some(Some(Due {
            at: day,
            all_day: start_all_day,
        })),
        due: Some(Some(Due {
            at: day + 9 * 3600,
            all_day: false,
        })),
        ..Default::default()
    };
    f.a.update_task(bad.clone(), dates(true)).unwrap();

    let report = f.a.sync().unwrap();
    let refused: Vec<_> = report.refused.iter().map(|r| r.summary.as_str()).collect();
    assert_eq!(refused, ["Bad dates"]);
    assert!(report.refused[0].error.contains("DTSTART"), "{report:?}");
    assert!(f.a.task(bad.clone()).unwrap().sync_error.is_some());
    f.b.sync().unwrap();
    assert!(f.b.task(good.clone()).is_some(), "the rest synced");
    assert!(f.b.task(bad.clone()).is_none());

    // Not sent again as it is; an edit the server takes goes up and clears the error.
    assert!(f.a.sync().unwrap().refused.is_empty());
    f.a.update_task(bad.clone(), dates(false)).unwrap();
    assert!(f.a.sync().unwrap().refused.is_empty());
    assert_eq!(f.a.task(bad.clone()).unwrap().sync_error, None);
    f.b.sync().unwrap();
    assert!(f.b.task(bad.clone()).is_some());

    // Gone again: dated today, they would show in the account's calendars.
    for uid in [bad, good] {
        f.a.delete_task(uid).unwrap();
    }
    f.a.sync().unwrap();
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

#[test]
fn lists_are_created_renamed_and_deleted() {
    let (url, user, pass) = env();
    let dir = std::env::temp_dir().join(format!("todav-it-lists-{}", now()));
    let c = Client::open(dir.to_string_lossy().to_string()).unwrap();
    c.set_account(url, user, pass).unwrap();
    let href = c
        .create_list("it-new & <list>".into(), "#B6469D".into(), false)
        .unwrap();
    let find = || c.lists().into_iter().find(|l| l.href == href);
    let l = find().expect("the new list is here at once");
    assert_eq!(
        (l.display_name.as_str(), l.color.as_deref()),
        ("it-new & <list>", Some("#B6469D"))
    );
    assert!(c.calendars().iter().all(|l| l.href != href), "tasks only");

    c.edit_list(href.clone(), "it-renamed".into(), "#0082C9".into())
        .unwrap();
    c.sync().unwrap();
    let l = find().unwrap();
    assert_eq!(
        (l.display_name.as_str(), l.color.as_deref()),
        ("it-renamed", Some("#0082C9")),
        "the server has it too"
    );

    c.add_task(href.clone(), "Inside".into(), None, None, false)
        .unwrap();
    c.delete_list(href.clone()).unwrap();
    assert!(find().is_none());
    assert!(!c.undo(None).unwrap(), "nothing left to undo in it");
    c.sync().unwrap();
    assert!(find().is_none(), "gone on the server");
}

#[test]
fn a_setting_reaches_a_new_device() {
    let (url, user, pass) = env();
    let device = |name: &str| {
        let dir = std::env::temp_dir().join(format!("todav-it-settings-{name}-{}", now()));
        let c = Client::open(dir.to_string_lossy().to_string()).unwrap();
        c.set_account(url.clone(), user.clone(), pass.clone())
            .unwrap();
        c
    };
    // A key of its own: a real setting would reach the account's real devices.
    let value = now().to_string();
    let a = device("a");
    a.sync().unwrap();
    a.set_setting("it_probe".into(), value.clone()).unwrap();
    a.sync().unwrap();
    let b = device("b");
    b.sync().unwrap();
    assert_eq!(b.setting("it_probe".into()), Some(value));
}

#[test]
fn password_login_checks_the_credentials() {
    let (url, user, pass) = env();
    let host = url.trim_start_matches("https://").to_string() + "/";
    let c = todav_core::password_login(host, user.clone(), pass.clone()).unwrap();
    assert_eq!((c.server, c.login_name), (url.clone(), user.clone()));
    let wrong = todav_core::password_login(url, user, format!("{pass}x"));
    assert!(matches!(wrong, Err(todav_core::Error::Auth)), "{wrong:?}");
}

#[test]
fn events_sync_with_their_zone_and_recurrence() {
    let f = Fixture::with("events", &["VEVENT", "VTODO"]);
    let cal =
        f.a.calendars()
            .into_iter()
            .find(|c| c.href == f.href)
            .expect("calendar discovered");
    assert!(cal.holds_events && cal.holds_tasks && cal.writable);
    assert!(
        f.a.lists().iter().any(|l| l.href == f.href),
        "also a task list"
    );

    // From "another client": weekly at 9:00 in New York, three times, the second moved to 15:00.
    let ics = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//it//EN\r\nBEGIN:VTIMEZONE\r\nTZID:America/New_York\r\nBEGIN:DAYLIGHT\r\nTZOFFSETFROM:-0500\r\nTZOFFSETTO:-0400\r\nDTSTART:20070311T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=2SU\r\nEND:DAYLIGHT\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:-0400\r\nTZOFFSETTO:-0500\r\nDTSTART:20071104T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=11;BYDAY=1SU\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:it-weekly\r\nDTSTAMP:20261001T000000Z\r\nSUMMARY:Weekly\r\nDTSTART;TZID=America/New_York:20261005T090000\r\nDTEND;TZID=America/New_York:20261005T100000\r\nRRULE:FREQ=WEEKLY;COUNT=3\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:it-weekly\r\nDTSTAMP:20261001T000000Z\r\nRECURRENCE-ID;TZID=America/New_York:20261012T090000\r\nSUMMARY:Moved\r\nDTSTART;TZID=America/New_York:20261012T150000\r\nDTEND;TZID=America/New_York:20261012T160000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let href = format!("{}it-weekly.ics", f.href);
    f.dav.put(&href, ics, None, None).unwrap();
    f.a.sync_list(f.href.clone()).unwrap();

    let db = rusqlite::Connection::open(&f.a_db).unwrap();
    let local = |utc: &str| {
        let t = todav_core::ical::parse_time(utc).unwrap();
        todav_core::store::to_local(&db, t).unwrap()
    };
    let got: Vec<(i64, i64, String)> =
        f.a.events(local("20261001T000000"), local("20261101T000000"))
            .into_iter()
            .filter(|e| e.list_href == f.href)
            .map(|e| (e.start, e.end, e.summary))
            .collect();
    assert_eq!(
        got,
        [
            (
                local("20261005T130000"),
                local("20261005T140000"),
                "Weekly".into()
            ),
            (
                local("20261012T190000"),
                local("20261012T200000"),
                "Moved".into()
            ),
            (
                local("20261019T130000"),
                local("20261019T140000"),
                "Weekly".into()
            ),
        ]
    );

    // Edits from here: one occurrence, then the rest of the series, reach device b.
    let rid = local("20261019T130000");
    let only = todav_core::EventPatch {
        summary: Some("Only this one".into()),
        ..Default::default()
    };
    f.a.update_event(
        f.href.clone(),
        "it-weekly".into(),
        Some(rid),
        todav_core::Scope::This,
        only,
    )
    .unwrap();
    f.a.sync().unwrap();
    let (_, server) = f.dav.get(&href).unwrap().unwrap();
    assert!(
        server.contains("SUMMARY:Only this one") && server.matches("RECURRENCE-ID").count() == 2,
        "{server}"
    );
    f.b.sync().unwrap();
    let on_b = f.b.events(rid, rid + 1);
    assert_eq!(
        on_b.iter().map(|e| e.summary.as_str()).collect::<Vec<_>>(),
        ["Only this one"]
    );

    // A new event borrows the zone of the stored one.
    f.a.set_local_zone("America/New_York".into());
    let time = todav_core::EventTime {
        start: local("20261020T160000"),
        end: local("20261020T170000"),
        all_day: false,
    };
    let uid =
        f.a.add_event(f.href.clone(), "Lunch".into(), time, vec![-600])
            .unwrap();
    f.a.sync().unwrap();
    let (_, lunch) = f.dav.get(&format!("{}{uid}.ics", f.href)).unwrap().unwrap();
    assert!(
        lunch.contains("DTSTART;TZID=America/New_York:20261020T120000")
            && lunch.contains("BEGIN:VTIMEZONE")
            && lunch.contains("TRIGGER:-PT10M"),
        "{lunch}"
    );
    f.a.delete_event(f.href.clone(), uid.clone(), None, todav_core::Scope::All)
        .unwrap();
    f.a.sync().unwrap();
    assert!(
        f.dav
            .get(&format!("{}{uid}.ics", f.href))
            .unwrap()
            .is_none()
    );

    // Deleted on the server: gone here too.
    f.dav.delete(&href, None, None).unwrap();
    f.a.sync_list(f.href.clone()).unwrap();
    assert!(
        f.a.events(local("20261001T000000"), local("20261101T000000"))
            .iter()
            .all(|e| e.list_href != f.href)
    );
}
