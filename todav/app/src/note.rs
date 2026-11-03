//! The notes of tasks and events as light Markdown (GitHub's): formatting, links (bare URLs and
//! e-mail addresses too), task boxes to tick. The note stays plain text in `DESCRIPTION`, so
//! other clients show it as written; front-ends style its ranges, show its links and tick its
//! boxes in it. Taken from Notav's `markdown.rs`, without its tables, footnotes and links to
//! notes.

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::ops::Range;

/// What a range of a note is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Style {
    /// A heading's line, level 1 to 6.
    Heading(u8),
    Strong,
    Emphasis,
    Strikethrough,
    /// Inline code, its backticks included.
    Code,
    /// A code block's lines, its fences included.
    CodeBlock,
    /// A quote's lines.
    Quote,
    /// A link's text, an autolink, a bare URL or e-mail address, with the address it opens
    /// (`https:`, `mailto:`, `todav:`…); None for a link to no such address (a relative one).
    Link(Option<String>),
    /// A list item's bullet or number.
    ListMark,
    /// A task's `[ ]` (false) or `[x]` (true).
    Task(bool),
    /// Markdown's own syntax: `**`, `#`, `>`, a link's target, fences, inline HTML tags, rules.
    /// Dimmed where the note is edited, hidden where it is read (a `<br>` as a line break).
    Mark,
}

/// `style` from character `start` to `end` (character offsets, as GTK's text buffer counts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

fn options() -> Options {
    Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_GFM
}

/// An element being parsed: where it is and what its children cover, so that the rest of it
/// is its Markdown marks.
struct Open {
    range: Range<usize>,
    marks: bool,
    children: Vec<Range<usize>>,
}

