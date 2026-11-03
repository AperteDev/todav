//! Notes of tasks and events as light Markdown (`todav_app::note`): styled where they are
//! edited (a click ticks a box, Ctrl+click opens a link) and shown without their marks where
//! they are read, links opening on a click.

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use todav_app::note::{self, Style};

/// Open `url`: a `todav://` link in Todav, any other in the app for it.
pub fn open(url: &str, widget: &impl IsA<gtk::Widget>) {
    if url.starts_with("todav:") {
        crate::open::show(url);
        return;
    }
    let window = widget.root().and_downcast::<gtk::Window>();
    gtk::UriLauncher::new(url).launch(window.as_ref(), gio::Cancellable::NONE, |_| {});
}

/// A label showing note `text` as read, wrapped at about 40 characters. Its boxes tick when
/// `save` is given, which saves the note ticked.
pub fn label(text: &str, save: Option<Box<dyn Fn(String)>>) -> gtk::Label {
    let l = gtk::Label::builder()
        .label(markup(text, save.is_some()))
        .use_markup(true)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .max_width_chars(40)
        .build();
    let text = Rc::new(RefCell::new(text.to_string()));
    let save: Option<Rc<dyn Fn(String)>> = save.map(Rc::from);
    l.connect_activate_link(move |l, url| {
        let tick = url.strip_prefix(BOX).and_then(|n| n.parse().ok());
        match (tick, &save) {
            // After the click: a label's links may not change while one is activated.
            (Some(n), Some(save)) => {
                let (l, text, save) = (l.downgrade(), text.clone(), save.clone());
                glib::idle_add_local_once(move || {
                    let ticked = note::toggle_nth_task(&text.borrow(), n);
                    if let (Some(ticked), Some(l)) = (ticked, l.upgrade()) {
                        l.set_markup(&markup(&ticked, true));
                        *text.borrow_mut() = ticked.clone();
                        save(ticked);
                    }
                });
            }
            _ => open(url, l),
        }
        glib::Propagation::Stop
    });
    l
}

/// What a box's link in a label is to, before its number.
const BOX: &str = "todav-box:";

/// Pango markup of note `text` as read (`note::read`); its boxes links (`BOX`) when `tick`.
fn markup(text: &str, tick: bool) -> String {
    let read = note::read(text);
    let mut boxes: Vec<usize> = read
        .spans
        .iter()
        .filter(|s| matches!(s.style, Style::Task(_)))
        .map(|s| s.start)
        .collect();
    boxes.sort();
    let chars: Vec<char> = read.text.chars().collect();
    let active = |i: usize| -> Vec<&note::Span> {
        read.spans
            .iter()
            .filter(|s| s.start <= i && i < s.end)
            .collect()
    };
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let here = active(i);
        let mut j = i + 1;
        while j < chars.len() && active(j) == here {
            j += 1;
        }
        // Links outermost, each run's tags opened and closed in it, so they always nest.
        let mut tags: Vec<(String, &str)> = Vec::new();
        for s in &here {
            let url = match &s.style {
                Style::Link(Some(url)) => glib::markup_escape_text(url).to_string(),
                Style::Task(_) if tick => {
                    let n = boxes.iter().position(|&b| b == s.start).unwrap_or_default();
                    format!("{BOX}{n}")
                }
                _ => continue,
            };
            tags.insert(0, (format!("<a href=\"{url}\">"), "</a>"));
        }
        for s in &here {
            tags.push(match s.style {
                Style::Heading(1 | 2) => {
                    ("<span weight=\"bold\" size=\"larger\">".into(), "</span>")
                }
                Style::Heading(_) | Style::Strong => ("<b>".into(), "</b>"),
                Style::Emphasis | Style::Quote => ("<i>".into(), "</i>"),
                Style::Strikethrough => ("<s>".into(), "</s>"),
                Style::Code | Style::CodeBlock => ("<tt>".into(), "</tt>"),
                Style::Link(None) => ("<u>".into(), "</u>"),
                Style::Task(_) => ("<span size=\"x-large\">".into(), "</span>"),
                _ => continue,
            });
        }
        for (open, _) in &tags {
            out.push_str(open);
        }
        out.push_str(&glib::markup_escape_text(
            &chars[i..j].iter().collect::<String>(),
        ));
        for (_, close) in tags.iter().rev() {
            out.push_str(close);
        }
        i = j;
    }
    out
}

/// The tags styling a note where it is edited, by style.
fn tag(style: &Style) -> Option<&'static str> {
    Some(match style {
        Style::Heading(_) => "note-heading",
        Style::Strong => "note-strong",
        Style::Emphasis | Style::Quote => "note-emphasis",
        Style::Strikethrough => "note-strike",
        Style::Code | Style::CodeBlock => "note-code",
        Style::Link(_) => "note-link",
        Style::Mark => "note-mark",
        Style::Task(_) | Style::ListMark => return None,
    })
}

