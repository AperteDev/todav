//! The week view: seven columns of hours with events and planned tasks as blocks, all-day items
//! above. Blocks are children of an overlay, placed by `Geo`; the drawing area under them draws
//! the hour lines, the drop preview and the current time.

use super::*;

/// Height of an hour, and width of the hour labels on the left.
const HOUR: f64 = 48.0;
const GUTTER: i32 = 56;

/// Minutes a dropped task snaps to.
const SNAP: i64 = 15;

/// Where a block goes: day of the week (0 = Monday), minutes into the day, and its column among
/// the blocks it overlaps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geo {
    day: i64,
    from: i64,
    to: i64,
    col: usize,
    cols: usize,
}

/// Width of one day column in an overlay `width` wide.
fn col_width(width: i32) -> f64 {
    (width - GUTTER).max(7) as f64 / 7.0
}

pub(super) fn setup(c: &Rc<Cal>) {
    let height = (24.0 * HOUR) as i32;
    c.week_lines.set_content_height(height);
    for h in 0..24 {
        let l = gtk::Label::builder()
            .label(format!("{h:02}:00"))
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .yalign(0.0)
            .height_request(HOUR as i32)
            .margin_end(8)
            .build();
        l.set_width_request(GUTTER - 8);
        c.hours.append(&l);
    }
    let weak = Rc::downgrade(c);
    c.week_lines.set_draw_func(move |area, cr, w, h| {
        if let Some(c) = weak.upgrade() {
            draw(&c, area, cr, w, h);
        }
    });
    let weak = Rc::downgrade(c);
    c.week_grid
        .connect_get_child_position(move |overlay, child| {
            let c = weak.upgrade()?;
            let blocks = c.blocks.borrow();
            let (_, g) = blocks.iter().find(|(w, _)| w == child)?;
            let colw = col_width(overlay.width());
            let x = GUTTER as f64 + colw * g.day as f64 + colw * g.col as f64 / g.cols as f64;
            let y = g.from as f64 / 60.0 * HOUR;
            let h = ((g.to - g.from) as f64 / 60.0 * HOUR).max(24.0);
            let w = colw / g.cols as f64;
            Some(gdk::Rectangle::new(
                x as i32 + 1,
                y as i32 + 1,
                (w as i32 - 2).max(4),
                h as i32 - 2,
            ))
        });

    // Dragging over the hours shows where it lands: a task is planned there, an event moves.
    let drop = item_drop(
        &c.week_grid,
        |_| true,
        |i| {
            let Some(c) = cal() else { return };
            if let Some((day, minute)) = c.hover.take() {
                let at = week_start(c.anchor.get()) + day * DAY + minute * 60;
                match i {
                    Item::Task(t) => plan_at(t, at),
                    Item::Event(e) => move_event(
                        e,
                        EventTime {
                            start: at,
                            end: at + drop_length(i) * 60,
                            all_day: false,
                        },
                    ),
                }
            }
            c.week_lines.queue_draw();
        },
    );
    // A double click or a long press on a free hour adds an event there.
    new_event_on(&c.week_grid, |c, x, y| {
        let (day, minute) = slot(c, x, y)?;
        let at = week_start(c.anchor.get()) + day * DAY + minute / 30 * 30 * 60;
        Some(EventTime {
            start: at,
            end: at + 3600,
            all_day: false,
        })
    });
    let weak = Rc::downgrade(c);
    drop.connect_motion(move |_, x, y| {
        if let Some(c) = weak.upgrade() {
            c.hover.set(slot(&c, x, y));
            c.week_lines.queue_draw();
        }
        gdk::DragAction::MOVE
    });
    let weak = Rc::downgrade(c);
    drop.connect_leave(move |_| {
        if let Some(c) = weak.upgrade() {
            c.hover.set(None);
            c.week_lines.queue_draw();
        }
    });
    // Keep the current-time line moving.
    let weak = Rc::downgrade(c);
    glib::timeout_add_seconds_local(60, move || match weak.upgrade() {
        Some(c) => {
            c.week_lines.queue_draw();
            glib::ControlFlow::Continue
        }
        None => glib::ControlFlow::Break,
    });
}

/// The (day, minute) slot under a point of the grid.
fn slot(c: &Cal, x: f64, y: f64) -> Option<(i64, i64)> {
    let colw = col_width(c.week_grid.width());
    let day = ((x - GUTTER as f64) / colw).floor() as i64;
    if !(0..7).contains(&day) {
        return None;
    }
    let minute = (y / HOUR * 60.0) as i64 / SNAP * SNAP;
    Some((day, minute.clamp(0, 24 * 60 - SNAP)))
}

/// Length in minutes of what a dropped task becomes (for the drop preview).
fn drop_length(i: &Item) -> i64 {
    match i {
        Item::Task(t) => match (t.start, slot_end(t)) {
            (Some(s), Some(e)) => (e - s) / 60,
            _ => 60,
        },
        Item::Event(e) if !e.all_day && e.end > e.start => (e.end - e.start) / 60,
        Item::Event(_) => 60,
    }
}

