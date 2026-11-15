//! Alarm reminders (`todav_app::prefs::alarm_reminder`): a reminder that rings until it is acted
//! on, for `ALARM_SECONDS` at most. GNOME has no alarms, so the app makes its own: a sound
//! looping meanwhile and a window with the notification's choices. Where the GNOME extension is
//! installed it shows the alarm over everything instead (Wayland does not let a background app
//! take the focus), told by the `AlarmRang` signal and answering with `AlarmShown` and
//! `AlarmAction`; the app's window waits a moment for that answer.

use crate::window::{get, view};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

/// How long the app waits for the extension to say it shows the alarm before showing its own.
const EXTENSION_WAIT: Duration = Duration::from_millis(1500);

const SOUND: &str = "/dev/aperte/Todav/alarm.wav";

/// A reminder that rings: `key` is its notification's id, `uid` its task's or event's, `at` the
/// occurrence it is for (`i64::MIN` for none, as `app.snooze` takes it).
#[derive(Clone)]
pub struct Alarm {
    pub key: String,
    pub uid: String,
    pub task: bool,
    pub at: i64,
    pub title: String,
    pub body: String,
}

struct Ring {
    alarm: Alarm,
    window: Option<adw::Window>,
    /// The extension shows it.
    shown_elsewhere: bool,
    /// Waiting for the extension, and for the time to run out.
    wait: Option<glib::SourceId>,
    limit: Option<glib::SourceId>,
}

/// What plays: the sound looping, or (where GTK has no media backend) the display's beep.
enum Sound {
    File(gtk::MediaFile),
    Beep(glib::SourceId),
}

thread_local! {
    static RINGING: RefCell<HashMap<String, Ring>> = RefCell::default();
    static SOUND_ON: RefCell<Option<Sound>> = const { RefCell::new(None) };
}

/// Start the alarm (again, when it rings already).
pub fn start(alarm: Alarm) {
    end(&alarm.key);
    let key = alarm.key.clone();
    let wait = glib::timeout_add_local_once(EXTENSION_WAIT, {
        let key = key.clone();
        move || {
            let show = RINGING.with(|r| {
                r.borrow_mut().get_mut(&key).is_some_and(|r| {
                    r.wait = None;
                    !r.shown_elsewhere
                })
            });
            if show {
                show_window(&key);
            }
        }
    });
    let limit = glib::timeout_add_seconds_local_once(todav_app::prefs::ALARM_SECONDS as u32, {
        let key = key.clone();
        move || {
            RINGING.with(|r| {
                if let Some(r) = r.borrow_mut().get_mut(&key) {
                    r.limit = None;
                }
            });
            end(&key);
        }
    });
    crate::dbus::alarm_rang(&alarm);
    RINGING.with(|r| {
        r.borrow_mut().insert(
            key,
            Ring {
                alarm,
                window: None,
                shown_elsewhere: false,
                wait: Some(wait),
                limit: Some(limit),
            },
        );
    });
    play();
}

/// The alarm is over (acted on, stopped, gone or out of time): silent, its window closed, the
/// extension told. A notification stays as it is.
pub fn end(key: &str) {
    let Some(ring) = RINGING.with(|r| r.borrow_mut().remove(key)) else {
        return;
    };
    for id in [ring.wait, ring.limit].into_iter().flatten() {
        id.remove();
    }
    if let Some(w) = ring.window {
        w.destroy();
    }
    crate::dbus::alarm_over(key);
    if RINGING.with(|r| r.borrow().is_empty()) {
        silence();
    }
}

/// The extension shows the alarm: the app shows none.
pub fn shown_elsewhere(key: &str) {
    RINGING.with(|r| {
        if let Some(r) = r.borrow_mut().get_mut(key) {
            r.shown_elsewhere = true;
            if let Some(id) = r.wait.take() {
                id.remove();
            }
        }
    });
}

/// A choice on the alarm `key` (`done`, `snooze` or `stop`), from its window or the extension's.
pub fn act(key: &str, action: &str) {
    let Some(a) = RINGING.with(|r| r.borrow().get(key).map(|r| r.alarm.clone())) else {
        return;
    };
    match action {
        "done" if a.task => crate::complete(&a.uid),
        "snooze" => crate::snooze(a.task, a.uid, a.at),
        _ => end(key),
    }
}

fn show_window(key: &str) {
    let Some(alarm) = RINGING.with(|r| r.borrow().get(key).map(|r| r.alarm.clone())) else {
        return;
    };
    let b = view!("alarm");
    let window: adw::Window = get(&b, "window");
    get::<gtk::Label>(&b, "summary").set_label(&alarm.title);
    let body: gtk::Label = get(&b, "body");
    body.set_label(&alarm.body);
    body.set_visible(!alarm.body.is_empty());
    get::<gtk::Button>(&b, "done").set_visible(alarm.task);
    for (id, action) in [("done", "done"), ("snooze", "snooze"), ("stop", "stop")] {
        let key = key.to_string();
        get::<gtk::Button>(&b, id).connect_clicked(move |_| act(&key, action));
    }
    let k = key.to_string();
    window.connect_close_request(move |_| {
        act(&k, "stop");
        glib::Propagation::Proceed
    });
    if let Some(app) = gio::Application::default().and_downcast::<gtk::Application>() {
        window.set_application(Some(&app));
    }
    window.present();
    RINGING.with(|r| {
        if let Some(r) = r.borrow_mut().get_mut(key) {
            r.window = Some(window);
        }
    });
}

/// The sound, if it does not play yet.
fn play() {
    if SOUND_ON.with(|s| s.borrow().is_some()) {
        return;
    }
    let file = gtk::MediaFile::for_resource(SOUND);
    file.set_loop(true);
    file.play();
    // No media backend, or no codec for it: the display's beep every two seconds.
    file.connect_error_notify(|f| {
        if f.error().is_some() {
            SOUND_ON.with(|s| *s.borrow_mut() = Some(beep()));
        }
    });
    let sound = if file.error().is_some() {
        beep()
    } else {
        Sound::File(file)
    };
    SOUND_ON.with(|s| *s.borrow_mut() = Some(sound));
}

fn beep() -> Sound {
    Sound::Beep(glib::timeout_add_local(Duration::from_secs(2), || {
        if let Some(d) = gtk::gdk::Display::default() {
            d.beep();
        }
        glib::ControlFlow::Continue
    }))
}

fn silence() {
    match SOUND_ON.with(|s| s.borrow_mut().take()) {
        Some(Sound::File(f)) => f.pause(),
        Some(Sound::Beep(id)) => id.remove(),
        None => {}
    }
}