/// The styled ranges of `text`, in the order found. Ranges of different styles overlap
/// (strong text in a heading, marks in a link).
pub fn spans(text: &str) -> Vec<Span> {
    let parser = Parser::new_ext(text, options()).into_offset_iter();
    let mut out: Vec<(Range<usize>, Style)> = Vec::new();
    // Link reference definitions ("[id]: url") make no events.
    for (_, def) in parser.reference_definitions().iter() {
        out.push((def.span.clone(), Style::Mark));
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut in_code_block = false;
    let mut links: Vec<Option<String>> = Vec::new();
    // Text the parser hands over in pieces (it splits at `_`, `*`…), joined to find URLs in.
    let mut run: Option<Range<usize>> = None;
    let urls = |run: Option<Range<usize>>, out: &mut Vec<(Range<usize>, Style)>| {
        if let Some(r) = run {
            for (url, to) in bare_links(&text[r.clone()]) {
                out.push((
                    r.start + url.start..r.start + url.end,
                    Style::Link(Some(to)),
                ));
            }
        }
    };
    for (event, range) in parser {
        match (&event, &mut run) {
            (Event::Text(_), Some(r)) if r.end == range.start => {
                r.end = range.end;
            }
            (Event::Text(_), _) if !in_code_block && links.is_empty() => {
                urls(run.replace(range.clone()), &mut out);
            }
            _ => urls(run.take(), &mut out),
        }
        // Where what came before in the same parent ends: an escape's backslash is in no event.
        let before = stack
            .last()
            .map_or(0, |p| p.children.last().map_or(p.range.start, |c| c.end));
        if let Some(parent) = stack.last_mut()
            && !matches!(event, Event::End(_))
        {
            parent.children.push(range.clone());
        }
        match event {
            Event::Start(tag) => {
                let marks = match &tag {
                    Tag::Heading { level, .. } => {
                        out.push((range.clone(), Style::Heading(*level as u8)));
                        true
                    }
                    Tag::Strong => style(&mut out, &range, Style::Strong),
                    Tag::Emphasis => style(&mut out, &range, Style::Emphasis),
                    Tag::Strikethrough => style(&mut out, &range, Style::Strikethrough),
                    Tag::Link {
                        link_type,
                        dest_url,
                        ..
                    } => {
                        links.push(address(*link_type, dest_url));
                        true
                    }
                    Tag::Image { dest_url, .. } => {
                        links.push(has_scheme(dest_url).then(|| dest_url.to_string()));
                        true
                    }
                    Tag::CodeBlock(kind) => {
                        in_code_block = true;
                        out.push((whole_lines(text, &range), Style::CodeBlock));
                        matches!(kind, CodeBlockKind::Fenced(_))
                    }
                    Tag::BlockQuote(_) => {
                        out.push((whole_lines(text, &range), Style::Quote));
                        quote_marks(text, &range, &mut out);
                        false
                    }
                    // An HTML block (some calendars write HTML) stays as written: hidden, its
                    // text would be gone where the note is read.
                    _ => false,
                };
                stack.push(Open {
                    range,
                    marks,
                    children: Vec::new(),
                });
            }
            Event::End(end) => {
                let Some(open) = stack.pop() else { continue };
                match end {
                    TagEnd::Link | TagEnd::Image => {
                        let to = links.pop().flatten();
                        for c in &open.children {
                            out.push((c.clone(), Style::Link(to.clone())));
                        }
                    }
                    TagEnd::CodeBlock => in_code_block = false,
                    // The bullet or number: from the item's start to its first content.
                    TagEnd::Item => {
                        let line_end = line_end(text, open.range.start);
                        let first = open.children.first().map_or(line_end, |c| c.start);
                        out.push((open.range.start..first.min(line_end), Style::ListMark));
                    }
                    _ => {}
                }
                if open.marks {
                    for gap in gaps(&open.range, open.children) {
                        out.push((gap, Style::Mark));
                    }
                }
            }
            Event::Code(_) => {
                out.push((range.clone(), Style::Code));
                let ticks = text[range.clone()]
                    .bytes()
                    .take_while(|&b| b == b'`')
                    .count();
                out.push((range.start..range.start + ticks, Style::Mark));
                out.push((range.end - ticks..range.end, Style::Mark));
            }
            // An escaped mark ("\*"): its backslash, just before the text, is Markdown's.
            Event::Text(_) if range.start > before && text.as_bytes()[range.start - 1] == b'\\' => {
                out.push((range.start - 1..range.start, Style::Mark))
            }
            Event::TaskListMarker(done) => out.push((range, Style::Task(done))),
            Event::InlineHtml(_) | Event::Rule => out.push((range, Style::Mark)),
            _ => {}
        }
    }
    urls(run, &mut out);
    to_chars(text, out)
}

/// A note as it is read: its text without Markdown's marks, its boxes as ☐ and ☑, its bullets
/// as •, long bare web addresses shortened (the link keeps the whole), blank lines at its ends
/// and runs of them inside dropped; and the styled ranges of that text (no `Mark` or
/// `ListMark`; a `Task` is its box).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    pub text: String,
    pub spans: Vec<Span>,
}

/// Characters a bare web address shows at most where a note is read, its scheme left out.
const SHORT_URL: usize = 40;

