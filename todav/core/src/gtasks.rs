//! Google Tasks over its REST API, in a Google account next to its calendars (CalDAV) and with
//! the same access token. Each task is kept as a VTODO, so the store and the front-ends take it
//! as any other; Google keeps only its title, notes, done, a due date (without a time), its
//! parent and its place among its siblings, and drops the rest.
//!
//! A list's href is `{API}lists/{id}/`, a task's `{list}tasks/{id}`. A task made here keeps
//! its UID; one made elsewhere takes Google's id as its UID.

use crate::caldav::{Dav, RemoteList};
use crate::ical::{self, Calendar};
use crate::json::Json;
use crate::{Error, Result};
use std::collections::BTreeMap;

pub const API: &str = "https://tasks.googleapis.com/tasks/v1/";

/// Whether list `href` is a Google Tasks list.
pub fn is_list(href: &str) -> bool {
    href.starts_with(API)
}

/// Google's id at the end of a list's or task's href.
pub fn id(href: &str) -> &str {
    href.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
}

/// The href of task `j` (as Google sends it) in list `list`.
pub fn task_href(list: &str, j: &Json) -> String {
    format!("{list}tasks/{}", text(j, "id").unwrap_or_default())
}

fn text<'a>(j: &'a Json, key: &str) -> Option<&'a str> {
    j.get(key).and_then(Json::str).filter(|s| !s.is_empty())
}

/// One call; the JSON answer (None when empty). Gone: `NotFound`; changed since `etag`:
/// `Conflict`.
fn call(
    dav: &Dav,
    method: &str,
    url: &str,
    body: Option<&Json>,
    etag: Option<&str>,
) -> Result<Option<Json>> {
    let mut h = vec![("Content-Type", "application/json")];
    h.extend(etag.map(|e| ("If-Match", e)));
    let body = body.map(Json::to_string).unwrap_or_default();
    let r = dav.request(method, url, &h, &body)?;
    match r.status {
        200..=299 => Ok(Json::parse(&r.body)),
        404 | 410 => Err(Error::NotFound(url.into())),
        412 => Err(Error::Conflict),
        s => Err(Error::Http(s, r.body)),
    }
}

/// Every item of a paged listing at `url` (with its query).
fn all(dav: &Dav, url: &str) -> Result<Vec<Json>> {
    let mut out = Vec::new();
    let mut page = String::new();
    loop {
        let j = call(
            dav,
            "GET",
            &format!("{url}&maxResults=100&pageToken={page}"),
            None,
            None,
        )?
        .unwrap_or(Json::Null);
        out.extend(j.get("items").map_or(&[][..], Json::arr).iter().cloned());
        match text(&j, "nextPageToken") {
            Some(p) => page = p.to_string(),
            None => return Ok(out),
        }
    }
}

/// The account's task lists; none when Google gave no access to them.
pub fn lists(dav: &Dav) -> Result<Vec<RemoteList>> {
    let items = match all(dav, &format!("{API}users/@me/lists?")) {
        Err(Error::Http(403, _)) => return Ok(Vec::new()),
        r => r?,
    };
    Ok(items
        .iter()
        .filter_map(|l| {
            Some(RemoteList {
                href: format!("{API}lists/{}/", text(l, "id")?),
                display_name: text(l, "title").unwrap_or_default().to_string(),
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
            })
        })
        .collect())
}

fn title(name: &str) -> Json {
    Json::Obj(BTreeMap::from([("title".into(), Json::Str(name.into()))]))
}

/// A new task list; returns its href.
pub fn create_list(dav: &Dav, name: &str) -> Result<String> {
    let j = call(
        dav,
        "POST",
        &format!("{API}users/@me/lists"),
        Some(&title(name)),
        None,
    )?;
    let id = j.as_ref().and_then(|j| text(j, "id"));
    Ok(format!(
        "{API}lists/{}/",
        id.ok_or_else(|| Error::Parse("no list id".into()))?
    ))
}

pub fn rename_list(dav: &Dav, href: &str, name: &str) -> Result<()> {
    let url = format!("{API}users/@me/lists/{}", id(href));
    call(dav, "PATCH", &url, Some(&title(name)), None).map(drop)
}

