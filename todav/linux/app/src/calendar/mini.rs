//! The sidebar's month. Browsing it moves nothing else; clicking a day goes there. The day the
//! views are around is filled (in its own month only), today has a ring. Gtk.Calendar can't do
//! this: it always keeps a day selected, carries it to every month shown, and turns months on
//! the scroll wheel.

use super::*;

pub(super) struct Mini {
    title: gtk::Label,
    grid: gtk::Grid,
    /// Midnight of the first day of the month shown.
    month: Cell<i64>,
    /// The anchor last brought into view: a new one shows its month again.
    followed: Cell<i64>,
}

impl Mini {
    /// Fill `parent` with a header (the month between arrows), the weekdays and the days.
    pub(super) fn new(parent: &gtk::Box) -> Mini {
        let arrow = |icon: &str, tip: String| {
            let b = gtk::Button::builder()
                .icon_name(icon)
                .tooltip_text(tip)
                .build();
            b.add_css_class("flat");
            b
        };
        let prev = arrow("go-previous-symbolic", gettext("Previous Month"));
        let next = arrow("go-next-symbolic", gettext("Next Month"));
        prev.connect_clicked(|_| turn(-1));
        next.connect_clicked(|_| turn(1));
        let title = gtk::Label::builder().hexpand(true).build();
        title.add_css_class("heading");
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        head.append(&prev);
        head.append(&title);
        head.append(&next);
        let grid = gtk::Grid::builder().column_homogeneous(true).build();
        parent.append(&head);
        parent.append(&grid);
        Mini {
            title,
            grid,
            month: Cell::new(month_first(today())),
            followed: Cell::new(i64::MIN),
        }
    }
}

/// Show the month `by` away, the views staying where they are.
fn turn(by: i32) {
    let Some(c) = cal() else { return };
    if let Ok(m) = dates::fields(c.mini.month.get()).add_months(by) {
        c.mini.month.set(dates::midnight(&m));
        render(&c);
    }
}

/// Redraw; when the views moved to another day, show its month.
pub(super) fn refresh(c: &Rc<Cal>) {
    let anchor = c.anchor.get();
    if c.mini.followed.replace(anchor) != anchor {
        c.mini.month.set(month_first(anchor));
    }
    render(c);
}

fn render(c: &Rc<Cal>) {
    let m = &c.mini;
    let first = m.month.get();
    m.title.set_label(&capitalized(&fmt(first, "%OB %Y")));
    while let Some(w) = m.grid.first_child() {
        m.grid.remove(&w);
    }
    let start = week_start(first);
    for d in 0..7 {
        let name = gtk::Label::new(Some(&fmt(start + d * DAY, "%a")));
        name.add_css_class("weekday");
        m.grid.attach(&name, d as i32, 0, 1, 1);
    }
    let (today, anchor, month) = (today(), c.anchor.get(), dates::fields(first).month());
    // The Week view's week: a band under its row (if shown in this month).
    if mode(c) == Mode::Week && c.query.borrow().is_empty() {
        let row = (week_start(anchor) - start).div_euclid(7 * DAY);
        if (0..6).contains(&row) {
            let band = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            band.add_css_class("week-band");
            m.grid.attach(&band, 0, row as i32 + 1, 7, 1);
        }
    }
    let all = if prefs::mini_dots(core()) {
        items(start, start + 42 * DAY)
    } else {
        Vec::new()
    };
    for n in 0..42 {
        let day = start + n * DAY;
        let inside = dates::fields(day).month() == month;
        let selected = day == anchor && inside;
        // Under the number, a dot for each calendar with something that day (three at most).
        let dots = gtk::Box::builder()
            .spacing(2)
            .halign(gtk::Align::Center)
            .build();
        for list in day_lists(&all, day) {
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.add_css_class("dot");
            // On the selected day's fill they take its text colour (see the CSS).
            if !selected {
                dot.add_css_class(&class(list));
            }
            dots.append(&dot);
        }
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .build();
        content.append(&gtk::Label::new(Some(&fmt(day, "%-d"))));
        content.append(&dots);
        let b = gtk::Button::builder()
            .child(&content)
            .halign(gtk::Align::Center)
            .build();
        b.add_css_class("flat");
        b.add_css_class("circular");
        b.add_css_class("day");
        if !inside {
            b.add_css_class("other");
        }
        if day == today {
            b.add_css_class("today");
        }
        if selected {
            b.add_css_class("selected");
        }
        b.connect_clicked(move |_| {
            if let Some(c) = cal() {
                c.split.set_show_content(true); // narrow window: from the sidebar to the views
            }
            go(|_| day);
        });
        m.grid.attach(&b, (n % 7) as i32, (n / 7) as i32 + 1, 1, 1);
    }
}