pub fn read(text: &str) -> Read {
    let spans = spans(text);
    let chars: Vec<char> = text.chars().collect();
    let covering = |i: usize| spans.iter().filter(move |s| s.start <= i && i < s.end);
    let mut out: Vec<char> = Vec::new();
    // Where each character of `text` starts in `out`.
    let mut at = vec![0; chars.len() + 1];
    let mut i = 0;
    while i < chars.len() {
        at[i] = out.len();
        let (shown, next): (Vec<char>, usize) =
            if let Some(s) = covering(i).find(|s| matches!(s.style, Style::Task(_))) {
                let mark = if s.style == Style::Task(true) {
                    '☑'
                } else {
                    '☐'
                };
                (vec![mark], s.end)
            } else if let Some((s, short)) = covering(i).find_map(|s| match &s.style {
                Style::Link(Some(url)) if s.start == i => {
                    let shown: String = chars[s.start..s.end].iter().collect();
                    short_url(&shown, url).map(|short| (s, short))
                }
                _ => None,
            }) {
                (short.chars().collect(), s.end)
            } else if let Some(mark) = covering(i).find(|s| s.style == Style::Mark) {
                let tag: String = chars[i..mark.end.min(i + 3)].iter().collect();
                // Line breaks stay (a heading's is its mark).
                let br = chars[i] == '\n' || mark.start == i && tag.eq_ignore_ascii_case("<br");
                (br.then_some('\n').into_iter().collect(), i + 1)
            } else if let Some(l) = covering(i).find(|s| s.style == Style::ListMark) {
                // A task's box stands for its bullet.
                let boxed = spans
                    .iter()
                    .any(|s| matches!(s.style, Style::Task(_)) && s.start == l.end);
                match chars[i] {
                    _ if boxed => (vec![], i + 1),
                    '-' | '*' | '+' => (vec!['•'], i + 1),
                    c => (vec![c], i + 1),
                }
            } else {
                (vec![chars[i]], i + 1)
            };
        out.extend(shown);
        for a in &mut at[i + 1..next] {
            *a = out.len();
        }
        i = next;
    }
    at[chars.len()] = out.len();
    let (out, kept) = tidy(&out);
    let spans = spans
        .iter()
        .filter(|s| !matches!(s.style, Style::Mark | Style::ListMark))
        .map(|s| Span {
            start: kept[at[s.start]],
            end: kept[at[s.end]],
            style: s.style.clone(),
        })
        .filter(|s| s.start < s.end)
        .collect();
    Read {
        text: out.into_iter().collect(),
        spans,
    }
}

/// A bare web address `shown` (its text is its address `url`) as it is read when long: without
/// its scheme, cut at `SHORT_URL` characters with "…".
fn short_url(shown: &str, url: &str) -> Option<String> {
    let bare = shown == url || format!("https://{shown}") == url;
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    if !bare || !url.starts_with("http") || rest.chars().count() <= SHORT_URL {
        return None;
    }
    let cut: String = rest.chars().take(SHORT_URL - 1).collect();
    Some(format!("{cut}…"))
}

/// `chars` without blank lines (only spaces) at the start and the end, with one empty line for
/// each run of them inside, and without spaces ending a line; and where each character (and the
/// end) of `chars` went in what is kept.
fn tidy(chars: &[char]) -> (Vec<char>, Vec<usize>) {
    let lines: Vec<&[char]> = chars.split(|c| *c == '\n').collect();
    let blank = |l: &[char]| l.iter().all(|c| c.is_whitespace());
    let first = lines.iter().position(|l| !blank(l));
    let last = lines.iter().rposition(|l| !blank(l));
    let mut out = Vec::new();
    let mut kept = Vec::with_capacity(chars.len() + 1);
    let mut was_blank = false;
    for (n, line) in lines.iter().enumerate() {
        let inside = first.is_some_and(|f| n >= f) && last.is_some_and(|l| n <= l);
        let shown = inside && !(blank(line) && was_blank);
        was_blank = blank(line);
        let end = line
            .iter()
            .rposition(|c| !c.is_whitespace())
            .map_or(0, |e| e + 1);
        for (k, c) in line.iter().enumerate() {
            kept.push(out.len());
            if shown && k < end {
                out.push(*c);
            }
        }
        // Its line break, unless it ends the last line shown.
        if n + 1 < lines.len() {
            kept.push(out.len());
            if shown && last.is_some_and(|l| n < l) {
                out.push('\n');
            }
        }
    }
    kept.push(out.len());
    (out, kept)
}

/// `text` with its task box number `n` (from 0, in the order they come) ticked or unticked; as
/// `read` shows them.
pub fn toggle_nth_task(text: &str, n: usize) -> Option<String> {
    let mut boxes: Vec<Span> = spans(text)
        .into_iter()
        .filter(|s| matches!(s.style, Style::Task(_)))
        .collect();
    boxes.sort_by_key(|s| s.start);
    toggle_task(text, boxes.get(n)?.start)
}

/// The address the link at character `at` opens, if there is one there.
pub fn link_at(text: &str, at: usize) -> Option<String> {
    spans(text).into_iter().find_map(|s| match s.style {
        Style::Link(to) if (s.start..s.end).contains(&at) => to,
        _ => None,
    })
}

