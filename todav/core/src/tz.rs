//! Date and date-time values, and time zones from the VTIMEZONE components inside each object
//! (no tz database needed: a TZID must come with its VTIMEZONE, Windows zone names included).

use crate::ical::{Calendar, Comp, Line, parse_time};
use crate::rrule::Rule;

/// How a DATE or DATE-TIME value reads. The numbers are wall-clock seconds (as if the zone were
/// UTC) except for `Utc`.
#[derive(Debug, Clone, PartialEq)]
pub enum Stamp {
    Date(i64),
    /// No zone: the same wall-clock time wherever you are.
    Floating(i64),
    Utc(i64),
    Zoned(i64, String),
}

impl Stamp {
    /// The wall-clock reading, ignoring the zone (for UTC: the UTC reading).
    pub fn wall(&self) -> i64 {
        match self {
            Stamp::Date(t) | Stamp::Floating(t) | Stamp::Utc(t) | Stamp::Zoned(t, _) => *t,
        }
    }
}

/// The values of a DATE / DATE-TIME property (several for RDATE and EXDATE lists; PERIOD values
/// are skipped).
pub fn stamps(l: &Line) -> Vec<Stamp> {
    let date = l
        .param("VALUE")
        .is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    let tzid = l.param("TZID");
    l.value
        .split(',')
        .filter(|v| !v.contains('/'))
        .filter_map(|v| {
            let v = v.trim();
            let t = parse_time(v)?;
            Some(match tzid {
                _ if date || v.len() == 8 => Stamp::Date(t),
                _ if v.ends_with('Z') => Stamp::Utc(t),
                Some(z) => Stamp::Zoned(t, z.to_string()),
                None => Stamp::Floating(t),
            })
        })
        .collect()
}

pub fn stamp(l: &Line) -> Option<Stamp> {
    stamps(l).into_iter().next()
}

/// One STANDARD or DAYLIGHT part of a VTIMEZONE: from its onsets on, the offset is `to`.
#[derive(Debug, Clone)]
struct Observance {
    /// First onset, wall-clock time in the offset before it (`from`).
    start: i64,
    from: i64,
    to: i64,
    rule: Option<Rule>,
    rdates: Vec<i64>,
}

/// UTC offsets of a zone over time.
#[derive(Debug, Clone)]
pub struct Zone {
    obs: Vec<Observance>,
}

/// `+0100`, `-0530`, `+013045` → seconds.
fn offset(v: &str) -> Option<i64> {
    let (sign, d) = match v.as_bytes().first()? {
        b'-' => (-1, &v[1..]),
        b'+' => (1, &v[1..]),
        _ => (1, v),
    };
    let n = |r: std::ops::Range<usize>| d.get(r).map_or(Some(0), |s| s.parse::<i64>().ok());
    Some(sign * (n(0..2)? * 3600 + n(2..4)? * 60 + n(4..6)?))
}

impl Zone {
    pub fn parse(c: Comp) -> Zone {
        let obs = c
            .subs()
            .iter()
            .filter(|s| {
                s.kind.eq_ignore_ascii_case("STANDARD") || s.kind.eq_ignore_ascii_case("DAYLIGHT")
            })
            .filter_map(|s| {
                Some(Observance {
                    start: parse_time(&s.prop("DTSTART")?.value)?,
                    from: offset(&s.prop("TZOFFSETFROM")?.value)?,
                    to: offset(&s.prop("TZOFFSETTO")?.value)?,
                    rule: s.prop("RRULE").and_then(|r| Rule::parse(&r.value)),
                    rdates: s
                        .props("RDATE")
                        .iter()
                        .flat_map(|l| stamps(l))
                        .map(|s| s.wall())
                        .collect(),
                })
            })
            .collect();
        Zone { obs }
    }

    /// The observance in effect at `t` (UTC if `utc`, else wall-clock time) and its last onset.
    fn current(&self, t: i64, utc: bool) -> Option<&Observance> {
        let mut best: Option<(i64, &Observance)> = None;
        for o in &self.obs {
            // Onsets compared in UTC, or as wall-clock times in the offset before them.
            let key = |onset: i64| if utc { onset - o.from } else { onset };
            let until = o
                .rule
                .as_ref()
                .and_then(|r| r.until.as_deref())
                .and_then(|u| {
                    let v = parse_time(u)?;
                    Some(if u.ends_with('Z') { v + o.from } else { v })
                });
            let from = t - 400 * 86400;
            let mut onsets: Vec<i64> = o.rdates.clone();
            match &o.rule {
                Some(r) => onsets.extend(r.expand(o.start, from - 86400, t + 2 * 86400, until)),
                None => onsets.push(o.start),
            }
            let last = onsets
                .into_iter()
                .filter(|&s| key(s) <= t)
                .max_by_key(|&s| key(s));
            if let Some(s) = last.map(key)
                && best.is_none_or(|(b, _)| s > b)
            {
                best = Some((s, o));
            }
        }
        best.map(|(_, o)| o)
    }

    /// Offset (seconds east of UTC) at wall-clock time `wall`. Before the first onset: the
    /// earliest observance's offset before it.
    pub fn offset_at_wall(&self, wall: i64) -> i64 {
        match self.current(wall, false) {
            Some(o) => o.to,
            None => self
                .obs
                .iter()
                .min_by_key(|o| o.start)
                .map_or(0, |o| o.from),
        }
    }

