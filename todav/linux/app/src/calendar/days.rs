//! A week as one row: the day cells, with the items on top. An item keeps its lane (row) for the
//! whole week, so one lasting several days is a single bar across them, broken only at the
//! week's ends or where a day has no room left.

use super::*;

/// Height of an item's lane: a chip (22 px, see `CSS`) and a gap.
/// ponytail: fixed; measure a chip if a large text scale makes them taller.
const LANE: i32 = 24;

/// The `week`'s days (up to seven midnights, in order: a week or some of its days): a `cell` per
/// day (its background: number, clicks, drops) and `items` on top, in at most `max` lanes.
/// Dropped on an item, something goes to that item's day, as `day_drop` with `all_day`.
pub(super) fn row(
    c: &Rc<Cal>,
    week: &[i64],
    items: &[&Item],
    max: usize,
    all_day: bool,
    cell: impl Fn(i64) -> gtk::Box,
) -> gtk::Overlay {
    let n = week.len();
    let days: Vec<Vec<usize>> = items
        .iter()
        .map(|i| (0..n).filter(|&d| on_day(i, week[d])).collect())
        .collect();
    let (pieces, more) = calendar::lanes(&days, max);
    let lanes = pieces
        .iter()
        .map(|p| p.lane + 1)
        .chain(more.iter().filter(|m| !m.is_empty()).map(|_| max))
        .max()
        .unwrap_or(0);

    // Each cell ends with room for the lanes; the first one's tells where they start.
    let base = gtk::Box::builder().homogeneous(true).build();
    let mut top = None;
    for &day in week {
        let cell = cell(day);
        let room = gtk::Box::builder()
            .height_request(lanes as i32 * LANE)
            .build();
        cell.append(&room);
        top.get_or_insert(room);
        base.append(&cell);
    }
    let top = top.unwrap();
    let overlay = gtk::Overlay::builder().child(&base).build();

    // (widget, lane, first and last column, whether it goes on left and right)
    let mut placed: Vec<(gtk::Widget, usize, usize, usize, bool, bool)> = Vec::new();
    for p in &pieces {
        let i = items[p.item];
        // On the day before or after, shown or not.
        let (left, right) = (on_day(i, week[p.from] - DAY), on_day(i, week[p.to] + DAY));
        let w = chip(c, i);
        if left {
            w.add_css_class("from-before");
        }
        if right {
            w.add_css_class("goes-on");
        }
        overlay.add_overlay(&w);
        placed.push((w, p.lane, p.from, p.to, left, right));
    }
    for (d, hidden) in more.iter().enumerate().filter(|(_, m)| !m.is_empty()) {
        let rest = hidden.iter().map(|&n| items[n].clone()).collect();
        let w = more_button(c, week[d], rest).upcast::<gtk::Widget>();
        overlay.add_overlay(&w);
        placed.push((w, max - 1, d, d, false, false));
    }
    overlay.connect_get_child_position(move |o, w| {
        let &(_, lane, from, to, left, right) = placed.iter().find(|p| &p.0 == w)?;
        let col = o.width() as f64 / n as f64;
        let y = top.compute_bounds(o)?.y() as i32 + lane as i32 * LANE;
        // Ends that go on reach the cell's edge, to meet the next piece.
        let x0 = (from as f64 * col) as i32 + if left { 0 } else { 3 };
        let x1 = ((to + 1) as f64 * col) as i32 - if right { 0 } else { 3 };
        Some(gdk::Rectangle::new(x0, y, (x1 - x0).max(1), LANE - 2))
    });

    // Dropped on an item rather than a cell: the day under the pointer.
    let column = Rc::new(Cell::new(0));
    let (col, week) = (column.clone(), week.to_vec());
    let drop = item_drop(
        &overlay,
        |_| true,
        move |i| drop_on_day(i, week[col.get()], all_day),
    );
    drop.connect_motion(move |t, x, _| {
        let width = t.widget().map_or(1, |w| w.width().max(1));
        column.set(((x * n as f64 / width as f64) as usize).min(n - 1));
        gdk::DragAction::MOVE
    });
    overlay
}

/// The day cells of a row made by `row`.
pub(super) fn cells(row: &gtk::Widget) -> Vec<gtk::Widget> {
    let base = row.downcast_ref::<gtk::Overlay>().and_then(|o| o.child());
    let mut out = Vec::new();
    let mut cell = base.and_then(|b| b.first_child());
    while let Some(w) = cell {
        cell = w.next_sibling();
        out.push(w);
    }
    out
}
