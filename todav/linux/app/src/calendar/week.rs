//! The hours of the Week, Work Week, 3 Days and Day views: a column of hours a day with events and
//! planned tasks as blocks, all-day items above. Blocks are children of an overlay, placed by
//! `Geo`, as is the drop preview; the drawing area under them draws the hour lines and the
//! current time.

use super::*;

/// Height of an hour, and width of the hour labels on the left.
const HOUR: f64 = 48.0;
const GUTTER: i32 = 56;

/// Minutes a dropped task snaps to.
const SNAP: i64 = 15;

/// Where a block goes: the day's column (0 = the first day shown), minutes into the day, and its
/// column among the blocks it overlaps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geo {
    day: i64,
    from: i64,
    to: i64,
    col: usize,
    cols: usize,
}

/// Width of one day column in an overlay `width` wide.
fn col_width(c: &Cal, width: i32) -> f64 {
    let n = c.days.borrow().len().max(1);
    (width - GUTTER).max(n as i32) as f64 / n as f64
}

/// Midnight of the day in column `col`.
fn day_at(c: &Cal, col: i64) -> Option<i64> {
    c.days.borrow().get(usize::try_from(col).ok()?).copied()
}

/// The days the mode shows around the anchor.
fn shown(c: &Cal) -> Vec<i64> {
    let a = c.anchor.get();
    match mode(c) {
        Mode::WorkWeek => todav_app::dates::days_of_week(
            a,
            crate::i18n::first_weekday(),
            &prefs::work_days(core()),
        ),
        Mode::ThreeDays => (0..3).map(|d| a + d * DAY).collect(),
        Mode::Day => vec![a],
        _ => (0..7).map(|d| week_start(a) + d * DAY).collect(),
    }
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
            if child == c.preview.upcast_ref::<gtk::Widget>() {
                let (day, from, to) = c.span.get()?;
                let colw = col_width(&c, overlay.width());
                let h = ((to - from) as f64 / 60.0 * HOUR).max(24.0);
                return Some(gdk::Rectangle::new(
                    (GUTTER as f64 + colw * day as f64) as i32 + 2,
                    (from as f64 / 60.0 * HOUR) as i32,
                    (colw as i32 - 4).max(4),
                    h as i32,
                ));
            }
            let blocks = c.blocks.borrow();
            let (_, g, _) = blocks.iter().find(|(w, _, _)| w == child)?;
            let (x, y, w, h) = rect(&c, overlay.width(), g);
            Some(gdk::Rectangle::new(x as i32, y as i32, w as i32, h as i32))
        });

    // Dragging over the hours shows where it lands: a task is planned there, an event moves.
    let drop = item_drop(
        &c.week_grid,
        |_| true,
        |i| {
            let Some(c) = cal() else { return };
            if let Some((day, minute)) = c.hover.take()
                && let Some(day) = day_at(&c, day)
            {
                apply(i, calendar::slot_drop(core(), i, day + minute * 60));
            }
            show_preview(&c, None);
        },
    );
    // A double click or a long press on a free hour adds an event there.
    new_event_on(&c.week_grid, |c, x, y| {
        let (day, minute) = slot(c, x, y)?;
        let at = day_at(c, day)? + minute / 30 * 30 * 60;
        Some(EventTime {
            start: at,
            end: at + prefs::event_duration(core()),
            all_day: false,
        })
    });
    c.week_grid.add_overlay(&c.preview);
    let weak = Rc::downgrade(c);
    drop.connect_motion(move |_, x, y| {
        let (Some(c), Some(i)) = (weak.upgrade(), dragging()) else {
            return gdk::DragAction::MOVE;
        };
        // Where the dragged block's top would be, to the nearest slot.
        let top = y - GRAB.get() + HOUR * SNAP as f64 / 120.0;
        let hover = slot(&c, x, top);
        c.hover.set(hover);
        show_preview(
            &c,
            hover.map(|(day, m)| (&i, (day, m, m + calendar::drop_length(core(), &i)))),
        );
        gdk::DragAction::MOVE
    });
    let weak = Rc::downgrade(c);
    drop.connect_leave(move |_| {
        if let Some(c) = weak.upgrade() {
            c.hover.set(None);
            show_preview(&c, None);
        }
    });
    resizing(c);
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
    let colw = col_width(c, c.week_grid.width());
    let day = ((x - GUTTER as f64) / colw).floor() as i64;
    if !(0..c.days.borrow().len() as i64).contains(&day) {
        return None;
    }
    let minute = (y / HOUR * 60.0) as i64 / SNAP * SNAP;
    Some((day, minute.clamp(0, 24 * 60 - SNAP)))
}

