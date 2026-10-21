//! A week as one row: the day cells, with the items on top. An item keeps its lane (row) for the
//! whole week, so one lasting several days is a single bar across them, broken only at the
//! week's ends or where a day has no room left.

use super::*;

/// Height of an item's lane: a chip (22 px, see `CSS`) and a gap.
/// ponytail: fixed; measure a chip if a large text scale makes them taller.
const LANE: i32 = 24;

/// One piece of an item on the row: its lane and the days (columns 0–6) it covers.
#[derive(Debug, PartialEq)]
struct Piece {
    item: usize,
    lane: usize,
    from: usize,
    to: usize,
}

/// Lanes for items covering the days `days[i]` (ascending, contiguous): the earliest first,
/// then the longest, each in the lowest lane free on all its days. At most `max` lanes show: a
/// day with an item past them keeps its last lane for "+N more", listing what it hides there.
fn layout(days: &[Vec<usize>], max: usize) -> (Vec<Piece>, [Vec<usize>; 7]) {
    let mut order: Vec<usize> = (0..days.len()).filter(|&i| !days[i].is_empty()).collect();
    order.sort_by_key(|&i| (days[i][0], std::cmp::Reverse(days[i].len()), i));
    let mut taken: Vec<[bool; 7]> = Vec::new();
    let mut lane = vec![0; days.len()];
    for &i in &order {
        let free = |t: &[bool; 7]| days[i].iter().all(|&d| !t[d]);
        let l = taken.iter().position(free).unwrap_or(taken.len());
        if l == taken.len() {
            taken.push([false; 7]);
        }
        for &d in &days[i] {
            taken[l][d] = true;
        }
        lane[i] = l;
    }
    let limit: Vec<usize> = (0..7)
        .map(|d| {
            let over = order
                .iter()
                .any(|&i| days[i].contains(&d) && lane[i] >= max);
            if over { max - 1 } else { max }
        })
        .collect();
    let mut pieces = Vec::new();
    let mut more: [Vec<usize>; 7] = Default::default();
    for &i in &order {
        let mut run: Option<(usize, usize)> = None;
        for &d in &days[i] {
            if lane[i] < limit[d] {
                run = Some(run.map_or((d, d), |(from, _)| (from, d)));
            } else {
                more[d].push(i);
                if let Some((from, to)) = run.take() {
                    pieces.push(Piece {
                        item: i,
                        lane: lane[i],
                        from,
                        to,
                    });
                }
            }
        }
        if let Some((from, to)) = run {
            pieces.push(Piece {
                item: i,
                lane: lane[i],
                from,
                to,
            });
        }
    }
    for m in &mut more {
        m.sort(); // in the items' order
    }
    (pieces, more)
}

/// The week from `start`: a `cell` per day (its background: number, clicks, drops) and `items`
/// on top, in at most `max` lanes. Dropped on an item, something goes to that item's day, as
/// `day_drop` with `all_day`.
pub(super) fn row(
    c: &Rc<Cal>,
    start: i64,
    items: &[&Item],
    max: usize,
    all_day: bool,
    cell: impl Fn(i64) -> gtk::Box,
) -> gtk::Overlay {
    let days: Vec<Vec<usize>> = items
        .iter()
        .map(|i| {
            (0..7)
                .filter(|&d| on_day(i, start + d as i64 * DAY))
                .collect()
        })
        .collect();
    let (pieces, more) = layout(&days, max);
    let lanes = pieces
        .iter()
        .map(|p| p.lane + 1)
        .chain(more.iter().filter(|m| !m.is_empty()).map(|_| max))
        .max()
        .unwrap_or(0);

    // Each cell ends with room for the lanes; the first one's tells where they start.
    let base = gtk::Box::builder().homogeneous(true).build();
    let mut top = None;
    for d in 0..7 {
        let cell = cell(start + d * DAY);
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
        let goes_on = |d: i64| on_day(i, start + d * DAY);
        let (left, right) = (goes_on(p.from as i64 - 1), goes_on(p.to as i64 + 1));
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
        let w = more_button(c, start + d as i64 * DAY, rest).upcast::<gtk::Widget>();
        overlay.add_overlay(&w);
        placed.push((w, max - 1, d, d, false, false));
    }
    overlay.connect_get_child_position(move |o, w| {
        let &(_, lane, from, to, left, right) = placed.iter().find(|p| &p.0 == w)?;
        let col = o.width() as f64 / 7.0;
        let y = top.compute_bounds(o)?.y() as i32 + lane as i32 * LANE;
        // Ends that go on reach the cell's edge, to meet the next piece.
        let x0 = (from as f64 * col) as i32 + if left { 0 } else { 3 };
        let x1 = ((to + 1) as f64 * col) as i32 - if right { 0 } else { 3 };
        Some(gdk::Rectangle::new(x0, y, (x1 - x0).max(1), LANE - 2))
    });

    // Dropped on an item rather than a cell: the day under the pointer.
    let column = Rc::new(Cell::new(0));
    let col = column.clone();
    let drop = item_drop(&overlay, false, move |i| {
        drop_on_day(i, start + col.get() * DAY, all_day)
    });
    drop.connect_motion(move |t, x, _| {
        let width = t.widget().map_or(1, |w| w.width().max(1));
        column.set(((x * 7.0 / width as f64) as i64).clamp(0, 6));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(item: usize, lane: usize, from: usize, to: usize) -> Piece {
        Piece {
            item,
            lane,
            from,
            to,
        }
    }

    #[test]
    fn multi_day_items_keep_a_lane_and_overflow_goes_to_more() {
        // 0: Mon–Wed, 1: Tue, 2: Tue–Sun, 3: Wed, 4: Wed
        let days = vec![
            vec![0, 1, 2],
            vec![1],
            vec![1, 2, 3, 4, 5, 6],
            vec![2],
            vec![2],
        ];
        let (pieces, more) = layout(&days, 3);
        assert_eq!(
            pieces,
            vec![
                piece(0, 0, 0, 2),
                // The longer one first on Tuesday, under the bar from Monday.
                piece(2, 1, 1, 6),
                piece(1, 2, 1, 1),
            ]
        );
        // Wednesday has four: its third lane shows "+2 more" for the two singles.
        assert_eq!(more[2], vec![3, 4]);
        assert!(more.iter().enumerate().all(|(d, m)| d == 2 || m.is_empty()));

        // Without a limit everything gets a lane.
        let (pieces, more) = layout(&days, usize::MAX);
        assert_eq!(pieces.len(), 5);
        assert!(more.iter().all(Vec::is_empty));
    }
}
