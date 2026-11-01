//! The Month view: weeks scrolled up and down a row at a time, as GNOME Calendar's, six in
//! sight. The month shown is the one most of them are in; scrolling into another moves the
//! anchor there, moving the anchor to another month (the arrows, a day picked) scrolls to it.
//! The grid holds a few weeks either side of those in sight, built again around them once the
//! month changes.

use super::*;

/// Weeks in the grid, and how many come before the top one in sight.
const ROWS: i64 = 18;
const BEFORE: i64 = 6;

/// The month most of the six weeks from `top` are in.
fn shown(top: i64) -> i64 {
    month_first(top + 21 * DAY)
}

/// Height of a row: the grid's rows are all as tall.
fn row_height(c: &Cal) -> f64 {
    c.month_scroll.vadjustment().upper() / ROWS as f64
}

pub(super) fn setup(c: &Rc<Cal>) {
    let adj = c.month_scroll.vadjustment();
    // Laid out anew (the first time, a new size): six rows in sight (more when the window is
    // too short for them), the same week at the top. After the layout, which would undo both.
    let fit = |_: &gtk::Adjustment| {
        glib::idle_add_local_once(|| {
            let Some(c) = cal() else { return };
            let adj = c.month_scroll.vadjustment();
            c.month
                .set_height_request((adj.page_size() * ROWS as f64 / 6.0) as i32);
            if !animating(&c) {
                let row = (c.month_top.get() - c.month_start.get()) / (7 * DAY);
                adj.set_value(row as f64 * row_height(&c));
            }
        });
    };
    adj.connect_page_size_notify(fit);
    adj.connect_upper_notify(fit);
    // Once it stops (a touchpad's or a finger's scrolling, an animation): onto the nearest
    // row, then into the month in sight.
    adj.connect_value_changed(|_| {
        let Some(c) = cal() else { return };
        if let Some(id) = c.month_settle.take() {
            id.remove();
        }
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(150), || {
            if let Some(c) = cal() {
                c.month_settle.take();
                settled(&c);
            }
        });
        c.month_settle.replace(Some(id));
    });
    // A notch of the wheel scrolls a row; a touchpad scrolls as it goes.
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    wheel.connect_scroll(|w, _, dy| {
        let Some(c) = cal() else {
            return glib::Propagation::Proceed;
        };
        if w.unit() != gdk::ScrollUnit::Wheel {
            return glib::Propagation::Proceed;
        }
        // High-resolution wheels send parts of a notch.
        let notches = c.month_wheel.get() + dy;
        let rows = notches.trunc();
        c.month_wheel.set(notches - rows);
        if rows != 0.0 {
            let from = match c.month_anim.borrow().as_ref() {
                Some(a) if animating(&c) => a.value_to(),
                _ => c.month_scroll.vadjustment().value(),
            };
            scroll_to(&c, (from / row_height(&c)).round() + rows);
        }
        glib::Propagation::Stop
    });
    c.month_scroll.add_controller(wheel);
    drag_days(c);
}

fn animating(c: &Cal) -> bool {
    c.month_anim
        .borrow()
        .as_ref()
        .is_some_and(|a| a.state() == adw::AnimationState::Playing)
}

/// Scroll the grid smoothly to have row `row` at the top.
fn scroll_to(c: &Cal, row: f64) {
    let adj = c.month_scroll.vadjustment();
    let to = (row * row_height(c)).clamp(0.0, adj.upper() - adj.page_size());
    if let Some(a) = c.month_anim.take() {
        a.pause();
    }
    let target = adw::PropertyAnimationTarget::new(&adj, "value");
    let a = adw::TimedAnimation::new(&c.month_scroll, adj.value(), to, 250, target);
    a.play();
    c.month_anim.replace(Some(a));
}

fn settled(c: &Rc<Cal>) {
    let h = row_height(c);
    if animating(c) || h <= 0.0 {
        return;
    }
    let at = c.month_scroll.vadjustment().value() / h;
    let row = at.round();
    if (at - row).abs() * h > 0.5 {
        return scroll_to(c, row);
    }
    let top = c.month_start.get() + row as i64 * 7 * DAY;
    c.month_top.set(top);
    let month = shown(top);
    if month != month_first(c.anchor.get()) {
        // Today when it is in that month, else its first day.
        let today = today();
        c.anchor.set(if month_first(today) == month {
            today
        } else {
            month
        });
        c.browsed.set(true);
        refresh();
    }
}

/// Fill the grid around the weeks in sight, or those of the anchor's month when it is another
/// (scrolled to when near). Returns the end of the weeks in sight.
pub(super) fn render(c: &Rc<Cal>) -> i64 {
    let first = month_first(c.anchor.get());
    let old = c.month_top.get();
    let top = if shown(old) == first {
        old
    } else {
        week_start(first)
    };
    c.month_top.set(top);
    let start = top - BEFORE * 7 * DAY;
    let moved = start != c.month_start.replace(start);
    c.title.set_title(&capitalized(&fmt(first, "%OB %Y")));
    c.title.set_subtitle("");

    clear(&c.weekdays);
    for i in 0..7 {
        let name = gtk::Label::new(Some(&fmt(start + i * DAY, "%a")));
        weekend(&name, start + i * DAY);
        c.weekdays.append(&name);
    }
    while let Some(child) = c.month.first_child() {
        c.month.remove(&child);
    }
    let end = start + ROWS * 7 * DAY;
    let all = items(start, end);
    let month = dates::fields(first).month();
    for w in 0..ROWS {
        let from = start + w * 7 * DAY;
        let week: Vec<i64> = (0..7).map(|d| from + d * DAY).collect();
        let here: Vec<&Item> = all
            .iter()
            .filter(|i| week.iter().any(|&d| on_day(i, d)))
            .collect();
        let row = days::row(c, &week, &here, CHIPS, false, |day| {
            day_cell(c, day, dates::fields(day).month() != month, select)
        });
        c.month.attach(&row, 0, w as i32, 1, 1);
    }
    if moved {
        // The weeks that were at the top, if still in the grid, scroll to the new ones.
        let h = row_height(c);
        let adj = c.month_scroll.vadjustment();
        let was = (old - start) / (7 * DAY);
        if let Some(a) = c.month_anim.take() {
            a.pause();
        }
        if old != top && (0..=ROWS - 6).contains(&was) {
            adj.set_value(was as f64 * h);
            scroll_to(c, BEFORE as f64);
        } else {
            adj.set_value(BEFORE as f64 * h);
        }
    }
    top + 42 * DAY
}