/// `text` with the task box at character `at` (its `[ ]` or `[x]`) ticked or unticked; None
/// when there is none there.
fn toggle_task(text: &str, at: usize) -> Option<String> {
    let (start, done) = spans(text).into_iter().find_map(|s| match s.style {
        Style::Task(done) if (s.start..s.end).contains(&at) => Some((s.start, done)),
        _ => None,
    })?;
    let mut chars: Vec<char> = text.chars().collect();
    chars[start + 1] = if done { ' ' } else { 'x' };
    Some(chars.into_iter().collect())
}

/// The address a Markdown link opens: one with a scheme, an e-mail autolink as `mailto:`.
fn address(kind: LinkType, dest: &str) -> Option<String> {
    match kind {
        LinkType::Email => Some(format!("mailto:{dest}")),
        _ if has_scheme(dest) => Some(dest.to_string()),
        _ => None,
    }
}

/// Whether `url` starts with a scheme ("https:", "mailto:").
fn has_scheme(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    })
}

/// Push `style` over `range`; its marks are what its children leave.
fn style(out: &mut Vec<(Range<usize>, Style)>, range: &Range<usize>, style: Style) -> bool {
    out.push((range.clone(), style));
    true
}

/// What of `range` its children do not cover.
fn gaps(range: &Range<usize>, mut children: Vec<Range<usize>>) -> Vec<Range<usize>> {
    children.sort_by_key(|c| c.start);
    let mut out = Vec::new();
    let mut at = range.start;
    for c in children {
        if c.start > at {
            out.push(at..c.start.min(range.end));
        }
        at = at.max(c.end);
    }
    if at < range.end {
        out.push(at..range.end);
    }
    out
}

/// The `>` at the start of each line of a quote, with the space after each.
fn quote_marks(text: &str, range: &Range<usize>, out: &mut Vec<(Range<usize>, Style)>) {
    let bytes = text.as_bytes();
    let mut line = range.start;
    while line < range.end {
        let mut i = line;
        loop {
            while i < range.end && matches!(bytes[i], b' ' | b'\t') {
                i += 1;
            }
            if i >= range.end || bytes[i] != b'>' {
                break;
            }
            let end = if bytes.get(i + 1) == Some(&b' ') {
                i + 2
            } else {
                i + 1
            };
            out.push((i..end.min(range.end), Style::Mark));
            i = end;
        }
        line = line_end(text, line) + 1;
    }
}

/// `range` from the start of its first line (an indented code block's range starts after the
/// indent).
fn whole_lines(text: &str, range: &Range<usize>) -> Range<usize> {
    text[..range.start].rfind('\n').map_or(0, |n| n + 1)..range.end
}

/// The byte where the line holding `at` ends (its `\n`, or the text's end).
fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |n| at + n)
}

/// Bare links in `text` with the address each opens: URLs (GitHub's extended autolinks:
/// `http://`, `https://` or `www.` up to a space, less the punctuation that ends a sentence;
/// Todav's `todav://` too) and e-mail addresses.
fn bare_links(text: &str) -> Vec<(Range<usize>, String)> {
    let mut out: Vec<(Range<usize>, String)> = bare_urls(text)
        .into_iter()
        .map(|r| {
            let url = &text[r.clone()];
            let to = match url.starts_with("www.") {
                true => format!("https://{url}"),
                false => url.to_string(),
            };
            (r, to)
        })
        .collect();
    for r in emails(text) {
        if !out.iter().any(|(u, _)| u.start < r.end && r.start < u.end) {
            out.push((r.clone(), format!("mailto:{}", &text[r])));
        }
    }
    out.sort_by_key(|(r, _)| r.start);
    out
}

fn bare_urls(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = ["https://", "http://", "www.", "todav://"]
        .iter()
        .filter_map(|p| text[from..].find(p).map(|i| (from + i, p.len())))
        .min()
    {
        let (start, prefix) = at;
        let starts_word = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || "(*_~".contains(c));
        let mut end = text[start..]
            .find(|c: char| c.is_whitespace() || c == '<')
            .map_or(text.len(), |n| start + n);
        while end > start + prefix
            && text[..end].ends_with(['.', ',', ':', ';', '!', '?', '*', '_', '~', '\'', '"'])
        {
            end -= 1;
        }
        // A closing parenthesis without its opening one ends the sentence, not the URL.
        while text[start..end].ends_with(')')
            && text[start..end].matches(')').count() > text[start..end].matches('(').count()
        {
            end -= 1;
        }
        if starts_word && end > start + prefix {
            out.push(start..end);
        }
        from = end.max(start + prefix);
    }
    out
}

