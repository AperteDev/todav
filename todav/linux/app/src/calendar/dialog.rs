//! The event dialog: a new event or an existing one. Changing or deleting an occurrence of a
//! repeating event asks which occurrences it is for.

use super::*;
use todav_core::rrule::Freq;
use todav_core::{EventPatch, EventTime, Repeat, Scope};

const FREQS: [Freq; 4] = [Freq::Daily, Freq::Weekly, Freq::Monthly, Freq::Yearly];

/// The unit of "Every n …" for repeat choice `freq` (1 daily … 4 yearly), in `n`'s form.
fn unit(freq: u32, n: u64) -> String {
    match freq {
        1 => ngettext("day", "days", n),
        2 => ngettext("week", "weeks", n),
        3 => ngettext("month", "months", n),
        _ => ngettext("year", "years", n),
    }
}

/// How an existing rule shows in the dialog: as one of its plain choices (1 daily … 4 yearly,
/// every n, until), or as "Custom" (kept unless the user picks another).
fn plain(rule: &str) -> Option<(usize, i64, Option<i64>)> {
    let r = todav_app::calendar::plain_repeat(rule)?;
    let freq = FREQS.iter().position(|f| *f == r.freq)? + 1;
    Some((freq, r.interval, r.until))
}

/// Ask which occurrences of a repeating event a change (or delete) is for.
pub(super) fn ask_scope(
    parent: &impl IsA<gtk::Widget>,
    heading: &str,
    delete: bool,
    then: impl Fn(Scope) + 'static,
) {
    let d = adw::AlertDialog::new(Some(heading), Some(&gettext("The event repeats.")));
    d.add_responses(&[
        ("cancel", &gettext("Cancel")),
        ("this", &gettext("Only This Event")),
        ("following", &gettext("This and Following")),
        ("all", &gettext("All Events")),
    ]);
    d.set_close_response("cancel");
    d.set_default_response(Some("this"));
    if delete {
        for r in ["this", "following", "all"] {
            d.set_response_appearance(r, adw::ResponseAppearance::Destructive);
        }
    }
    d.connect_response(None, move |_, r| match r {
        "this" => then(Scope::This),
        "following" => then(Scope::Following),
        "all" => then(Scope::All),
        _ => {}
    });
    d.present(Some(parent));
}

