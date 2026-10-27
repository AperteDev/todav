//! Links to a task or an event, `todav://task/<uid>` and `todav://event/<uid>`: copied in Todav,
//! pasted elsewhere (notes, chats), they open the item in Todav.

/// What a link is to.
#[derive(Debug, Clone, PartialEq)]
pub enum Link {
    Task(String),
    Event(String),
}

pub fn task(uid: &str) -> String {
    format!("todav://task/{}", encode(uid))
}

pub fn event(uid: &str) -> String {
    format!("todav://event/{}", encode(uid))
}

/// The item a `todav://` link is to; None for anything else.
pub fn parse(url: &str) -> Option<Link> {
    let rest = url.strip_prefix("todav://")?;
    let (kind, uid) = rest.split_once('/')?;
    let uid = decode(uid.trim_end_matches('/'))?;
    if uid.is_empty() {
        return None;
    }
    match kind {
        "task" => Some(Link::Task(uid)),
        "event" => Some(Link::Event(uid)),
        _ => None,
    }
}

/// UIDs hold anything ("…@host", spaces): all but unreserved characters escaped.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_round_trip() {
        let uid = "a1b2-c3 x@cloud.example/ż";
        assert_eq!(parse(&task(uid)), Some(Link::Task(uid.into())));
        assert_eq!(parse(&event(uid)), Some(Link::Event(uid.into())));
        assert_eq!(task("ab@c"), "todav://task/ab%40c");
        for bad in [
            "https://x/task/a",
            "todav://list/a",
            "todav://task/",
            "todav://task/%G1",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }
}
