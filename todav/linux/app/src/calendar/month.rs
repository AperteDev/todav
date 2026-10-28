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

/// Select a day of the month grid. In the month shown only the highlight moves: a rebuilt
/// grid would lose the second click of a double click.
fn select(day: i64) {
    let Some(c) = cal() else { return };
    if month_first(day) != month_first(c.anchor.get()) {
        return go(|_| day);
    }
    c.anchor.set(day);
    c.browsed.set(false);
    let mut d = c.month_start.get();
    let mut row = c.month.first_child();
    while let Some(r) = row {
        for w in days::cells(&r) {
            if d == day {
                w.add_css_class("selected");
            } else {
                w.remove_css_class("selected");
            }
            d += DAY;
        }
        row = r.next_sibling();
    }
    mini::refresh(&c);
}