/// E-mail addresses in `text`: a local part, `@`, and a domain with a dot.
fn emails(text: &str) -> Vec<Range<usize>> {
    let local = |c: char| c.is_ascii_alphanumeric() || "._%+-".contains(c);
    let domain = |c: char| c.is_ascii_alphanumeric() || ".-".contains(c);
    let mut out = Vec::new();
    for (at, _) in text.match_indices('@') {
        let start = text[..at].rfind(|c| !local(c)).map_or(0, |i| i + 1);
        let mut end = text[at + 1..]
            .find(|c| !domain(c))
            .map_or(text.len(), |n| at + 1 + n);
        while text[..end].ends_with(['.', '-']) {
            end -= 1;
        }
        let host = &text[at + 1..end];
        if start < at && host.contains('.') && !host.starts_with('.') {
            out.push(start..end);
        }
    }
    out
}

/// Byte ranges as character ranges; empty ones dropped.
fn to_chars(text: &str, ranges: Vec<(Range<usize>, Style)>) -> Vec<Span> {
    let mut chars = vec![0usize; text.len() + 1];
    let mut n = 0;
    for (i, c) in text.char_indices() {
        for b in 0..c.len_utf8() {
            chars[i + b] = n;
        }
        n += 1;
    }
    chars[text.len()] = n;
    ranges
        .into_iter()
        .filter(|(r, _)| r.start < r.end && r.end <= text.len())
        .map(|(r, style)| Span {
            start: chars[r.start],
            end: chars[r.end],
            style,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pieces of `text` with `style`.
    fn of(text: &str, style: Style) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut found: Vec<(usize, String)> = spans(text)
            .into_iter()
            .filter(|s| s.style == style)
            .map(|s| (s.start, chars[s.start..s.end].iter().collect()))
            .collect();
        found.sort();
        found.into_iter().map(|(_, s)| s).collect()
    }

    #[test]
    fn formatting_and_its_marks() {
        let t = "# Plan\n\n**bold** *it* ~~gone~~ `code`\n\n> quoted\n\n- [ ] todo\n- [x] done\n";
        assert_eq!(of(t, Style::Heading(1)), ["# Plan\n"]);
        assert_eq!(of(t, Style::Strong), ["**bold**"]);
        assert_eq!(of(t, Style::Emphasis), ["*it*"]);
        assert_eq!(of(t, Style::Strikethrough), ["~~gone~~"]);
        assert_eq!(of(t, Style::Code), ["`code`"]);
        assert_eq!(of(t, Style::Quote), ["> quoted\n"]);
        assert_eq!(of(t, Style::Task(false)), ["[ ]"]);
        assert_eq!(of(t, Style::Task(true)), ["[x]"]);
        let marks = of(t, Style::Mark);
        for m in ["# ", "**", "*", "~~", "`", "> "] {
            assert!(marks.contains(&m.to_string()), "{m:?} in {marks:?}");
        }
    }

    #[test]
    fn plain_text_stays_plain() {
        let t = "Buy milk\nand bread, 2 * 3 = 6";
        assert!(spans(t).is_empty(), "{:?}", spans(t));
    }

    #[test]
    fn links_open_their_address() {
        let t = "Zażółć [the *text*](https://a.b/c \"t\") <me@f.g> see www.h.i/j_(k). \
                 `https://code` and https://l.m, mail ann.b+c@d-e.fg. todav://task/x1 [rel](x.md)";
        let at = |s: &str| link_at(t, t[..t.find(s).unwrap()].chars().count());
        assert_eq!(at("text").as_deref(), Some("https://a.b/c"));
        assert_eq!(at("me@").as_deref(), Some("mailto:me@f.g"));
        assert_eq!(at("(k)").as_deref(), Some("https://www.h.i/j_(k)"));
        assert_eq!(at("code"), None);
        assert_eq!(at("l.m").as_deref(), Some("https://l.m"));
        assert_eq!(at("ann").as_deref(), Some("mailto:ann.b+c@d-e.fg"));
        assert_eq!(at("task/").as_deref(), Some("todav://task/x1"));
        assert_eq!(at("rel"), None);
        assert_eq!(at("see"), None);
        assert_eq!(at("Zażółć"), None);
    }

    #[test]
    fn bare_links_stop_at_punctuation() {
        let urls = |t: &str| -> Vec<String> { bare_links(t).into_iter().map(|(_, u)| u).collect() };
        assert_eq!(
            urls("(see www.a.b/c), https://d.e?f=1. no:http://x a@b @c.d e@f."),
            ["https://www.a.b/c", "https://d.e?f=1"]
        );
        // An address in a URL is the URL's.
        assert_eq!(urls("https://x.y/u@v.w"), ["https://x.y/u@v.w"]);
    }

    #[test]
    fn boxes_tick_and_untick() {
        let t = "Zażółć\n- [ ] one\n- [x] two\n";
        let one = t[..t.find("[ ]").unwrap()].chars().count();
        let two = t[..t.find("[x]").unwrap()].chars().count();
        assert_eq!(
            toggle_task(t, one + 1).as_deref(),
            Some("Zażółć\n- [x] one\n- [x] two\n")
        );
        assert_eq!(
            toggle_task(t, two).as_deref(),
            Some("Zażółć\n- [ ] one\n- [ ] two\n")
        );
        assert_eq!(toggle_task(t, 0), None);
        assert_eq!(
            toggle_nth_task(t, 1).as_deref(),
            Some("Zażółć\n- [ ] one\n- [ ] two\n")
        );
        assert_eq!(toggle_nth_task(t, 2), None);
    }

    #[test]
    fn read_without_marks() {
        let t = "# Plan\n**Buy** [milk](https://a.b) at<br>noon\n\n- [ ] one\n- [x] two\n* three\n\n<p>kept</p>\n";
        let r = read(t);
        assert_eq!(
            r.text,
            "Plan\nBuy milk at\nnoon\n\n☐ one\n☑ two\n• three\n\n<p>kept</p>"
        );
        let of = |style: Style| -> Vec<String> {
            let chars: Vec<char> = r.text.chars().collect();
            r.spans
                .iter()
                .filter(|s| s.style == style)
                .map(|s| chars[s.start..s.end].iter().collect())
                .collect()
        };
        assert_eq!(of(Style::Heading(1)), ["Plan\n"]);
        assert_eq!(of(Style::Strong), ["Buy"]);
        assert_eq!(of(Style::Link(Some("https://a.b".into()))), ["milk"]);
        assert_eq!(of(Style::Task(false)), ["☐"]);
        assert_eq!(of(Style::Task(true)), ["☑"]);
    }

    #[test]
    fn read_without_gaps_and_long_addresses() {
        let long = "https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc%40thread.v2/0";
        let t = format!("\n  \nCaution  \n\n \n\n\nJoin **now** {long}\n\nhttps://a.b/c\n \n\n");
        let r = read(&t);
        assert_eq!(
            r.text,
            "Caution\n\nJoin now teams.microsoft.com/l/meetup-join/19%3a…\n\nhttps://a.b/c"
        );
        let chars: Vec<char> = r.text.chars().collect();
        let piece = |style: Style| -> Vec<String> {
            r.spans
                .iter()
                .filter(|s| s.style == style)
                .map(|s| chars[s.start..s.end].iter().collect())
                .collect()
        };
        assert_eq!(piece(Style::Strong), ["now"]);
        assert_eq!(
            piece(Style::Link(Some(long.into()))),
            ["teams.microsoft.com/l/meetup-join/19%3a…"]
        );
        assert_eq!(
            piece(Style::Link(Some("https://a.b/c".into()))),
            ["https://a.b/c"]
        );
        assert_eq!(read(" \n\n").text, "");
    }
}