/// Show the preview block of `item` over `span` (day, from, to in minutes) with its title and
/// times, laid out as blocks are; or hide it.
pub(super) fn show_preview(c: &Cal, at: Option<(&Item, (i64, i64, i64))>) {
    let span = at.map(|(_, s)| s);
    if span == c.span.get() {
        return;
    }
    c.span.set(span);
    if let Some((i, (_, from, to))) = at {
        let time = |m: i64| fmt(today() + m * 60, "%H:%M");
        let times = format!("{} – {}", time(from), time(to));
        let title = match i {
            Item::Event(e) => &e.summary,
            Item::Task(t) => &t.summary,
        };
        c.preview.set_label(&if to - from >= 45 {
            format!("{title}\n{times}")
        } else {
            format!("{times} {title}")
        });
        // Over the blocks (last drawn), which re-rendering puts after it.
        c.preview.insert_before(&c.week_grid, gtk::Widget::NONE);
    }
    c.preview.set_visible(span.is_some());
    c.week_grid.queue_allocate();
    CARD.with(|card| {
        if let Some(card) = &*card.borrow() {
            // Not quite 0: a drag icon with nothing to draw keeps showing its last frame.
            card.set_opacity(if span.is_some() { 0.01 } else { 1.0 });
        }
    });
}

/// Dragging a block's top or bottom edge changes when it starts or ends, in steps of `SNAP`;
/// the pointer shows a resize cursor over those edges.
fn resizing(c: &Rc<Cal>) {
    // The block being resized, and whether by its top.
    let held: Rc<RefCell<Option<(Item, Geo, bool)>>> = Rc::default();
    let drag = gtk::GestureDrag::new();
    // Before the block's own click and drag, which an edge takes from it.
    drag.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (weak, h) = (Rc::downgrade(c), held.clone());
    drag.connect_drag_begin(move |g, x, y| {
        let edge = weak.upgrade().and_then(|c| edge_at(&c, x, y));
        g.set_state(if edge.is_some() {
            gtk::EventSequenceState::Claimed
        } else {
            gtk::EventSequenceState::Denied
        });
        *h.borrow_mut() = edge;
    });
    let (weak, h) = (Rc::downgrade(c), held.clone());
    drag.connect_drag_update(move |g, _, dy| {
        let (Some(c), Some((i, geo, top)), Some((_, y))) =
            (weak.upgrade(), h.borrow().clone(), g.start_point())
        else {
            return;
        };
        let minute = ((y + dy) / HOUR * 60.0 / SNAP as f64).round() as i64 * SNAP;
        let span = if top {
            (geo.day, minute.clamp(0, geo.to - SNAP), geo.to)
        } else {
            (geo.day, geo.from, minute.clamp(geo.from + SNAP, 24 * 60))
        };
        show_preview(&c, Some((&i, span)));
    });
    let weak = Rc::downgrade(c);
    drag.connect_drag_end(move |_, _, _| {
        let (Some(c), Some((i, geo, top))) = (weak.upgrade(), held.borrow_mut().take()) else {
            return;
        };
        let span = c.span.get();
        show_preview(&c, None);
        let Some((day, from, to)) = span.filter(|s| (s.1, s.2) != (geo.from, geo.to)) else {
            return;
        };
        let Some(day) = day_at(&c, day) else { return };
        let (from, to) = (day + from * 60, day + to * 60);
        apply(&i, calendar::resize(&i, from, to, top));
    });
    c.week_grid.add_controller(drag);
    let motion = gtk::EventControllerMotion::new();
    let weak = Rc::downgrade(c);
    motion.connect_motion(move |_, x, y| {
        if let Some(c) = weak.upgrade() {
            let edge = edge_at(&c, x, y).is_some();
            c.week_grid
                .set_cursor_from_name(edge.then_some("ns-resize"));
        }
    });
    c.week_grid.add_controller(motion);
}

