//! CalDAV over blocking HTTP: discovery, sync-collection, PUT/DELETE, plus a tiny XML tree.

use crate::json::Json;
use crate::{Error, Result, ServerInfo};
use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

pub const DAV: &str = "DAV:";
pub const CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
pub const CS: &str = "http://calendarserver.org/ns/";
pub const APPLE: &str = "http://apple.com/ns/ical/";
pub const PUSH: &str = "https://bitfire.at/webdav-push";

/// Opening a connection (TLS included) gets this long: the system resends a dropped connection
/// request after 1 and 3 s, later ones are seldom answered. Then it is tried anew, up to
/// `CONNECT_TRIES` more times.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);
const CONNECT_TRIES: u32 = 3;

#[derive(Debug, Default, Clone)]
pub struct Element {
    pub ns: String,
    pub name: String,
    pub text: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    pub fn parse(xml: &str) -> Result<Element> {
        let mut r = quick_xml::NsReader::from_str(xml);
        let mut stack = vec![Element::default()];
        loop {
            let (ns, ev) = r
                .read_resolved_event()
                .map_err(|e| Error::Parse(e.to_string()))?;
            let ns = match ns {
                ResolveResult::Bound(n) => n.as_ref().to_string(),
                _ => String::new(),
            };
            match ev {
                Event::Start(ref e) | Event::Empty(ref e) => {
                    let attrs = e
                        .attributes()
                        .flatten()
                        .map(|a| {
                            let v = a
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map(|v| v.into_owned())
                                .unwrap_or_default();
                            (a.key.local_name().as_ref().to_string(), v)
                        })
                        .collect();
                    let el = Element {
                        ns,
                        name: e.local_name().as_ref().to_string(),
                        attrs,
                        ..Default::default()
                    };
                    stack.push(el);
                    if matches!(ev, Event::Empty(_)) {
                        let el = stack.pop().unwrap();
                        stack.last_mut().unwrap().children.push(el);
                    }
                }
                Event::End(_) => {
                    let el = stack.pop().unwrap();
                    stack
                        .last_mut()
                        .ok_or_else(|| Error::Parse("unbalanced xml".into()))?
                        .children
                        .push(el);
                }
                Event::Text(t) => stack.last_mut().unwrap().text += &*t,
                Event::CData(t) => stack.last_mut().unwrap().text += &*t,
                Event::GeneralRef(g) => {
                    let ent = format!("&{};", &*g);
                    let s = quick_xml::escape::unescape(&ent)
                        .map_err(|e| Error::Parse(e.to_string()))?;
                    stack.last_mut().unwrap().text += &s;
                }
                Event::Eof => break,
                _ => {}
            }
        }
        stack
            .pop()
            .and_then(|mut root| root.children.pop())
            .ok_or_else(|| Error::Parse("empty xml".into()))
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn child(&self, ns: &str, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.ns == ns && c.name == name)
    }