fn draw(c: &Cal, area: &gtk::DrawingArea, cr: &gtk::cairo::Context, w: i32, h: i32) {
    let fg = area.color();
    let (r, g, b) = (fg.red() as f64, fg.green() as f64, fg.blue() as f64);
    let colw = col_width(w + GUTTER);
    let start = week_start(c.anchor.get());
    // Today's column.
    let today_col = (today() - start) / DAY;
    if (0..7).contains(&today_col) {
        cr.set_source_rgba(r, g, b, 0.04);
        cr.rectangle(colw * today_col as f64, 0.0, colw, h as f64);
        let _ = cr.fill();
    }
    cr.set_line_width(1.0);
    for hour in 0..24 {
        let y = (hour as f64 * HOUR).floor() + 0.5;
        cr.set_source_rgba(r, g, b, 0.12);
        cr.move_to(0.0, y);
        cr.line_to(w as f64, y);
        let _ = cr.stroke();
        let y = (hour as f64 * HOUR + HOUR / 2.0).floor() + 0.5;
        cr.set_source_rgba(r, g, b, 0.05);
        cr.move_to(0.0, y);
        cr.line_to(w as f64, y);
        let _ = cr.stroke();
    }
    cr.set_source_rgba(r, g, b, 0.12);
    for d in 0..7 {
        let x = (colw * d as f64).floor() + 0.5;
        cr.move_to(x, 0.0);
        cr.line_to(x, h as f64);
        let _ = cr.stroke();
    }
    // Where a dragged task would land.
    if let (Some((day, minute)), Some(t)) = (c.hover.get(), dragging()) {
        let accent = adw::StyleManager::default().accent_color_rgba();
        let (ar, ag, ab) = (
            accent.red() as f64,
            accent.green() as f64,
            accent.blue() as f64,
        );
        let y = minute as f64 / 60.0 * HOUR;
        let hgt = drop_length(&t) as f64 / 60.0 * HOUR;
        cr.set_source_rgba(ar, ag, ab, 0.25);
        cr.rectangle(colw * day as f64 + 2.0, y, colw - 4.0, hgt);
        let _ = cr.fill();
    }
    // Now.
    if (0..7).contains(&today_col) {
        let now = core().local_now();
        let y = (now - today()) as f64 / 3600.0 * HOUR;
        let x = colw * today_col as f64;
        cr.set_source_rgb(0.88, 0.11, 0.14);
        cr.set_line_width(2.0);
        cr.move_to(x, y);
        cr.line_to(x + colw, y);
        let _ = cr.stroke();
        cr.arc(x + 1.0, y, 4.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
}

/// A timed item's part within one day: (day of the week, from, to in minutes into the day).
fn segments(i: &Item, start: i64) -> Vec<(i64, i64, i64)> {
    let (from, to) = match i {
        Item::Event(e) if !e.all_day => (e.start, e.end.max(e.start + 15 * 60)),
        // Only tasks planned into a slot (a start time) take hours; one just due at a time goes
        // above with the day's other items, its chip showing the time.
        Item::Task(t) => match t.start {
            Some(s) if !t.start_all_day => (s, slot_end(t).unwrap_or(s + 30 * 60)),
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    (0..7)
        .filter_map(|d| {
            let day = start + d * DAY;
            let (a, b) = (from.max(day), to.min(day + DAY));
            (a < b).then(|| (d, (a - day) / 60, (b - day) / 60))
        })
        .collect()
}

/// Columns for the blocks of one day: overlapping ones side by side.
fn columns(mut segs: Vec<(usize, i64, i64)>) -> Vec<(usize, usize, usize)> {
    segs.sort_by_key(|&(_, a, b)| (a, -b));
    let mut out = Vec::new();
    let mut cluster: Vec<(usize, i64, usize)> = Vec::new(); // (index, end, column)
    let mut cluster_end = i64::MIN;
    let flush = |cluster: &mut Vec<(usize, i64, usize)>, out: &mut Vec<(usize, usize, usize)>| {
        let cols = cluster.iter().map(|c| c.2 + 1).max().unwrap_or(1);
        out.extend(cluster.drain(..).map(|(i, _, col)| (i, col, cols)));
    };
    for (i, a, b) in segs {
        if a >= cluster_end {
            flush(&mut cluster, &mut out);
        }
        let col = (0..)
            .find(|col| !cluster.iter().any(|&(_, end, c)| c == *col && end > a))
            .unwrap_or(0);
        cluster.push((i, b, col));
        cluster_end = cluster_end.max(b);
    }
    flush(&mut cluster, &mut out);
    out
}

/// Draws the week holding the anchor; returns its end.
pub(super) fn render(c: &Rc<Cal>) -> i64 {
    let start = week_start(c.anchor.get());
    let end = start + 7 * DAY;
    let last = end - DAY;
    let title = if fmt(start, "%m") == fmt(last, "%m") {
        format!("{} – {}", fmt(start, "%-d"), fmt(last, "%-d %OB %Y"))
    } else {
        format!("{} – {}", fmt(start, "%-d %b"), fmt(last, "%-d %b %Y"))
    };
    c.title.set_title(&title);
    c.title.set_subtitle("");

    // Day names, then the all-day row, both after a gutter as wide as the hour labels.
    clear(&c.week_head);
    clear(&c.week_all_day);
    let gutter = || gtk::Box::builder().width_request(GUTTER).build();
    c.week_head.append(&gutter());
    c.week_all_day.append(&gutter());
    let head = gtk::Box::builder().homogeneous(true).hexpand(true).build();
    c.week_head.append(&head);
    let items = items(start, end);
    for d in 0..7 {
        let day = start + d * DAY;
        let name = gtk::Label::new(Some(&fmt(day, "%a %-d")));
        if day == today() {
            name.add_css_class("today");
        }
        head.append(&name);
    }
    // What has no hours: all-day items, a bar across their days.
    let all_day: Vec<&Item> = items
        .iter()
        .filter(|i| segments(i, start).is_empty())
        .collect();
    let row = days::row(c, start, &all_day, usize::MAX, true, |day| {
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cell.add_css_class("all-day-cell");
        day_drop(&cell, day, true);
        cell
    });
    row.set_hexpand(true);
    c.week_all_day.append(&row);

    // The hours.
    for (w, _) in c.blocks.borrow_mut().drain(..) {
        c.week_grid.remove_overlay(&w);
    }
    let mut placed = Vec::new();
    for d in 0..7 {
        let segs: Vec<(usize, i64, i64, i64)> = items
            .iter()
            .enumerate()
            .flat_map(|(n, i)| {
                segments(i, start)
                    .into_iter()
                    .filter(move |s| s.0 == d)
                    .map(move |(_, a, b)| (n, a, b, d))
            })
            .collect();
        let laid = columns(
            segs.iter()
                .enumerate()
                .map(|(k, s)| (k, s.1, s.2))
                .collect(),
        );
        for (k, col, cols) in laid {
            let (n, from, to, day) = segs[k];
            placed.push((
                n,
                Geo {
                    day,
                    from,
                    to,
                    col,
                    cols,
                },
            ));
        }
    }
    for (n, geo) in placed {
        let w = block(c, &items[n], geo);
        c.week_grid.add_overlay(&w);
        c.blocks.borrow_mut().push((w, geo));
    }
    c.week_lines.queue_draw();
    // Open on the morning (or the current hour) the first time.
    if !c.scrolled.replace(true) {
        let hour = if (start..end).contains(&today()) {
            ((core().local_now() - today()) / 3600 - 1).clamp(0, 7)
        } else {
            7
        };
        let adj = c.week_scroll.vadjustment();
        glib::idle_add_local_once(move || adj.set_value(hour as f64 * HOUR));
    }
    end
}

/// An event (opens its details) or a planned task (tick, open, drag elsewhere) in the hours.
fn block(c: &Rc<Cal>, i: &Item, geo: Geo) -> gtk::Widget {
    let time = |t: i64| fmt(t, "%H:%M");
    let text = |summary: &str, from: i64| {
        // Short blocks get one line.
        if geo.to - geo.from >= 45 {
            format!("{summary}\n{}", time(from))
        } else {
            format!("{} {summary}", time(from))
        }
    };
    let label = |s: String| {
        gtk::Label::builder()
            .label(&s)
            .xalign(0.0)
            .yalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .vexpand(true)
            .build()
    };
    match i {
        Item::Event(e) => {
            let b = gtk::MenuButton::builder()
                .child(&label(text(&e.summary, e.start)))
                .has_frame(false)
                .build();
            b.add_css_class("block");
            b.add_css_class("chip");
            b.add_css_class(&class(c, &e.list_href));
            if e.cancelled {
                b.add_css_class("cancelled");
            }
            drag_event(c, &b, e);
            let (weak, e) = (Rc::downgrade(c), e.clone());
            b.set_create_popup_func(move |b| {
                if let Some(c) = weak.upgrade() {
                    b.set_popover(Some(&details(&c, &e)));
                }
            });
            b.upcast()
        }
        Item::Task(t) => {
            let b = task_chip(c, t, &label(text(&t.summary, when(t).0)), gtk::Align::Start);
            b.add_css_class("block");
            b.upcast()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_blocks_share_the_width() {
        // a 9-10, b 9:30-11, c 10-10:30 (beside b), d 12-13 alone.
        let laid = columns(vec![
            (0, 540, 600),
            (1, 570, 660),
            (2, 600, 630),
            (3, 720, 780),
        ]);
        let at = |i| *laid.iter().find(|l| l.0 == i).unwrap();
        assert_eq!(at(0), (0, 0, 2));
        assert_eq!(at(1), (1, 1, 2));
        assert_eq!(at(2), (2, 0, 2), "c takes a's freed column");
        assert_eq!(at(3), (3, 0, 1));
    }
}