    pub fn offset_at_utc(&self, utc: i64) -> i64 {
        match self.current(utc, true) {
            Some(o) => o.to,
            None => self
                .obs
                .iter()
                .min_by_key(|o| o.start)
                .map_or(0, |o| o.from),
        }
    }
}

impl Calendar {
    /// The VTIMEZONE with this TZID in the object.
    pub fn zone(&self, tzid: &str) -> Option<Zone> {
        self.comps()
            .into_iter()
            .find(|c| {
                c.kind.eq_ignore_ascii_case("VTIMEZONE") && c.text("TZID").as_deref() == Some(tzid)
            })
            .map(Zone::parse)
    }

    /// The UTC instant of a stamp, when it has one: UTC, or zoned with its VTIMEZONE in the
    /// object. Dates and floating times (and a TZID without a VTIMEZONE) have none.
    pub fn utc(&self, s: &Stamp) -> Option<i64> {
        match s {
            Stamp::Utc(t) => Some(*t),
            Stamp::Zoned(t, id) => self.zone(id).map(|z| t - z.offset_at_wall(*t)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WARSAW: &str = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Warsaw\r\nBEGIN:STANDARD\r\nTZNAME:CET\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nDTSTART:19961027T030000\r\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nTZNAME:CEST\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nDTSTART:19810329T020000\r\nRRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\nBEGIN:VTIMEZONE\r\nTZID:Central Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0500\r\nTZOFFSETTO:-0600\r\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=1SU;BYMONTH=11\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0600\r\nTZOFFSETTO:-0500\r\nRRULE:FREQ=YEARLY;INTERVAL=1;BYDAY=2SU;BYMONTH=3\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:e\r\nDTSTART;TZID=Europe/Warsaw:20260710T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    fn t(s: &str) -> i64 {
        parse_time(s).unwrap()
    }

    #[test]
    fn offsets_follow_daylight_saving() {
        let cal = Calendar::parse(WARSAW).unwrap();
        let waw = cal.zone("Europe/Warsaw").unwrap();
        assert_eq!(waw.offset_at_wall(t("20260115T120000")), 3600);
        assert_eq!(waw.offset_at_wall(t("20260710T120000")), 7200);
        // Switches at 02:00 on 29 March and 03:00 (summer time) on 25 October 2026.
        assert_eq!(waw.offset_at_wall(t("20260329T015959")), 3600);
        assert_eq!(waw.offset_at_wall(t("20260329T030000")), 7200);
        assert_eq!(waw.offset_at_utc(t("20261025T005959")), 7200);
        assert_eq!(waw.offset_at_utc(t("20261025T010000")), 3600);
        // Rules from 1601 still work centuries later.
        let cst = cal.zone("Central Standard Time").unwrap();
        assert_eq!(cst.offset_at_wall(t("20260307T120000")), -6 * 3600);
        assert_eq!(cst.offset_at_wall(t("20260309T120000")), -5 * 3600);
        assert_eq!(cst.offset_at_wall(t("20261102T120000")), -6 * 3600);
        assert_eq!(cst.offset_at_wall(t("21000701T120000")), -5 * 3600);
    }

    #[test]
    fn stamps_read_every_form() {
        let cal = Calendar::parse(WARSAW).unwrap();
        let start = stamp(cal.main().prop("DTSTART").unwrap()).unwrap();
        assert_eq!(
            start,
            Stamp::Zoned(t("20260710T100000"), "Europe/Warsaw".into())
        );
        assert_eq!(cal.utc(&start), Some(t("20260710T080000")));
        let l = |s: &str| {
            Calendar::parse(&format!(
                "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\n{s}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
            ))
            .unwrap()
            .main()
            .prop("X")
            .cloned()
            .unwrap()
        };
        assert_eq!(
            stamps(&l("X;VALUE=DATE:20261020,20261021")),
            [Stamp::Date(t("20261020")), Stamp::Date(t("20261021"))]
        );
        assert_eq!(
            stamp(&l("X:20261020T100000Z")),
            Some(Stamp::Utc(t("20261020T100000")))
        );
        assert_eq!(
            stamp(&l("X:20261020T100000")),
            Some(Stamp::Floating(t("20261020T100000")))
        );
        assert_eq!(
            cal.utc(&Stamp::Zoned(1, "Mars/Olympus".into())),
            None,
            "no VTIMEZONE"
        );
        assert_eq!(offset("-0530"), Some(-19800));
    }

    /// Every VTIMEZONE in the exports in `<repo>/tmp/events/` gives a sane offset for now.
    #[test]
    fn real_zones_parse() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tmp/events");
        for f in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let text = std::fs::read_to_string(f.path()).unwrap();
            for z in text.split("BEGIN:VTIMEZONE").skip(1) {
                let body = z.split("END:VTIMEZONE").next().unwrap();
                let one = format!(
                    "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE{body}END:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:x\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
                );
                let cal = Calendar::parse(&one).unwrap();
                let c = cal
                    .comps()
                    .into_iter()
                    .find(|c| c.kind == "VTIMEZONE")
                    .unwrap();
                let off = Zone::parse(c).offset_at_utc(crate::now());
                assert!((-12 * 3600..=14 * 3600).contains(&off), "{off} for {body}");
            }
        }
    }
}