/// Open the dialog on an existing occurrence (`Some(e)`) or a new event at `time`; returns its
/// title field.
pub(super) fn open(c: &Rc<Cal>, e: Option<Event>, time: EventTime) -> adw::EntryRow {
    let b = view!("event-dialog");
    let dialog: adw::Dialog = get(&b, "dialog");
    let summary: adw::EntryRow = get(&b, "summary");
    let location: adw::EntryRow = get(&b, "location");
    let calendar: adw::ComboRow = get(&b, "calendar");
    let priority: adw::ComboRow = get(&b, "priority");
    let old_priority = crate::window::priority_row(e.as_ref().and_then(|e| e.priority));
    priority.set_selected(old_priority);
    let all_day: adw::SwitchRow = get(&b, "all_day");
    let (start_row, end_row): (adw::ExpanderRow, adw::ExpanderRow) =
        (get(&b, "start"), get(&b, "end"));
    let (start_cal, end_cal): (gtk::Calendar, gtk::Calendar) =
        (get(&b, "start_cal"), get(&b, "end_cal"));
    let spins: [gtk::SpinButton; 4] = [
        get(&b, "start_hour"),
        get(&b, "start_minute"),
        get(&b, "end_hour"),
        get(&b, "end_minute"),
    ];
    let (start_time, end_time): (adw::ActionRow, adw::ActionRow) =
        (get(&b, "start_time"), get(&b, "end_time"));
    let repeat: adw::ComboRow = get(&b, "repeat");
    let repeats: gtk::StringList = get(&b, "repeats");
    let interval: adw::SpinRow = get(&b, "interval");
    let until: adw::ExpanderRow = get(&b, "until");
    let until_cal: gtk::Calendar = get(&b, "until_cal");
    let description: gtk::TextView = get(&b, "description");
    dialog.set_title(&if e.is_some() {
        gettext("Edit Event")
    } else {
        gettext("New Event")
    });
    for s in &spins {
        crate::window::two_digits(s);
        crate::window::scroll_past(s.upcast_ref());
    }
    for c in [&start_cal, &end_cal, &until_cal] {
        crate::window::scroll_past(c.upcast_ref());
    }

    // Calendars we may write to, grouped by where they are kept; the event's own, or the one
    // used last.
    let writable = todav_app::lists::by_place(
        core(),
        c.lists
            .borrow()
            .iter()
            .filter(|l| l.holds_events && l.writable)
            .cloned()
            .collect(),
    );
    crate::lists::picker(&calendar, &writable, None);
    let pick = e
        .as_ref()
        .map(|e| e.list_href.clone())
        .or_else(|| core().setting("last_calendar".into()));
    calendar.set_selected(
        writable
            .iter()
            .position(|l| Some(&l.href) == pick.as_ref())
            .unwrap_or(0) as u32,
    );

    if let Some(e) = &e {
        summary.set_text(&e.summary);
        location.set_text(e.location.as_deref().unwrap_or_default());
        description
            .buffer()
            .set_text(e.description.as_deref().unwrap_or_default());
    }
    all_day.set_active(time.all_day);
    start_cal.select_day(&dates::fields(time.start));
    // An all-day event's last day is the one before its end.
    end_cal.select_day(&dates::fields(if time.all_day {
        time.end - DAY
    } else {
        time.end
    }));
    // An all-day event's times, should it get some: 9:00 for the default length.
    let (from, to) = if time.all_day {
        let at = 9 * 3600;
        (at, at + prefs::event_duration(core()))
    } else {
        (time.start, time.end)
    };
    for (spin, t, unit) in [
        (&spins[0], from, 3600),
        (&spins[1], from, 60),
        (&spins[2], to, 3600),
        (&spins[3], to, 60),
    ] {
        let v = if unit == 3600 {
            t.rem_euclid(DAY) / 3600
        } else {
            t.rem_euclid(3600) / 60
        };
        spin.set_value(v as f64);
    }
    let show_times = {
        let (s, e) = (start_time.clone(), end_time.clone());
        move |all_day: bool| {
            s.set_visible(!all_day);
            e.set_visible(!all_day);
        }
    };
    show_times(time.all_day);

    // Repeat: a plain rule, or "Custom" for one this dialog cannot show.
    let rule = e.as_ref().and_then(|e| e.rule.clone());
    let shown = rule.as_deref().map(plain);
    let custom = matches!(shown, Some(None));
    if custom {
        repeats.append(&pgettext("repeat", "Custom"));
    }
    let (freq, every, until_at) = match shown {
        Some(Some(p)) => p,
        Some(None) => (5, 1, None),
        None => (0, 1, None),
    };
    repeat.set_selected(freq as u32);
    interval.set_value(every as f64);
    until.set_enable_expansion(until_at.is_some());
    until_cal.select_day(&dates::fields(until_at.unwrap_or(time.start + 30 * DAY)));
    let show_repeat = {
        let (interval, until, repeat) = (interval.clone(), until.clone(), repeat.clone());
        move || {
            let i = repeat.selected();
            let plain = (1..=4).contains(&i);
            interval.set_visible(plain);
            until.set_visible(plain);
            if plain {
                interval.set_subtitle(&unit(i, interval.value() as u64));
            }
        }
    };
    show_repeat();
    // An occurrence of a series changes how the series repeats only as a whole.
    let s = show_repeat.clone();
    repeat.connect_selected_notify(move |_| s());
    interval.connect_value_notify(move |_| show_repeat());

    // Reminders count from the start.
    let offsets: Vec<i64> = e
        .as_ref()
        .map(|e| e.reminders.iter().map(|r| r - e.start).collect())
        .unwrap_or_default();
    let reminders = crate::reminders::Reminders::for_event(&b, time.all_day, &offsets);
    let r = reminders.clone();
    let st = show_times.clone();
    all_day.connect_active_notify(move |a| {
        st(a.is_active());
        r.set_all_day(a.is_active());
    });

    // Moving the start keeps the length.
    let read = {
        let (all_day, start_cal, end_cal, spins) = (
            all_day.clone(),
            start_cal.clone(),
            end_cal.clone(),
            spins.clone(),
        );
        move || {
            let day = |cal: &gtk::Calendar| dates::midnight(&cal.date());
            let hm = |h: &gtk::SpinButton, m: &gtk::SpinButton| {
                h.value() as i64 * 3600 + m.value() as i64 * 60
            };
            let all = all_day.is_active();
            let start = day(&start_cal) + if all { 0 } else { hm(&spins[0], &spins[1]) };
            let end = day(&end_cal) + if all { DAY } else { hm(&spins[2], &spins[3]) };
            let end = if end > start {
                end
            } else {
                start
                    + if all {
                        DAY
                    } else {
                        prefs::event_duration(core())
                    }
            };
            EventTime {
                start,
                end,
                all_day: all,
            }
        }
    };
    let len = Rc::new(Cell::new(time.end - time.start));
    let keep_length = {
        let (read, end_cal, spins, len, busy) = (
            read.clone(),
            end_cal.clone(),
            spins.clone(),
            len.clone(),
            Rc::new(Cell::new(false)),
        );
        move || {
            if busy.replace(true) {
                return;
            }
            let t = read();
            let end = t.start + len.get();
            end_cal.select_day(&dates::fields(if t.all_day { end - DAY } else { end }));
            spins[2].set_value((end.rem_euclid(DAY) / 3600) as f64);
            spins[3].set_value((end.rem_euclid(3600) / 60) as f64);
            busy.set(false);
        }
    };
    let k = keep_length.clone();
    start_cal.connect_day_selected(move |_| k());
    for s in &spins[..2] {
        let k = keep_length.clone();
        s.connect_value_changed(move |_| k());
    }
    // Changing the end changes the length kept.
    for s in &spins[2..] {
        let (read, len) = (read.clone(), len.clone());
        s.connect_value_changed(move |_| {
            let t = read();
            len.set(t.end - t.start);
        });
    }
    let (read2, len2) = (read.clone(), len.clone());
    end_cal.connect_day_selected(move |_| {
        let t = read2();
        len2.set(t.end - t.start);
    });
    // So does turning "All day" off or on: its days or times are what is kept then.
    let (read2, len2) = (read.clone(), len.clone());
    all_day.connect_active_notify(move |_| {
        let t = read2();
        len2.set(t.end - t.start);
    });

    // The rows say when it starts and ends, as the task dialog's dates; connected last, so they
    // show the end moved with the start.
    let show_dates = {
        let read = read.clone();
        let now = core().local_now();
        move || {
            let t = read();
            let last = if t.all_day { t.end - DAY } else { t.end };
            start_row.set_subtitle(&dates::label(t.start, t.all_day, now));
            end_row.set_subtitle(&dates::label(last, t.all_day, now));
        }
    };
    show_dates();
    for cal in [&start_cal, &end_cal] {
        // Day, month or year: moving the month changes the date too.
        let s = show_dates.clone();
        cal.connect_notify_local(None, move |_, _| s());
    }
    for spin in &spins {
        let s = show_dates.clone();
        spin.connect_value_changed(move |_| s());
    }
    all_day.connect_active_notify(move |_| show_dates());

    let d = dialog.downgrade();
    get::<gtk::Button>(&b, "cancel").connect_clicked(move |_| {
        if let Some(d) = d.upgrade() {
            d.close();
        }
    });

    let delete_box: gtk::ListBox = get(&b, "delete_box");
    delete_box.set_visible(e.is_some());
    if let Some(ev) = e.clone() {
        let link = todav_app::links::event(&ev.uid);
        get::<adw::ButtonRow>(&b, "copy_link")
            .connect_activated(move |row| crate::open::copy_link(row, link.clone()));
        let d = dialog.downgrade();
        get::<adw::ButtonRow>(&b, "delete").connect_activated(move |_| {
            let Some(d) = d.upgrade() else { return };
            let ev = ev.clone();
            let done = d.clone();
            let del = move |scope: Scope| {
                match core().delete_event(
                    ev.list_href.clone(),
                    ev.uid.clone(),
                    ev.recurrence_id,
                    scope,
                ) {
                    Ok(()) => {
                        request_sync();
                        crate::window::undo_toast(
                            &gettext("“{title}” deleted").replace("{title}", &ev.summary),
                        );
                    }
                    Err(err) => crate::window::toast(&err.to_string()),
                }
                done.close();
            };
            if ev.recurring {
                ask_scope(&d, &gettext("Delete Repeating Event?"), true, del);
            } else {
                del(Scope::All);
            }
        });
    }

    let d = dialog.downgrade();
    get::<gtk::Button>(&b, "save").connect_clicked(move |_| {
        let Some(d) = d.upgrade() else { return };
        let Some(list) = writable
            .get(calendar.selected() as usize)
            .map(|l| l.href.clone())
        else {
            crate::window::toast(&gettext("No calendar to save to"));
            return;
        };
        let _ = core().set_setting("last_calendar".into(), list.clone());
        let new_time = read();
        let text = |w: &adw::EntryRow| w.text().trim().to_string();
        let notes = description.buffer().property::<String>("text");
        let picked = repeat.selected() as usize;
        let new_repeat = (1..=4).contains(&picked).then(|| Repeat {
            freq: FREQS[picked - 1],
            interval: interval.value() as i64,
            until: until
                .enables_expansion()
                .then(|| dates::midnight(&until_cal.date())),
        });
        let offsets = reminders.offsets();
        let new_priority = (priority.selected() != old_priority)
            .then(|| crate::window::priority_value(priority.selected()));
        let Some(ev) = e.clone() else {
            // New: the title and time first, the rest as an edit of it.
            let title = Some(text(&summary))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| gettext("New Event"));
            let added = core().together(|| {
                let uid = core().add_event(list.clone(), title, new_time, offsets)?;
                let rest = EventPatch {
                    location: Some(text(&location)).filter(|s| !s.is_empty()),
                    description: Some(notes).filter(|s| !s.is_empty()),
                    repeat: new_repeat.map(Some),
                    priority: new_priority,
                    ..Default::default()
                };
                if rest.location.is_some()
                    || rest.description.is_some()
                    || rest.repeat.is_some()
                    || rest.priority.is_some()
                {
                    let _ = core().update_event(list, uid, None, Scope::All, rest);
                }
                Ok::<_, todav_core::Error>(())
            });
            match added {
                Ok(()) => request_sync(),
                Err(err) => crate::window::toast(&err.to_string()),
            }
            d.close();
            return;
        };
        // Only what changed.
        let changed =
            |new: String, old: Option<&str>| (new != old.unwrap_or_default()).then_some(new);
        let old_offsets: Vec<i64> = ev.reminders.iter().map(|r| r - ev.start).collect();
        let old_time = EventTime {
            start: ev.start,
            end: ev.end,
            all_day: ev.all_day,
        };
        let repeat_changed = match (&rule, picked) {
            (Some(_), 5) => false, // "Custom" kept
            (Some(r), _) => {
                plain(r)
                    != new_repeat.as_ref().map(|n| {
                        (
                            FREQS.iter().position(|f| *f == n.freq).unwrap() + 1,
                            n.interval,
                            n.until,
                        )
                    })
            }
            (None, p) => p != 0,
        };
        let patch = EventPatch {
            summary: changed(text(&summary), Some(&ev.summary)).filter(|s| !s.is_empty()),
            location: changed(text(&location), ev.location.as_deref()),
            description: changed(notes, ev.description.as_deref()),
            time: (new_time != old_time).then_some(new_time),
            reminders: (offsets != old_offsets).then_some(offsets),
            repeat: repeat_changed.then_some(new_repeat),
            priority: new_priority,
        };
        let moved = list != ev.list_href;
        let nothing = patch.summary.is_none()
            && patch.location.is_none()
            && patch.description.is_none()
            && patch.time.is_none()
            && patch.reminders.is_none()
            && patch.repeat.is_none()
            && patch.priority.is_none();
        let done = d.clone();
        let save = move |scope: Scope| {
            let r = core().together(|| {
                if !nothing {
                    core().update_event(
                        ev.list_href.clone(),
                        ev.uid.clone(),
                        ev.recurrence_id,
                        scope,
                        patch.clone(),
                    )?;
                }
                if moved {
                    core().move_event(ev.list_href.clone(), ev.uid.clone(), list.clone())?;
                }
                Ok::<_, todav_core::Error>(())
            });
            match r {
                Ok(()) => request_sync(),
                Err(err) => crate::window::toast(&err.to_string()),
            }
            done.close();
        };
        // How a series repeats, or which calendar it is in, is about all of it.
        if ev.recurring && !nothing && !repeat_changed && !moved {
            ask_scope(&d, &gettext("Change Repeating Event?"), false, save);
        } else {
            save(Scope::All);
        }
    });
    dialog.present(Some(&c.split));
    get(&b, "summary")
}
