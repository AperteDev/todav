//! The locale: the first day of the week, as GTK's calendar reads it from glibc.

use std::sync::OnceLock;

/// The locale's first day of the week (LC_TIME), 0 = Sunday … 6 = Saturday, as GTK's calendar
/// reads it: glibc's week reference day plus its first-weekday offset.
pub fn first_weekday() -> i64 {
    // glibc's _NL_TIME_WEEK_1STDAY and _NL_TIME_FIRST_WEEKDAY (langinfo.h; not in the libc crate).
    const WEEK_1STDAY: libc::nl_item = 0x20066;
    const FIRST_WEEKDAY: libc::nl_item = 0x20068;
    static DAY: OnceLock<i64> = OnceLock::new();
    *DAY.get_or_init(|| {
        // The reference day is a date as a number in the pointer: 19971130 a Sunday, 19971201
        // a Monday.
        let reference = unsafe { libc::nl_langinfo(WEEK_1STDAY) } as usize as u32;
        let base = if reference == 19971201 { 1 } else { 0 };
        let first = unsafe { *libc::nl_langinfo(FIRST_WEEKDAY) } as i64;
        (base + first - 1).rem_euclid(7)
    })
}
