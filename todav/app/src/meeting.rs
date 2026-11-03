//! Meeting links: the video call an event is held on, for its Join button.

use todav_core::Event;

/// The address to join event `e`'s call at: its conference (CONFERENCE) if it is a web address,
/// else the first link to a known video call service (Meet, Zoom, Teams, Jitsi, Webex, Nextcloud
/// Talk and the like) in its URL, location or notes.
pub fn link(e: &Event) -> Option<String> {
    if let Some(c) = e.conference.as_deref().and_then(|c| urls(c).next()) {
        return Some(c.to_string());
    }
    [&e.url, &e.location, &e.description]
        .into_iter()
        .flatten()
        .flat_map(|t| urls(t))
        .find(|u| is_call(u))
        .map(str::to_string)
}

/// The `https://` addresses in `text`, also inside HTML (`href="…"`) or `<…>`.
fn urls(text: &str) -> impl Iterator<Item = &str> {
    text.match_indices("https://").map(move |(at, _)| {
        let rest = &text[at..];
        let end = rest
            .find(|c: char| c.is_whitespace() || "\"'<>()[]".contains(c))
            .unwrap_or(rest.len());
        rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?'])
    })
}

/// Whether `url` is a video call's.
fn is_call(url: &str) -> bool {
    let rest = &url["https://".len()..];
    let (host, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let host = host
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let under = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    // Nextcloud Talk: a room's link, `…/call/<token>`, on any server.
    let talk = path
        .trim_start_matches("/index.php")
        .strip_prefix("/call/")
        .is_some_and(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric()));
    talk || [
        "meet.google.com",
        "zoom.us",
        "teams.microsoft.com",
        "teams.live.com",
        "meet.jit.si",
        "8x8.vc",
        "webex.com",
        "whereby.com",
        "meet.goto.com",
        "gotomeeting.com",
        "chime.aws",
        "join.skype.com",
        "bluejeans.com",
    ]
    .iter()
    .any(|d| under(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(location: Option<&str>, description: Option<&str>) -> Event {
        Event {
            uid: "u".into(),
            list_href: "l".into(),
            recurrence_id: None,
            summary: "s".into(),
            description: description.map(Into::into),
            location: location.map(Into::into),
            start: 0,
            end: 0,
            all_day: false,
            recurring: false,
            cancelled: false,
            reminders: vec![],
            rule: None,
            priority: None,
            color: None,
            declined: false,
            url: None,
            conference: None,
        }
    }

    #[test]
    fn calls_are_found() {
        let found = |l: Option<&str>, d: Option<&str>| link(&event(l, d));
        assert_eq!(
            found(Some("Room 4"), Some("See https://example.com.")),
            None
        );
        assert_eq!(
            found(
                Some("https://example.com/agenda"),
                Some("Join: <a href=\"https://us02web.zoom.us/j/123?pwd=x\">Zoom</a>.")
            )
            .as_deref(),
            Some("https://us02web.zoom.us/j/123?pwd=x")
        );
        assert_eq!(
            found(
                None,
                Some("Talk: https://cloud.example/index.php/call/ab12cd.")
            )
            .as_deref(),
            Some("https://cloud.example/index.php/call/ab12cd")
        );
        assert_eq!(found(None, Some("https://cloud.example/call/")), None);
        assert_eq!(
            found(Some("Meet (https://meet.google.com/abc-defg-hij)"), None).as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
        let mut e = event(None, Some("https://meet.jit.si/Room"));
        e.conference = Some("https://video.example/r/1".into());
        assert_eq!(link(&e).as_deref(), Some("https://video.example/r/1"));
        assert_eq!(link(&event(None, Some("https://notzoom.us/j/1"))), None);
    }
}