/// Select a day of the month grid, then offer a new event on it, as GNOME Calendar does: after
/// the double-click time, as a double click opens the whole dialog instead. In the month shown
/// only the highlight moves: a rebuilt grid would lose the second click of a double click. A
/// day of another month goes there.
fn select(day: i64) {
    let Some(c) = cal() else { return };
    if month_first(day) != month_first(c.anchor.get()) {
        return go(|_| day);
    }
    c.anchor.set(day);
    c.browsed.set(false);
    mark(&c, "selected", |d| d == day);
    mini::refresh(&c);
    let wait = gtk::Settings::default().map_or(400, |s| s.gtk_double_click_time());
    let id = glib::timeout_add_local_once(
        std::time::Duration::from_millis(wait.max(0) as u64),
        move || {
            if let Some(c) = cal() {
                c.quick.take();
                quick_add(&c, day, day);
            }
        },
    );
    if let Some(old) = c.quick.replace(Some(id)) {
        old.remove();
    }
}

/// The grid's days, each with its cell.
fn cells(c: &Cal) -> Vec<(i64, gtk::Widget)> {
    let mut out = Vec::new();
    let mut d = c.month_start.get();
    let mut row = c.month.first_child();
    while let Some(r) = row {
        for w in days::cells(&r) {
            out.push((d, w));
            d += DAY;
        }
        row = r.next_sibling();
    }
    out
}

/// Give the grid's days for which `on` holds the class `cls`, and take it from the others.
fn mark(c: &Cal, cls: &str, on: impl Fn(i64) -> bool) {
    for (d, w) in cells(c) {
        if on(d) {
            w.add_css_class(cls);
        } else {
            w.remove_css_class(cls);
        }
    }
}

/// The day under `(x, y)` in the grid.
fn day_at(c: &Cal, x: f64, y: f64) -> Option<i64> {
    let p = gtk::graphene::Point::new(x as f32, y as f32);
    cells(c)
        .into_iter()
        .find(|(_, w)| {
            w.compute_bounds(&c.month)
                .is_some_and(|b| b.contains_point(&p))
        })
        .map(|(d, _)| d)
}

/// Days dragged across, with the mouse, from an empty spot of a day (not an item, which is
/// dragged or clicked itself): marked as it goes, then a new event over them is offered.
fn drag_days(c: &Rc<Cal>) {
    let drag = gtk::GestureDrag::new();
    // The first and the last day so far, once past the first.
    let span: Rc<Cell<Option<(i64, i64)>>> = Rc::default();
    let start: Rc<Cell<Option<i64>>> = Rc::default();
    let (s, sp) = (start.clone(), span.clone());
    drag.connect_drag_begin(move |g, x, y| {
        sp.set(None);
        s.set(None);
        let Some(c) = cal() else { return };
        let touch = g
            .device()
            .is_some_and(|d| d.source() == gdk::InputSource::Touchscreen);
        let on_item = c
            .month
            .pick(x, y, gtk::PickFlags::DEFAULT)
            .is_some_and(|w| w.ancestor(gtk::Button::static_type()).is_some());
        match day_at(&c, x, y) {
            // A finger scrolls the grid instead.
            Some(d) if !touch && !on_item => s.set(Some(d)),
            _ => {
                g.set_state(gtk::EventSequenceState::Denied);
            }
        }
    });
    let sp = span.clone();
    drag.connect_drag_update(move |g, dx, dy| {
        let (Some(c), Some(from), Some((x, y))) = (cal(), start.get(), g.start_point()) else {
            return;
        };
        let Some(at) = day_at(&c, x + dx, y + dy) else {
            return;
        };
        let last = sp.get().map_or(from, |(_, to)| to);
        if at == last {
            return;
        }
        // Past the first day: no longer a click (the cell's, which selects it).
        g.set_state(gtk::EventSequenceState::Claimed);
        sp.set(Some((from, at)));
        let (a, b) = (from.min(at), from.max(at));
        mark(&c, "range", |d| (a..=b).contains(&d));
    });
    drag.connect_drag_end(move |_, _, _| {
        if let (Some(c), Some((from, to))) = (cal(), span.take()) {
            quick_add(&c, from.min(to), from.max(to));
        }
    });
    c.month.add_controller(drag);
}

/// Offer a new all-day event from `from` to `to` (midnights of its first and last days), from
/// the cell of its last day, the days marked meanwhile.
fn quick_add(c: &Rc<Cal>, from: i64, to: i64) {
    let Some((_, cell)) = cells(c).into_iter().find(|(d, _)| *d == to) else {
        return;
    };
    mark(c, "range", |d| (from..=to).contains(&d));
    let time = EventTime {
        start: from,
        end: to + DAY,
        all_day: true,
    };
    quick::open(c, &cell, time, || {
        if let Some(c) = cal() {
            mark(&c, "range", |_| false);
        }
    });
}
