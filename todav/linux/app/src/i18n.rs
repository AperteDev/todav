//! Translations and the locale, through glibc: gettext with the `todav` domain (catalogs in
//! `po/`, refreshed by `scripts/i18n.sh`) and the first day of the week. Mark text with
//! `gettext`, `ngettext` for counts, `pgettext` where a word needs its context; xgettext finds
//! these calls. Placeholders are named (`{title}`) and filled with `replace`.

use std::ffi::{CStr, CString, c_char, c_ulong};
use std::sync::OnceLock;

const DOMAIN: &str = "todav";

unsafe extern "C" {
    #[link_name = "gettext"]
    fn c_gettext(msgid: *const c_char) -> *mut c_char;
    #[link_name = "ngettext"]
    fn c_ngettext(one: *const c_char, many: *const c_char, n: c_ulong) -> *mut c_char;
    fn bindtextdomain(domain: *const c_char, dir: *const c_char) -> *mut c_char;
    fn bind_textdomain_codeset(domain: *const c_char, codeset: *const c_char) -> *mut c_char;
    fn textdomain(domain: *const c_char) -> *mut c_char;
}

fn c(s: &str) -> CString {
    CString::new(s).expect("no NUL in text")
}

/// The returned text is gettext's (or the argument itself): copy it before the next call.
fn owned(p: *const c_char) -> String {
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Use the user's locale and this app's catalogs; before any text is shown or built (GtkBuilder
/// translates through the default domain set here).
pub fn init() {
    unsafe { libc::setlocale(libc::LC_ALL, c"".as_ptr()) };
    let dir = locale_dir();
    let (domain, dir) = (c(DOMAIN), c(&dir.to_string_lossy()));
    unsafe {
        bindtextdomain(domain.as_ptr(), dir.as_ptr());
        bind_textdomain_codeset(domain.as_ptr(), c"UTF-8".as_ptr());
        textdomain(domain.as_ptr());
    }
}

/// Installed (`make install-data`), the catalogs are in `../share/locale` next to the binary's
/// directory (/usr, ~/.local, /app); a build run uses the ones build.rs compiled.
fn locale_dir() -> std::path::PathBuf {
    let installed = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.parent()?.join("share/locale")));
    let has_catalogs = |d: &std::path::Path| {
        std::fs::read_dir(d).is_ok_and(|mut langs| {
            langs.any(|l| l.is_ok_and(|l| l.path().join("LC_MESSAGES/todav.mo").exists()))
        })
    };
    match installed {
        Some(d) if has_catalogs(&d) => d,
        _ => concat!(env!("OUT_DIR"), "/locale").into(),
    }
}

pub fn gettext(msgid: &str) -> String {
    let m = c(msgid);
    owned(unsafe { c_gettext(m.as_ptr()) })
}

/// The form of `one`/`many` for `n` in the user's language.
pub fn ngettext(one: &str, many: &str, n: u64) -> String {
    let (o, m) = (c(one), c(many));
    owned(unsafe { c_ngettext(o.as_ptr(), m.as_ptr(), n as c_ulong) })
}

/// Marks text in a constant for xgettext; `gettext` translates it where it is shown.
pub const fn gettext_noop(msgid: &str) -> &str {
    msgid
}

/// `msgid` as meant in `context` (gettext's msgctxt), for words that translate differently.
pub fn pgettext(context: &str, msgid: &str) -> String {
    let key = format!("{context}\u{4}{msgid}");
    let t = gettext(&key);
    if t == key { msgid.to_string() } else { t }
}

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