/// Style the note edited in `view` as it changes; a click on a box ticks it, Ctrl+click on a
/// link opens it.
pub fn attach(view: &gtk::TextView) {
    let buffer = view.buffer();
    let table = buffer.tag_table();
    let add = |t: gtk::TextTag| {
        table.add(&t);
    };
    add(gtk::TextTag::builder()
        .name("note-heading")
        .weight(700)
        .scale(1.2)
        .build());
    add(gtk::TextTag::builder()
        .name("note-strong")
        .weight(700)
        .build());
    add(gtk::TextTag::builder()
        .name("note-emphasis")
        .style(gtk::pango::Style::Italic)
        .build());
    add(gtk::TextTag::builder()
        .name("note-strike")
        .strikethrough(true)
        .build());
    add(gtk::TextTag::builder()
        .name("note-code")
        .family("monospace")
        .build());
    add(gtk::TextTag::builder()
        .name("note-link")
        .underline(gtk::pango::Underline::Single)
        .build());
    let mut dim = view.color();
    dim.set_alpha(0.5);
    add(gtk::TextTag::builder()
        .name("note-mark")
        .foreground_rgba(&dim)
        .build());
    buffer.connect_changed(restyle);
    restyle(&buffer);

    // Before the text view's own, which would move the cursor there.
    let click = gtk::GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed(|gesture, n, x, y| {
        let Some(view) = gesture.widget().and_downcast::<gtk::TextView>() else {
            return;
        };
        let mods = gesture.current_event_state() & gtk::accelerator_get_default_mod_mask();
        let done = n == 1
            && match mods {
                m if m.is_empty() => toggle_task(&view, x, y),
                gdk::ModifierType::CONTROL_MASK => match at(&view, x, y).and_then(link) {
                    Some(url) => {
                        open(&url, &view);
                        true
                    }
                    None => false,
                },
                _ => false,
            };
        if done {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    view.add_controller(click);
    // A hand over what a click acts on: a box, a link with Ctrl.
    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion(|motion, x, y| {
        let Some(view) = motion.widget().and_downcast::<gtk::TextView>() else {
            return;
        };
        let ctrl = motion
            .current_event_state()
            .contains(gdk::ModifierType::CONTROL_MASK);
        let hand = at(&view, x, y)
            .is_some_and(|(text, i)| task(&text, i).is_some() || ctrl && link((text, i)).is_some());
        view.set_cursor_from_name(Some(if hand { "pointer" } else { "text" }));
    });
    view.add_controller(motion);
}

fn restyle(buffer: &gtk::TextBuffer) {
    let (start, end) = buffer.bounds();
    buffer.remove_all_tags(&start, &end);
    for s in note::spans(&buffer.text(&start, &end, true)) {
        if let Some(name) = tag(&s.style) {
            buffer.apply_tag_by_name(
                name,
                &buffer.iter_at_offset(s.start as i32),
                &buffer.iter_at_offset(s.end as i32),
            );
        }
    }
}

/// The note's text and the character under the widget's point `x`, `y`.
fn at(view: &gtk::TextView, x: f64, y: f64) -> Option<(String, usize)> {
    let (x, y) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    let i = view.iter_at_location(x, y)?;
    let (start, end) = view.buffer().bounds();
    Some((
        view.buffer().text(&start, &end, true).into(),
        i.offset() as usize,
    ))
}

fn link((text, at): (String, usize)) -> Option<String> {
    note::link_at(&text, at)
}

/// The start of the box (`[ ]`, `[x]`) at character `at`, and whether it is ticked.
fn task(text: &str, at: usize) -> Option<(usize, bool)> {
    note::spans(text).into_iter().find_map(|s| match s.style {
        Style::Task(done) if (s.start..s.end).contains(&at) => Some((s.start, done)),
        _ => None,
    })
}

/// Tick or untick the box under the point, as one step to undo; whether there was one.
fn toggle_task(view: &gtk::TextView, x: f64, y: f64) -> bool {
    let Some((start, done)) = at(view, x, y).and_then(|(text, i)| task(&text, i)) else {
        return false;
    };
    let buffer = view.buffer();
    let mut from = buffer.iter_at_offset(start as i32 + 1);
    let mut to = buffer.iter_at_offset(start as i32 + 2);
    buffer.begin_user_action();
    buffer.delete(&mut from, &mut to);
    buffer.insert(&mut from, if done { " " } else { "x" });
    buffer.end_user_action();
    true
}