/// The block edge under a point of the grid: its item, its place and whether it is the top. Only
/// an item's real start or end (not where a longer one crosses midnight) of one we may change.
fn edge_at(c: &Cal, x: f64, y: f64) -> Option<(Item, Geo, bool)> {
    let width = c.week_grid.width();
    let blocks = c.blocks.borrow();
    blocks.iter().find_map(|(_, g, i)| {
        let (bx, by, bw, bh) = rect(c, width, g);
        if x < bx || x > bx + bw || y < by || y > by + bh {
            return None;
        }
        let zone = (bh / 4.0).min(6.0);
        let top = if y < by + zone {
            true
        } else if y > by + bh - zone {
            false
        } else {
            return None;
        };
        let (from, to) = calendar::span(core(), i)?;
        let day = day_at(c, g.day)?;
        let real = if top {
            from == day + g.from * 60
        } else {
            to == day + g.to * 60
        };
        let ours = match i {
            Item::Event(e) => writable(c, &e.list_href),
            Item::Task(_) => true,
        };
        (real && ours).then(|| (i.clone(), *g, top))
    })
}

/// Where `g`'s block goes in an overlay `width` wide: x, y, width, height.
fn rect(c: &Cal, width: i32, g: &Geo) -> (f64, f64, f64, f64) {
    let colw = col_width(c, width);
    let x = GUTTER as f64 + colw * g.day as f64 + colw * g.col as f64 / g.cols as f64;
    let y = g.from as f64 / 60.0 * HOUR;
    let h = ((g.to - g.from) as f64 / 60.0 * HOUR).max(24.0);
    (
        x + 1.0,
        y + 1.0,
        (colw / g.cols as f64 - 2.0).max(4.0),
        h - 2.0,
    )
}

