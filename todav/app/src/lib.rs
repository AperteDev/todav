//! What every Todav front-end does over the core, without a toolkit: the synced settings with
//! their defaults, dates as the app shows them, task trees, the calendar's items, layout and
//! drops, and the background service (sync worker, push listener, reminders).
//!
//! No text: it returns what to show (enums, times, ids) and the front-end words and formats it
//! in its locale. Times are local wall-clock seconds, as in the core (`Client::local_now`).

pub mod accounts;
pub mod calendar;
pub mod dates;
pub mod lists;
pub mod prefs;
pub mod service;
pub mod tasks;

pub const DAY: i64 = 86400;
