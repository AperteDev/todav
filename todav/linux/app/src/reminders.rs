//! The edit dialog's reminders: one row per reminder with a preset or a custom time span.

use crate::dates;
use crate::window::{get, view};
use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// One reminder: its choice row and, for "Custom…", the time span row below it.
struct Row {
    choice: adw::ComboRow,
    choices: gtk::StringList,
    custom: gtk::ListBoxRow,
    amount: gtk::SpinButton,
    unit: gtk::DropDown,
    dir: gtk::DropDown,
    dirs: gtk::StringList,
    /// Offsets of the presets, in choice order; "Custom…" follows them.
    presets: RefCell<Vec<i64>>,
}

impl Row {
    fn is_custom(&self) -> bool {
        self.choice.selected() as usize >= self.presets.borrow().len()
    }

    /// Seconds after the due time.
    fn offset(&self) -> i64 {
        match self.presets.borrow().get(self.choice.selected() as usize) {
            Some(o) => *o,
            None => dates::offset(
                self.amount.value() as i64,
                self.unit.selected(),
                self.dir.selected() == 0,
            ),
        }
    }

    /// Choices for the due kind, showing `offset`: its preset, else (or with `custom`) the
    /// time span.
    fn fill(&self, all_day: bool, offset: i64, custom: bool) {
        let presets = dates::reminders(all_day);
        let labels: Vec<&str> = presets.iter().map(|(l, _)| *l).chain(["Custom…"]).collect();
        // A date alone counts from the start of its day.
        let dirs = if all_day {
            ["before the day", "into the day"]
        } else {
            ["before", "after"]
        };
        *self.presets.borrow_mut() = presets.iter().map(|(_, o)| *o).collect();
        self.choices.splice(0, self.choices.n_items(), &labels);
        self.dirs.splice(0, self.dirs.n_items(), &dirs);
        let preset = presets
            .iter()
            .position(|(_, o)| *o == offset)
            .filter(|_| !custom);
        let (a, u, before) = dates::span(offset);
        self.amount.set_value(a as f64);
        self.unit.set_selected(u);
        self.dir.set_selected(if before { 0 } else { 1 });
        self.choice
            .set_selected(preset.unwrap_or(presets.len()) as u32);
        self.custom.set_visible(self.is_custom());
    }
}

pub struct Reminders {
    due: adw::ExpanderRow,
    rows: RefCell<Vec<Rc<Row>>>,
    all_day: Cell<bool>,
}

impl Reminders {
    /// Show `offsets` in the dialog `b`; its "+" adds one more.
    pub fn new(b: &gtk::Builder, all_day: bool, offsets: &[i64]) -> Rc<Reminders> {
        let r = Rc::new(Reminders {
            due: get(b, "due"),
            rows: RefCell::default(),
            all_day: Cell::new(all_day),
        });
        for o in offsets {
            r.add(*o);
        }
        let add: gtk::Button = get(b, "add_reminder");
        let weak = Rc::downgrade(&r);
        add.connect_clicked(move |_| {
            // The first preset not in use yet, so a second reminder is not a duplicate.
            if let Some(r) = weak.upgrade() {
                let (used, presets) = (r.offsets(), dates::reminders(r.all_day.get()));
                let free = presets.iter().find(|(_, o)| !used.contains(o));
                r.add(free.unwrap_or(&presets[0]).1);
            }
        });
        r
    }

    fn add(self: &Rc<Self>, offset: i64) {
        let b = view!("reminder");
        let row = Rc::new(Row {
            choice: get(&b, "choice"),
            choices: get(&b, "choices"),
            custom: get(&b, "custom"),
            amount: get(&b, "amount"),
            unit: get(&b, "unit"),
            dir: get(&b, "dir"),
            dirs: get(&b, "dirs"),
            presets: RefCell::default(),
        });
        row.fill(self.all_day.get(), offset, false);
        let weak = Rc::downgrade(&row);
        row.choice.connect_selected_notify(move |_| {
            if let Some(row) = weak.upgrade() {
                row.custom.set_visible(row.is_custom());
            }
        });
        let remove: gtk::Button = get(&b, "remove");
        let (weak, gone) = (Rc::downgrade(self), Rc::downgrade(&row));
        remove.connect_clicked(move |_| {
            let (Some(r), Some(row)) = (weak.upgrade(), gone.upgrade()) else {
                return;
            };
            r.due.remove(&row.choice);
            r.due.remove(&row.custom);
            r.rows.borrow_mut().retain(|x| !Rc::ptr_eq(x, &row));
        });
        self.due.add_row(&row.choice);
        self.due.add_row(&row.custom);
        self.rows.borrow_mut().push(row);
    }

    /// The due date switched between a date and a time: custom reminders keep their span, a
    /// preset that does not fit the new kind becomes its first one.
    pub fn set_all_day(&self, all_day: bool) {
        self.all_day.set(all_day);
        let presets = dates::reminders(all_day);
        for row in self.rows.borrow().iter() {
            let (o, custom) = (row.offset(), row.is_custom());
            let fits = presets.iter().any(|(_, p)| *p == o);
            row.fill(
                all_day,
                if custom || fits { o } else { presets[0].1 },
                custom,
            );
        }
    }

    /// Seconds after the due time, soonest first, without duplicates.
    pub fn offsets(&self) -> Vec<i64> {
        let mut o: Vec<i64> = self.rows.borrow().iter().map(|r| r.offset()).collect();
        o.sort();
        o.dedup();
        o
    }
}