fn draw(c: &Cal, area: &gtk::DrawingArea, cr: &gtk::cairo::Context, w: i32, h: i32) {
    let fg = area.color();
    let (r, g, b) = (fg.red() as f64, fg.green() as f64, fg.blue() as f64);
    let colw = col_width(c, w + GUTTER);
    let n = c.days.borrow().len();
    // Today's column.
    let today_col = c.days.borrow().iter().position(|&d| d == today());
    if let Some(today_col) = today_col {
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
    for d in 0..n {
        let x = (colw * d as f64).floor() + 0.5;
        cr.move_to(x, 0.0);
        cr.line_to(x, h as f64);
        let _ = cr.stroke();
    }
    // Now.
    if let Some(today_col) = today_col {
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

/// Draws the days the mode shows around the anchor; returns the end of the last.
pub(super) fn render(c: &Rc<Cal>) -> i64 {
    let days = shown(c);
    *c.days.borrow_mut() = days.clone();
    let (start, last) = (days[0], days[days.len() - 1]);
    let end = last + DAY;
    let title = if start == last {
        capitalized(&fmt(start, "%A %-d %B %Y"))
    } else if fmt(start, "%m") == fmt(last, "%m") {
        format!("{} – {}", fmt(start, "%-d"), fmt(last, "%-d %OB %Y"))
    } else {
        format!("{} – {}", fmt(start, "%-d %b"), fmt(last, "%-d %b %Y"))
    };
    c.title.set_title(&title);
    c.title.set_subtitle("");
    strip(c, mode(c) == Mode::Day);

    // Day names (but for one day: its week above has them), then the all-day row, both after a
    // gutter as wide as the hour labels.
    clear(&c.week_head);
    clear(&c.week_all_day);
    c.week_head.set_visible(days.len() > 1);
    let gutter = || gtk::Box::builder().width_request(GUTTER).build();
    c.week_head.append(&gutter());
    c.week_all_day.append(&gutter());
    let head = gtk::Box::builder().homogeneous(true).hexpand(true).build();
    c.week_head.append(&head);
    let items = items(start, end);
    for &day in &days {
        let name = gtk::Label::new(Some(&fmt(day, "%a %-d")));
        if day == today() {
            name.add_css_class("today");
        }
        flash_if(c, &name, day);
        head.append(&name);
    }
    // What has no hours: all-day items, a bar across their days.
    let all_day: Vec<&Item> = items
        .iter()
        .filter(|i| calendar::segments(core(), i, start).is_empty())
        .collect();
    let row = days::row(c, &days, &all_day, usize::MAX, true, |day| {
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cell.add_css_class("all-day-cell");
        day_drop(&cell, day, true);
        flash_if(c, &cell, day);
        cell
    });
    row.set_hexpand(true);
    c.week_all_day.append(&row);

    // The hours.
    for (w, _, _) in c.blocks.borrow_mut().drain(..) {
        c.week_grid.remove_overlay(&w);
    }
    let mut placed = Vec::new();
    for (col, &day) in days.iter().enumerate() {
        // Segments count days from the first shown; a work week may skip some.
        let d = (day - start) / DAY;
        let segs: Vec<(usize, i64, i64, i64)> = items
            .iter()
            .enumerate()
            .flat_map(|(n, i)| {
                calendar::segments(core(), i, start)
                    .into_iter()
                    .filter(move |s| s.0 == d)
                    .map(move |(_, a, b)| (n, a, b, col as i64))
            })
            .collect();
        let laid = calendar::columns(
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
        c.blocks.borrow_mut().push((w, geo, items[n].clone()));
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

/// The Day view's week above the hours, `shown` or emptied: its row of the month grid, the day
/// selected; clicking another day goes there.
fn strip(c: &Rc<Cal>, shown: bool) {
    clear(&c.week_strip);
    c.week_strip.set_visible(shown);
    if !shown {
        return;
    }
    let start = week_start(c.anchor.get());
    let week: Vec<i64> = (0..7).map(|d| start + d * DAY).collect();
    let names = gtk::Box::builder().homogeneous(true).build();
    names.add_css_class("weekdays");
    for &day in &week {
        names.append(&gtk::Label::new(Some(&fmt(day, "%a"))));
    }
    c.week_strip.append(&names);
    let all = items(start, start + 7 * DAY);
    let here: Vec<&Item> = all.iter().collect();
    let row = days::row(c, &week, &here, CHIPS, false, |day| {
        day_cell(c, day, false, |day| go(|_| day))
    });
    c.week_strip.append(&row);
}

/// An event (opens its details) or a planned task (tick, open, drag elsewhere) in the hours.
fn block(c: &Rc<Cal>, i: &Item, geo: Geo) -> gtk::Widget {
    // The title, then its time dimmer: below it, or at the end of a short block's one line.
    let label = |summary: &str, from: i64| -> gtk::Widget {
        let time = fmt(from, "%H:%M");
        if geo.to - geo.from < 45 {
            return super::one_line(summary, Some(time));
        }
        let text = format!(
            "{}\n<span alpha=\"55%\">{time}</span>",
            glib::markup_escape_text(summary)
        );
        gtk::Label::builder()
            .label(&text)
            .use_markup(true)
            .xalign(0.0)
            .yalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .vexpand(true)
            .build()
            .upcast()
    };
    match i {
        Item::Event(e) => {
            let b = gtk::Button::builder()
                .child(&label(&e.summary, e.start))
                .has_frame(false)
                .build();
            b.add_css_class("block");
            b.add_css_class("chip");
            b.add_css_class(&class(&e.list_href));
            b.set_tooltip_markup(Some(&tooltip(c, i)));
            if e.cancelled {
                b.add_css_class("cancelled");
            }
            drag_event(c, &b, e);
            event_clicks(&b, e);
            b.upcast()
        }
        Item::Task(t) => {
            let b = task_chip(c, t, &label(&t.summary, when(t).0), gtk::Align::Start);
            b.add_css_class("block");
            b.upcast()
        }
    }
}