    pub fn children<'a>(&'a self, ns: &'a str, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children
            .iter()
            .filter(move |c| c.ns == ns && c.name == name)
    }

    /// Depth-first search for the first descendant with this name.
    pub fn find(&self, ns: &str, name: &str) -> Option<&Element> {
        self.children.iter().find_map(|c| {
            if c.ns == ns && c.name == name {
                Some(c)
            } else {
                c.find(ns, name)
            }
        })
    }

    pub fn find_text(&self, ns: &str, name: &str) -> Option<String> {
        self.find(ns, name)
            .map(|e| e.text.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

/// One `<d:response>` of a multistatus: href, HTTP status, and the merged 200 `<d:prop>`.
#[derive(Debug)]
pub struct DavResponse {
    pub href: String,
    pub status: u16,
    pub props: Element,
}

pub fn multistatus(xml: &str) -> Result<(Vec<DavResponse>, Option<String>)> {
    let root = Element::parse(xml)?;
    let status_code = |e: &Element| {
        e.child(DAV, "status")
            .and_then(|s| s.text.split_whitespace().nth(1)?.parse().ok())
    };
    let mut out = Vec::new();
    for r in root.children(DAV, "response") {
        let href = r
            .child(DAV, "href")
            .map(|h| h.text.trim().to_string())
            .unwrap_or_default();
        let mut props = Element::default();
        for ps in r.children(DAV, "propstat") {
            if status_code(ps) == Some(200)
                && let Some(p) = ps.children.iter().find(|c| c.name == "prop")
            {
                props.children.extend(p.children.iter().cloned());
            }
        }
        let status = status_code(r).unwrap_or(200);
        out.push(DavResponse {
            href: percent_decode(&href),
            status,
            props,
        });
    }
    let token = root
        .child(DAV, "sync-token")
        .map(|t| t.text.trim().to_string());
    Ok((out, token))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode_path(s: &str) -> String {
    let mut out = String::new();
    for &c in s.as_bytes() {
        if c.is_ascii_alphanumeric() || b"/-._~@!$&'()*+,;=:".contains(&c) {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

pub struct HttpResponse {
    pub status: u16,
    pub etag: Option<String>,
    pub location: Option<String>,
    pub body: String,
}

pub struct Dav {
    agent: ureq::Agent,
    origin: String,
    root: String,
    auth: String,
}

#[derive(Debug, Clone)]
pub struct RemoteList {
    pub href: String,
    pub display_name: String,
    pub color: Option<String>,
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
    pub push_topic: Option<String>,
    /// VAPID key advertised by the server; Some only when web-push is supported.
    pub push_vapid: Option<String>,
    /// Holds tasks (VTODO) and/or events (VEVENT).
    pub tasks: bool,
    pub events: bool,
    /// We may change it (shared read-only calendars and Birthdays may not).
    pub writable: bool,
    /// Its place among the user's calendars (calendar-order); None: the server keeps none.
    pub order: Option<i64>,
}

pub enum SyncResult {
    Changes {
        token: String,
        /// (href, etag, calendar data)
        changed: Vec<(String, String, String)>,
        removed: Vec<String>,
    },
    InvalidToken,
}

/// "https://host:8443" of "https://host:8443/nc/remote.php/dav/".
pub fn origin(url: &str) -> String {
    let url = url.trim_end_matches('/');
    url.splitn(4, '/').take(3).collect::<Vec<_>>().join("/")
}

impl Dav {
    /// `url` is the Nextcloud base (https://cloud.example) or a full DAV root.
    pub fn new(url: &str, user: &str, password: &str) -> Dav {
        let url = url.trim_end_matches('/');
        let root = if url.contains("/remote.php/") {
            format!("{url}/")
        } else {
            format!("{url}/remote.php/dav/")
        };
        let auth = format!(
            "Basic {}",
            crate::b64encode(format!("{user}:{password}").as_bytes(), false)
        );
        Dav::with(origin(url), root, auth)
    }

    /// A server signed in to with an OAuth access token (Google); `url` is the user's principal.
    pub fn bearer(url: &str, token: &str) -> Dav {
        Dav::with(origin(url), url.to_string(), format!("Bearer {token}"))
    }

    fn with(origin: String, root: String, auth: String) -> Dav {
        let agent = ureq::Agent::config_builder()
            .allow_non_standard_methods(true)
            .user_agent("Todav")
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .build()
            .new_agent();
        Dav {
            agent,
            origin,
            root,
            auth,
        }
    }

    pub fn url(&self, href: &str) -> String {
        let path = self.path(href);
        if path.starts_with("http") {
            path.to_string()
        } else {
            format!("{}{}", self.origin, percent_encode_path(path))
        }
    }

    /// `href` as a path on this server: a list's key (its full URL, see `store::lists`) loses
    /// the origin; a path stays as it is.
    pub fn path<'a>(&self, href: &'a str) -> &'a str {
        href.strip_prefix(self.origin.as_str())
            .filter(|p| p.starts_with('/'))
            .unwrap_or(href)
    }

    /// The full URL of a path on this server: how lists are keyed, so two servers' lists
    /// with the same path stay apart.
    pub fn absolute(&self, path: &str) -> String {
        format!("{}{}", self.origin, self.path(path))
    }

    pub fn request(
        &self,
        method: &str,
        href: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Result<HttpResponse> {
        let req = || {
            let mut b = ureq::http::Request::builder()
                .method(method)
                .uri(self.url(href))
                .header("Authorization", &self.auth);
            if !body.is_empty()
                && !headers
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            {
                b = b.header("Content-Type", "application/xml; charset=utf-8");
            }
            for (k, v) in headers {
                // Also one stored before ETags were made strong (below).
                let v = if k.eq_ignore_ascii_case("if-match") {
                    strong(v)
                } else {
                    v
                };
                b = b.header(*k, v);
            }
            b.body(body.to_string())
                .map_err(|e| Error::Net(e.to_string()))
        };
        // A connection the server never answers is tried again rather than waited out (until
        // the global timeout): nothing was sent on it yet.
        let mut tries = 0;
        let mut resp = loop {
            match self.agent.run(req()?) {
                Err(ureq::Error::Timeout(ureq::Timeout::Connect)) if tries < CONNECT_TRIES => {
                    tries += 1
                }
                r => break r.map_err(|e| Error::Net(e.to_string()))?,
            }
        };
        let header = |n: &str| {
            resp.headers()
                .get(n)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let etag = header("etag").map(|e| strong(&e).to_string());
        let location = header("location");
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| Error::Net(e.to_string()))?;
        if status == 401 {
            return Err(Error::Auth);
        }
        Ok(HttpResponse {
            status,
            etag,
            location,
            body,
        })
    }

    /// What `status.php` at the server's base (before `/remote.php/`) says.
    pub fn server_info(&self) -> Result<ServerInfo> {
        let base = self.root.split_once("/remote.php/").map_or("", |(b, _)| b);
        let r = self.request("GET", &format!("{base}/status.php"), &[], "")?;
        if r.status != 200 {
            return Err(Error::Http(r.status, r.body));
        }
        server_info(&r.body).ok_or_else(|| Error::Parse("not a Nextcloud status".into()))
    }

    fn propfind(&self, href: &str, depth: &str, props: &str) -> Result<Vec<DavResponse>> {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:" xmlns:c="{CALDAV}" xmlns:cs="{CS}" xmlns:a="{APPLE}"><d:prop>{props}</d:prop></d:propfind>"#
        );
        let r = self.request("PROPFIND", href, &[("Depth", depth)], &body)?;
        if r.status != 207 {
            return Err(Error::Http(r.status, r.body));
        }
        Ok(multistatus(&r.body)?.0)
    }

    /// The user's principal; without one named, the root is (Google's is given as the root).
    pub fn principal(&self) -> Result<String> {
        let p = self.propfind(&self.root, "0", "<d:current-user-principal/>")?;
        Ok(p.first()
            .and_then(|r| r.props.find_text(DAV, "href"))
            .map_or_else(|| self.root.clone(), |h| percent_decode(&h)))
    }

    /// The collection the user's calendars are in.
    pub fn home(&self) -> Result<String> {
        let principal = self.principal()?;
        let p = self.propfind(&principal, "0", "<c:calendar-home-set/>")?;
        p.first()
            .and_then(|r| r.props.find_text(DAV, "href"))
            .map(|h| percent_decode(&h)) // Google's: "/caldav/v2/me%40gmail.com/"
            .ok_or_else(|| Error::Parse("no calendar-home-set".into()))
    }

    pub fn discover(&self) -> Result<Vec<RemoteList>> {
        let home = self.home()?;
        let props = format!(
            r#"<d:resourcetype/><d:displayname/><d:sync-token/><cs:getctag/><a:calendar-color/><a:calendar-order/><c:supported-calendar-component-set/><d:current-user-privilege-set/><p:transports xmlns:p="{PUSH}"/><p:topic xmlns:p="{PUSH}"/>"#
        );
        let mut lists = Vec::new();
        for r in self.propfind(&home, "1", &props)? {
            let p = &r.props;
            let is_cal = p
                .child(DAV, "resourcetype")
                .is_some_and(|t| t.child(CALDAV, "calendar").is_some());
            // Without a component set a calendar takes every kind.
            let has = |kind: &str| {
                p.child(CALDAV, "supported-calendar-component-set")
                    .is_none_or(|s| s.children.iter().any(|c| c.attr("name") == Some(kind)))
            };
            let (tasks, events) = (has("VTODO"), has("VEVENT"));
            if !is_cal || !(tasks || events) {
                continue;
            }
            // Without a privilege set, assume we may write (and let the server refuse).
            let writable = p.child(DAV, "current-user-privilege-set").is_none_or(|s| {
                ["write", "write-content", "all"]
                    .iter()
                    .any(|w| s.find(DAV, w).is_some())
            });
            lists.push(RemoteList {
                display_name: p
                    .find_text(DAV, "displayname")
                    .unwrap_or_else(|| r.href.clone()),
                color: p.find_text(APPLE, "calendar-color"),
                ctag: p.find_text(CS, "getctag"),
                sync_token: p.find_text(DAV, "sync-token"),
                push_topic: p.find_text(PUSH, "topic"),
                push_vapid: p
                    .child(PUSH, "transports")
                    .and_then(|t| t.child(PUSH, "web-push"))
                    .map(|w| w.find_text(PUSH, "vapid-public-key").unwrap_or_default()),
                order: p
                    .find_text(APPLE, "calendar-order")
                    .and_then(|o| o.trim().parse().ok()),
                href: r.href,
                tasks,
                events,
                writable,
            });
        }
        Ok(lists)
    }

    /// RFC 6578 sync-collection; an empty token returns every member.
    pub fn sync_collection(&self, href: &str, token: &str) -> Result<SyncResult> {
        let token = xml_escape(token);
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><d:sync-collection xmlns:d="DAV:" xmlns:c="{CALDAV}"><d:sync-token>{token}</d:sync-token><d:sync-level>1</d:sync-level><d:prop><d:getetag/><c:calendar-data/></d:prop></d:sync-collection>"#
        );
        let r = self.request("REPORT", href, &[("Depth", "1")], &body)?;
        if matches!(r.status, 403 | 409) && r.body.contains("valid-sync-token") {
            return Ok(SyncResult::InvalidToken);
        }
        if r.status != 207 {
            return Err(Error::Http(r.status, r.body));
        }
        let (resps, token) = multistatus(&r.body)?;
        let mut changed = Vec::new();
        let mut removed = Vec::new();
        // Google's sync-collection leaves calendar-data out: fetched after.
        let mut bare = Vec::new();
        for resp in resps {
            if resp.href.trim_end_matches('/') == self.path(href).trim_end_matches('/') {
                continue;
            }
            if resp.status == 404 {
                removed.push(resp.href);
            } else if let Some(etag) = resp.props.find_text(DAV, "getetag") {
                match resp.props.find(CALDAV, "calendar-data") {
                    Some(data) => changed.push((resp.href, etag, data.text.clone())),
                    None => bare.push(resp.href),
                }
            }
        }
        for hrefs in bare.chunks(100) {
            changed.extend(self.multiget(href, hrefs)?);
        }
        Ok(SyncResult::Changes {
            token: token.unwrap_or_default(),
            changed,
            removed,
        })
    }

    /// (href, etag, calendar data) of resources `hrefs` in calendar `href`.
    fn multiget(&self, href: &str, hrefs: &[String]) -> Result<Vec<(String, String, String)>> {
        let hrefs: String = hrefs
            .iter()
            .map(|h| format!("<d:href>{}</d:href>", xml_escape(&percent_encode_path(h))))
            .collect();
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><c:calendar-multiget xmlns:d="DAV:" xmlns:c="{CALDAV}"><d:prop><d:getetag/><c:calendar-data/></d:prop>{hrefs}</c:calendar-multiget>"#
        );
        let r = self.request("REPORT", href, &[("Depth", "1")], &body)?;
        if r.status != 207 {
            return Err(Error::Http(r.status, r.body));
        }
        Ok(multistatus(&r.body)?
            .0
            .into_iter()
            .filter_map(|r| {
                let etag = r.props.find_text(DAV, "getetag")?;
                let data = r.props.find(CALDAV, "calendar-data")?.text.clone();
                Some((r.href, etag, data))
            })
            .collect())
    }

    /// PUT with `If-Match` (update) or `If-None-Match: *` (create). Returns the new ETag if the server sent one.
    /// `dont_notify`: our own push registration URL, so the server does not echo this change back to us.
    pub fn put(
        &self,
        href: &str,
        ics: &str,
        etag: Option<&str>,
        dont_notify: Option<&str>,
    ) -> Result<Option<String>> {
        let mut h = vec![("Content-Type", "text/calendar; charset=utf-8".to_string())];
        h.push(match etag {
            Some(e) => ("If-Match", e.to_string()),
            None => ("If-None-Match", "*".to_string()),
        });
        h.extend(dont_notify.map(|r| ("Push-Dont-Notify", format!("\"{r}\""))));
        let h: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let r = self.request("PUT", href, &h, ics)?;
        match r.status {
            200..=299 => Ok(r.etag),
            412 => Err(Error::Conflict),
            s => Err(Error::Http(s, r.body)),
        }
    }

    pub fn delete(&self, href: &str, etag: Option<&str>, dont_notify: Option<&str>) -> Result<()> {
        let mut h: Vec<(&str, String)> = etag
            .map(|e| ("If-Match", e.to_string()))
            .into_iter()
            .collect();
        h.extend(dont_notify.map(|r| ("Push-Dont-Notify", format!("\"{r}\""))));
        let h: Vec<(&str, &str)> = h.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let r = self.request("DELETE", href, &h, "")?;
        match r.status {
            200..=299 | 404 => Ok(()),
            412 => Err(Error::Conflict),
            s => Err(Error::Http(s, r.body)),
        }
    }

    /// Create calendar `href` for tasks, or for events (`events`); which it holds stays.
    pub fn mkcalendar(&self, href: &str, name: &str, color: &str, events: bool) -> Result<()> {
        let comp = if events { "VEVENT" } else { "VTODO" };
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><c:mkcalendar xmlns:d="DAV:" xmlns:c="{CALDAV}" xmlns:a="{APPLE}"><d:set><d:prop>{}<c:supported-calendar-component-set><c:comp name="{comp}"/></c:supported-calendar-component-set></d:prop></d:set></c:mkcalendar>"#,
            name_and_color(name, color)
        );
        let r = self.request("MKCALENDAR", href, &[], &body)?;
        match r.status {
            200..=299 => Ok(()),
            s => Err(Error::Http(s, r.body)),
        }
    }

    /// Rename and recolour calendar `href`.
    pub fn rename(&self, href: &str, name: &str, color: &str) -> Result<()> {
        self.proppatch(href, &name_and_color(name, color))
    }

    /// Give calendar `href` its place among the user's calendars.
    pub fn set_order(&self, href: &str, order: i64) -> Result<()> {
        self.proppatch(
            href,
            &format!("<a:calendar-order>{order}</a:calendar-order>"),
        )
    }

    /// Set properties (`props`, in the `a:` namespace for Apple's) of calendar `href`.
    fn proppatch(&self, href: &str, props: &str) -> Result<()> {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><d:propertyupdate xmlns:d="DAV:" xmlns:a="{APPLE}"><d:set><d:prop>{props}</d:prop></d:set></d:propertyupdate>"#
        );
        let r = self.request("PROPPATCH", href, &[], &body)?;
        if r.status != 207 {
            return Err(Error::Http(r.status, r.body));
        }
        // Each property has its own status: the first that is not 200 is the answer.
        let root = Element::parse(&r.body)?;
        let refused = root
            .children(DAV, "response")
            .flat_map(|r| r.children(DAV, "propstat"))
            .filter_map(|ps| ps.find_text(DAV, "status"))
            .find(|s| s.split_whitespace().nth(1) != Some("200"));
        match refused {
            Some(s) => {
                let code = s.split_whitespace().nth(1).and_then(|c| c.parse().ok());
                Err(Error::Http(code.unwrap_or(403), s))
            }
            None => Ok(()),
        }
    }

    /// GET a resource: (etag, body), or None if gone. Google sends no ETag header: then its
    /// getetag property.
    pub fn get(&self, href: &str) -> Result<Option<(Option<String>, String)>> {
        let r = self.request("GET", href, &[], "")?;
        match r.status {
            200 if r.etag.is_none() => {
                let p = self.propfind(href, "0", "<d:getetag/>")?;
                let etag = p.first().and_then(|p| p.props.find_text(DAV, "getetag"));
                Ok(Some((etag, r.body)))
            }
            200 => Ok(Some((r.etag, r.body))),
            404 => Ok(None),
            s => Err(Error::Http(s, r.body)),
        }
    }
}

