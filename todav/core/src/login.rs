//! Nextcloud Login Flow v2: the user signs in in a browser (works behind SSO), we get an app password.

use crate::json::Json;
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct LoginFlow {
    /// Open this in a browser.
    pub login_url: String,
    pub endpoint: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct Credentials {
    pub server: String,
    pub login_name: String,
    pub app_password: String,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(crate::net::tls())
        .http_status_as_error(false)
        .user_agent("Todav")
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .new_agent()
}

fn post(url: &str, form: &[(&str, &str)]) -> Result<(u16, Option<Json>)> {
    let mut r = agent()
        .post(url)
        .send_form(form.iter().copied())
        .map_err(|e| Error::Net(e.to_string()))?;
    let body = r
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Net(e.to_string()))?;
    Ok((r.status().as_u16(), Json::parse(&body)))
}

/// A typed server address as a base URL: no trailing slash, https unless a scheme is given.
fn base_url(server: &str) -> String {
    let server = server.trim().trim_end_matches('/');
    if server.starts_with("http") {
        server.to_string()
    } else {
        format!("https://{server}")
    }
}

/// A login name and password typed in (an app password, or the account password without 2FA),
/// checked against the server's CalDAV.
pub fn password_login(server: String, login_name: String, password: String) -> Result<Credentials> {
    let server = base_url(&server);
    crate::caldav::Dav::new(&server, &login_name, &password).principal()?;
    Ok(Credentials {
        server,
        login_name,
        app_password: password,
    })
}

/// Where another CalDAV server's calendars are found from, signed in to with a login name and
/// a password: the address typed in, else where the server's own pointer (RFC 6764,
/// `/.well-known/caldav`) leads. The returned `server` is the one that answered.
pub fn caldav_login(server: String, login_name: String, password: String) -> Result<Credentials> {
    let typed = base_url(&server);
    let principal =
        |root: &str| crate::caldav::Dav::caldav(root, &login_name, &password).principal();
    let root = match principal(&typed) {
        Ok(_) => typed,
        Err(Error::Auth) => return Err(Error::Auth),
        Err(first) => {
            let found = well_known(&typed).ok_or(first)?;
            principal(&found)?;
            found
        }
    };
    Ok(Credentials {
        server: root,
        login_name,
        app_password: password,
    })
}

/// Where `/.well-known/caldav` of the server at `url` sends to, if it does. Not followed by the
/// request itself: a redirect would turn the PROPFIND into a GET.
fn well_known(url: &str) -> Option<String> {
    let origin = crate::caldav::origin(url);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(crate::net::tls())
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .user_agent("Todav")
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .into();
    let r = agent
        .get(format!("{origin}/.well-known/caldav"))
        .call()
        .ok()?;
    let to = r.headers().get("location")?.to_str().ok()?;
    Some(redirect_target(url, to))
}

/// Where a `Location` sends from the server at `url`. Behind a proxy a server often names
/// itself with http: the password does not go there when it was typed for https.
fn redirect_target(url: &str, to: &str) -> String {
    match to.strip_prefix("http://") {
        _ if to.starts_with('/') => format!("{}{to}", crate::caldav::origin(url)),
        Some(rest) if url.starts_with("https://") => format!("https://{rest}"),
        _ => to.to_string(),
    }
}

pub fn login_flow_start(server: String) -> Result<LoginFlow> {
    let server = base_url(&server);
    let (status, json) = post(&format!("{server}/index.php/login/v2"), &[])?;
    let field = |j: &Json, k: &str| j.get(k).and_then(Json::str).map(str::to_string);
    let bad = || Error::Http(status, "not a Nextcloud login flow response".into());
    let json = json.filter(|_| status == 200).ok_or_else(bad)?;
    let poll = json.get("poll").ok_or_else(bad)?;
    Ok(LoginFlow {
        login_url: field(&json, "login").ok_or_else(bad)?,
        endpoint: field(poll, "endpoint").ok_or_else(bad)?,
        token: field(poll, "token").ok_or_else(bad)?,
    })
}

/// None while the user has not finished signing in (poll every few seconds; the flow expires after 20 min).
pub fn login_flow_poll(flow: &LoginFlow) -> Result<Option<Credentials>> {
    let (status, json) = post(&flow.endpoint, &[("token", &flow.token)])?;
    if status == 404 {
        return Ok(None);
    }
    let field = |k: &str| json.as_ref()?.get(k)?.str().map(str::to_string);
    match (
        status,
        field("server"),
        field("loginName"),
        field("appPassword"),
    ) {
        (200, Some(server), Some(login_name), Some(app_password)) => Ok(Some(Credentials {
            server,
            login_name,
            app_password,
        })),
        _ => Err(Error::Http(status, "login flow failed".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// A server like mailbox.org's: nothing at its root, `/.well-known/caldav` sending to
    /// `/dav/`, which answers for the user `me` with the password `pw`.
    fn serve() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut s = stream.unwrap();
                let mut buf = [0; 4096];
                let n = s.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]);
                let line = req.lines().next().unwrap_or_default();
                let body = r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:"><d:response><d:href>/dav/</d:href><d:propstat><d:prop><d:current-user-principal><d:href>/dav/principals/me/</d:href></d:current-user-principal></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>"#;
                let reply = if line.starts_with("GET /.well-known/caldav") {
                    "HTTP/1.1 301 Moved\r\nLocation: /dav/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                } else if line.starts_with("PROPFIND /dav/ ") && !req.contains("Basic bWU6cHc=") {
                    "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_string()
                } else if line.starts_with("PROPFIND /dav/ ") {
                    format!(
                        "HTTP/1.1 207 Multi-Status\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                } else {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_string()
                };
                let _ = s.write_all(reply.as_bytes());
            }
        });
        url
    }

    #[test]
    fn a_redirect_never_leaves_https() {
        let r = redirect_target;
        assert_eq!(r("https://h.example", "/dav/"), "https://h.example/dav/");
        assert_eq!(
            r("https://h.example", "http://h.example/dav/"),
            "https://h.example/dav/"
        );
        assert_eq!(
            r("https://h.example", "https://d.example/"),
            "https://d.example/"
        );
        // Typed with http on purpose: left as it is.
        assert_eq!(
            r("http://h.example", "http://h.example/dav/"),
            "http://h.example/dav/"
        );
    }

    #[test]
    fn caldav_login_finds_the_calendars_where_the_server_points() {
        let url = serve();
        let login = |server: &str, pw: &str| caldav_login(server.into(), "me".into(), pw.into());
        // The address of the server alone: its own pointer leads to the CalDAV root.
        assert_eq!(login(&url, "pw").unwrap().server, format!("{url}/dav/"));
        // The CalDAV root itself, with or without its slash, stays as typed.
        assert_eq!(
            login(&format!("{url}/dav"), "pw").unwrap().server,
            format!("{url}/dav")
        );
        assert!(matches!(login(&url, "wrong"), Err(Error::Auth)));
        assert!(login("http://127.0.0.1:1", "pw").is_err());
    }
}