/// Delete a task list with its tasks (Google keeps its first list).
pub fn delete_list(dav: &Dav, href: &str) -> Result<()> {
    match call(
        dav,
        "DELETE",
        &format!("{API}users/@me/lists/{}", id(href)),
        None,
        None,
    ) {
        Err(Error::NotFound(_)) => Ok(()),
        r => r.map(drop),
    }
}

/// The tasks of list `list` changed since `since` (an RFC 3339 time), deleted ones marked
/// `deleted`; with `since` empty, all it has now. Also the `since` for next time.
pub fn changes(dav: &Dav, list: &str, since: &str) -> Result<(Vec<Json>, String)> {
    let mut url = format!("{list}tasks?showHidden=true");
    if !since.is_empty() {
        url += &format!("&showDeleted=true&updatedMin={since}");
    }
    let items = all(dav, &url)?;
    // A minute before the latest change seen, so one saved meanwhile with an earlier time still
    // comes; those seen again come with their ETag unchanged.
    let latest = items
        .iter()
        .filter_map(|t| text(t, "updated").and_then(parse))
        .max();
    let next = latest.map_or_else(|| since.to_string(), |t| format(t - 60));
    Ok((items, next))
}

pub fn get(dav: &Dav, href: &str) -> Result<Option<Json>> {
    match call(dav, "GET", href, None, None) {
        Err(Error::NotFound(_)) => Ok(None),
        r => r,
    }
}

fn answer(j: Option<Json>) -> Result<Json> {
    j.ok_or_else(|| Error::Parse("empty answer".into()))
}

/// Query of a new or moved task's place: under `parent`, right after `previous` (Google's ids;
/// none: first).
fn place_query(parent: Option<&str>, previous: Option<&str>) -> String {
    let p = parent.map(|p| format!("parent={p}&")).unwrap_or_default();
    let q = previous
        .map(|q| format!("previous={q}"))
        .unwrap_or_default();
    format!("{p}{q}")
}

/// A new task in list `list`, placed as `place_query` says.
pub fn insert(
    dav: &Dav,
    list: &str,
    body: &Json,
    parent: Option<&str>,
    previous: Option<&str>,
) -> Result<Json> {
    let url = format!("{list}tasks?{}", place_query(parent, previous));
    answer(call(dav, "POST", &url, Some(body), None)?)
}

/// Replace task `href` as Google had it at `etag`.
pub fn update(dav: &Dav, href: &str, body: &Json, etag: Option<&str>) -> Result<Json> {
    let mut body = body.clone();
    if let Json::Obj(m) = &mut body {
        m.insert("id".into(), Json::Str(id(href).into()));
    }
    answer(call(dav, "PUT", href, Some(&body), etag)?)
}

/// Put task `href` elsewhere in its list, as `place_query` says.
pub fn move_to(
    dav: &Dav,
    href: &str,
    parent: Option<&str>,
    previous: Option<&str>,
) -> Result<Json> {
    let url = format!("{href}/move?{}", place_query(parent, previous));
    answer(call(dav, "POST", &url, None, None)?)
}

/// What Google keeps of a task: `due` is its local due time, of which the date goes.
pub fn body(cal: &Calendar, due: Option<i64>) -> Json {
    let mut m = BTreeMap::new();
    let mut put = |k: &str, v: String| m.insert(k.to_string(), Json::Str(v));
    put("title", cal.text("SUMMARY").unwrap_or_default());
    if let Some(n) = cal.text("DESCRIPTION") {
        put("notes", n);
    }
    let done = cal.text("STATUS").as_deref() == Some("COMPLETED");
    put(
        "status",
        if done { "completed" } else { "needsAction" }.into(),
    );
    if let Some(c) = cal.time("COMPLETED").filter(|_| done) {
        put("completed", format(c));
    }
    if let Some(d) = due {
        put("due", format(d.div_euclid(86400) * 86400));
    }
    Json::Obj(m)
}

/// Where a task is among its siblings, as Google placed it last (`X-TODAV-GOOGLE-PLACE`): its
/// order and parent. When it differs from where it is now, it was moved here.
pub fn place(order: Option<i64>, parent: Option<&str>) -> String {
    format!(
        "{};{}",
        order.unwrap_or_default(),
        parent.unwrap_or_default()
    )
}

