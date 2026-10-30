//! The edit dialogs' reminders: one row per reminder with a preset or a custom time span.
//! Offsets count from a task's due time or start, or an event's start.

use crate::dates;
use crate::i18n::{gettext, pgettext};
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
    /// Counts from a start (an event's, a task's), not a due time.
    from_start: bool,
}

/// Presets for a date or a time, as `dates::reminders`, worded for a start if `from_start`.
fn presets(all_day: bool, from_start: bool) -> Vec<(String, i64)> {
    let mut p = dates::reminders(all_day);
    if from_start && !all_day {
        p[0].0 = gettext("At start");
    }
    p
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
        let presets = presets(all_day, self.from_start);
        let custom_label = gettext("Custom…");
        let labels: Vec<&str> = presets
            .iter()
            .map(|(l, _)| l.as_str())
            .chain([custom_label.as_str()])
            .collect();
        // A date alone counts from the start of its day. They follow "<amount> <unit>".
        let dirs = if all_day {
            [
                pgettext("reminder", "before the day"),
                pgettext("reminder", "into the day"),
            ]
        } else {
            [
                pgettext("reminder", "before"),
                pgettext("reminder", "after"),
            ]
        };
        let dirs: Vec<&str> = dirs.iter().map(String::as_str).collect();
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
    /// Each reminder's rows, then the "Add Reminder" button.
    list: gtk::ListBox,
    add: adw::ButtonRow,
    rows: RefCell<Vec<Rc<Row>>>,
    all_day: Cell<bool>,
    from_start: bool,
}

impl Reminders {
    /// Show `offsets` in the dialog `b`; its "Add Reminder" adds one more.
    pub fn new(b: &gtk::Builder, all_day: bool, offsets: &[i64]) -> Rc<Reminders> {
        Reminders::with(b, "", all_day, offsets, false)
    }

    /// The same for an event's reminders, which count from its start.
    pub fn for_event(b: &gtk::Builder, all_day: bool, offsets: &[i64]) -> Rc<Reminders> {
        Reminders::with(b, "", all_day, offsets, true)
    }

    /// The same for a task's start, in `start_reminders` and `add_start_reminder`.
    pub fn for_start(b: &gtk::Builder, all_day: bool, offsets: &[i64]) -> Rc<Reminders> {
        Reminders::with(b, "start_", all_day, offsets, true)
    }

    fn with(
        b: &gtk::Builder,
        prefix: &str,
        all_day: bool,
        offsets: &[i64],
        from_start: bool,
    ) -> Rc<Reminders> {
        let r = Rc::new(Reminders {
            list: get(b, &format!("{prefix}reminders")),
            add: get(b, &format!("add_{prefix}reminder")),
            rows: RefCell::default(),
            all_day: Cell::new(all_day),
            from_start,
        });
        for o in offsets {
            r.add(*o);
        }
        let weak = Rc::downgrade(&r);
        r.add.connect_activated(move |_| {
            // The default, else the first preset not in use yet, so it is not a duplicate.
            if let Some(r) = weak.upgrade() {
                let all_day = r.all_day.get();
                let (used, first) = (
                    r.offsets(),
                    todav_app::prefs::default_reminder(crate::core(), all_day),
                );
                let presets = dates::reminders(all_day).into_iter().map(|(_, o)| o);
                let free = std::iter::once(first)
                    .chain(presets)
                    .find(|o| !used.contains(o));
                r.add(free.unwrap_or(first));
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
            from_start: self.from_start,
        });
        crate::window::scroll_past(row.amount.upcast_ref());
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
            r.list.remove(&row.choice);
            r.list.remove(&row.custom);
            r.rows.borrow_mut().retain(|x| !Rc::ptr_eq(x, &row));
        });
        let at = self.add.index();
        self.list.insert(&row.choice, at);
        self.list.insert(&row.custom, at + 1);
        self.rows.borrow_mut().push(row);
    }

    /// The due date switched between a date and a time: custom reminders keep their span, a
    /// preset that does not fit the new kind becomes the default one.
    pub fn set_all_day(&self, all_day: bool) {
        self.all_day.set(all_day);
        let presets = presets(all_day, self.from_start);
        for row in self.rows.borrow().iter() {
            let (o, custom) = (row.offset(), row.is_custom());
            let fits = presets.iter().any(|(_, p)| *p == o);
            row.fill(
                all_day,
                if custom || fits {
                    o
                } else {
                    todav_app::prefs::default_reminder(crate::core(), all_day)
                },
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