/// A proxy that compresses a response marks its ETag weak ("W/"), and If-Match never matches a
/// weak one (412 every time); the value under it is the resource's own, strong ETag.
fn strong(etag: &str) -> &str {
    etag.strip_prefix("W/").unwrap_or(etag)
}

fn name_and_color(name: &str, color: &str) -> String {
    format!(
        "<d:displayname>{}</d:displayname><a:calendar-color>{}</a:calendar-color>",
        xml_escape(name),
        xml_escape(color)
    )
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `status.php`: `{"installed":true,"maintenance":false,"versionstring":"32.0.1","productname":"Nextcloud",…}`.
fn server_info(body: &str) -> Option<ServerInfo> {
    let j = Json::parse(body)?;
    let text = |k| j.get(k).and_then(Json::str).map(str::to_string);
    Some(ServerInfo {
        product: text("productname").unwrap_or_else(|| "Nextcloud".into()),
        version: text("versionstring")?,
        maintenance: j.get("maintenance") == Some(&Json::Bool(true)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_info_from_status_php() {
        let body = r#"{"installed":true,"maintenance":true,"needsDbUpgrade":false,"version":"32.0.1.2","versionstring":"32.0.1","edition":"","productname":"Nextcloud","extendedSupport":false}"#;
        assert_eq!(
            server_info(body),
            Some(ServerInfo {
                product: "Nextcloud".into(),
                version: "32.0.1".into(),
                maintenance: true,
            })
        );
        assert_eq!(server_info("<html>not found</html>"), None);
    }

    #[test]
    fn parses_multistatus() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:x1="https://bitfire.at/webdav-push">
 <d:response><d:href>/remote.php/dav/calendars/Test/zakupy%20dom/</d:href>
  <d:propstat><d:prop><d:displayname>Zakupy &amp; dom</d:displayname>
   <cal:supported-calendar-component-set><cal:comp name="VTODO"/></cal:supported-calendar-component-set>
   <x1:transports><x1:web-push><x1:vapid-public-key>BA1H</x1:vapid-public-key></x1:web-push></x1:transports>
   <x1:topic>calendar-12</x1:topic></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  <d:propstat><d:prop><d:getctag/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat>
 </d:response>
 <d:response><d:href>/x/gone.ics</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>
 <d:sync-token>http://sabre.io/ns/sync/42</d:sync-token>
</d:multistatus>"#;
        let (r, token) = multistatus(xml).unwrap();
        assert_eq!(token.as_deref(), Some("http://sabre.io/ns/sync/42"));
        assert_eq!(r[0].href, "/remote.php/dav/calendars/Test/zakupy dom/");
        assert_eq!(
            r[0].props.find_text(DAV, "displayname").unwrap(),
            "Zakupy & dom"
        );
        assert!(r[0].props.child(CS, "getctag").is_none());
        assert_eq!(
            r[0].props.find(CALDAV, "comp").unwrap().attr("name"),
            Some("VTODO")
        );
        assert_eq!(
            r[0].props.find_text(PUSH, "vapid-public-key").unwrap(),
            "BA1H"
        );
        assert_eq!((r[1].href.as_str(), r[1].status), ("/x/gone.ics", 404));
    }

    #[test]
    fn helpers() {
        assert_eq!(crate::b64encode(b"Test:6*U", false), "VGVzdDo2KlU=");
        assert_eq!(crate::b64encode(b"ab", false), "YWI=");
        assert_eq!(crate::b64decode("YWI=").unwrap(), b"ab");
        assert_eq!(crate::b64decode("_-8").unwrap(), [0xff, 0xef]);
        assert_eq!(percent_encode_path("/a b/ż.ics"), "/a%20b/%C5%BC.ics");
        assert_eq!(percent_decode("/a%20b/%C5%BC.ics"), "/a b/ż.ics");
    }
}