pub const PLACE: &str = "X-TODAV-GOOGLE-PLACE";

/// Task `j` as a VTODO with UID `uid`, under task `parent` (a UID), made at `created`.
pub fn to_ics(j: &Json, uid: &str, parent: Option<&str>, created: Option<i64>) -> Calendar {
    let updated = text(j, "updated")
        .and_then(parse)
        .unwrap_or_else(crate::now);
    let mut cal = Calendar::new_todo(uid, created.unwrap_or(updated));
    cal.set_time("DTSTAMP", Some(updated));
    cal.set_time("LAST-MODIFIED", Some(updated));
    cal.set_text("SUMMARY", Some(text(j, "title").unwrap_or_default()));
    cal.set_text("DESCRIPTION", text(j, "notes"));
    let done = text(j, "status") == Some("completed");
    crate::set_done(
        &mut cal,
        done,
        text(j, "completed").and_then(parse).unwrap_or(updated),
    );
    if let Some(d) = text(j, "due").and_then(|d| d.get(..10)) {
        cal.set("DUE", ";VALUE=DATE", Some(&d.replace('-', "")));
    }
    if let Some(p) = parent {
        cal.set_parent(p);
    }
    let order = text(j, "position").and_then(|p| p.parse().ok());
    if let Some(o) = order {
        crate::set_order(&mut cal, o);
    }
    cal.set_text(PLACE, Some(&place(order, parent)));
    cal
}

/// "2026-10-29T13:28:46.632Z" as a Unix time.
fn parse(s: &str) -> Option<i64> {
    let digits: String = s
        .get(..19)?
        .chars()
        .filter(|c| !matches!(c, '-' | ':'))
        .collect();
    ical::parse_time(&digits)
}

fn format(t: i64) -> String {
    let s = ical::fmt_utc(t);
    format!(
        "{}-{}-{}T{}:{}:{}.000Z",
        &s[..4],
        &s[4..6],
        &s[6..8],
        &s[9..11],
        &s[11..13],
        &s[13..15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_google_task_round_trips_through_a_vtodo() {
        let j = Json::parse(
            r#"{"id":"VEQz","etag":"\"yn6\"","title":"Buy milk","updated":"2026-10-29T13:28:38.624Z",
                "position":"00000000000000000003","notes":"2 l","status":"completed",
                "completed":"2026-10-29T13:28:46.608Z","due":"2026-11-02T00:00:00.000Z","parent":"TGZt"}"#,
        )
        .unwrap();
        let cal = to_ics(&j, "u1", Some("p1"), None);
        assert_eq!(cal.text("SUMMARY").unwrap(), "Buy milk");
        assert_eq!(cal.text("STATUS").unwrap(), "COMPLETED");
        assert_eq!(cal.prop("DUE").unwrap().value, "20261102");
        assert_eq!(cal.parent_uid().as_deref(), Some("p1"));
        assert_eq!(cal.text("X-APPLE-SORT-ORDER").unwrap(), "3");
        assert_eq!(cal.text(PLACE).unwrap(), place(Some(3), Some("p1")));
        assert_eq!(task_href("L/", &j), "L/tasks/VEQz");

        let due = ical::parse_time("20261102T150000").unwrap();
        let b = body(&cal, Some(due));
        let get = |k| b.get(k).and_then(Json::str).unwrap().to_string();
        assert_eq!(get("title"), "Buy milk");
        assert_eq!(get("notes"), "2 l");
        assert_eq!(get("status"), "completed");
        assert_eq!(get("completed"), "2026-10-29T13:28:46.000Z");
        assert_eq!(get("due"), "2026-11-02T00:00:00.000Z", "the date only");

        let open = Json::parse(r#"{"id":"x","title":"a","status":"needsAction"}"#).unwrap();
        let b = body(&to_ics(&open, "x", None, None), None);
        assert!(b.get("due").is_none() && b.get("notes").is_none() && b.get("completed").is_none());
        assert_eq!(
            id("https://tasks.googleapis.com/tasks/v1/lists/abc/"),
            "abc"
        );
    }
}
