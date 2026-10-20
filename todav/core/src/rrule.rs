//! Recurrence rules (RFC 5545 `RRULE`), the subset real calendars use: `FREQ` daily to yearly,
//! `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY` (with ordinals), `BYMONTHDAY`, `BYMONTH`, `BYSETPOS`,
//! `WKST`. A rule with any other part (`BYYEARDAY`, `BYWEEKNO`, `BYHOUR`, …) is `supported: false`:
//! the series then has only its first occurrence.
//!
//! Times are wall-clock seconds (seconds since the epoch as if the zone were UTC), in the zone of
//! the series' `DTSTART`; converting to real instants is the caller's job.

use crate::ical::{civil_from_days, days_from_civil};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub freq: Freq,
    pub interval: i64,
    pub count: Option<u32>,
    /// The raw `UNTIL` value: a date, a floating or a UTC date-time; see `ical::parse_time`.
    pub until: Option<String>,
    /// (ordinal, weekday): ordinal 0 is every such weekday, 1 the first, -1 the last; Monday is 0.
    pub by_day: Vec<(i32, i64)>,
    pub by_month_day: Vec<i32>,
    pub by_month: Vec<i64>,
    pub by_set_pos: Vec<i32>,
    /// First day of the week, Monday 0.
    pub wkst: i64,
    pub supported: bool,
}

const DAYS: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

/// Periods looked at past the start of the range before giving up on a rule that never matches
/// (say the 30th of February).
const MAX_PERIODS: i64 = 5000;

/// Monday 0 … Sunday 6 of a day number (days since 1970-01-01, a Thursday).
pub fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    let next = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(next.0, next.1, 1) - days_from_civil(y, m, 1)
}

impl Rule {
    pub fn parse(v: &str) -> Option<Rule> {
        let mut r = Rule {
            freq: Freq::Daily,
            interval: 1,
            count: None,
            until: None,
            by_day: Vec::new(),
            by_month_day: Vec::new(),
            by_month: Vec::new(),
            by_set_pos: Vec::new(),
            wkst: 0,
            supported: true,
        };
        let mut freq = None;
        for part in v.split(';').filter(|p| !p.is_empty()) {
            let (k, val) = part.split_once('=')?;
            let list = || val.split(',').map(str::trim);
            match k.to_ascii_uppercase().as_str() {
                "FREQ" => {
                    freq = Some(match val.to_ascii_uppercase().as_str() {
                        "DAILY" => Freq::Daily,
                        "WEEKLY" => Freq::Weekly,
                        "MONTHLY" => Freq::Monthly,
                        "YEARLY" => Freq::Yearly,
                        _ => {
                            r.supported = false; // SECONDLY … HOURLY
                            Freq::Daily
                        }
                    })
                }
                "INTERVAL" => r.interval = val.parse().ok().filter(|&i: &i64| i > 0)?,
                "COUNT" => r.count = Some(val.parse().ok()?),
                "UNTIL" => r.until = Some(val.to_string()),
                "BYDAY" => {
                    for d in list() {
                        let d = d.to_ascii_uppercase();
                        let (ord, name) = d.split_at(d.len().checked_sub(2)?);
                        let wd = DAYS.iter().position(|n| *n == name)? as i64;
                        let ord = match ord {
                            "" => 0,
                            o => o.trim_start_matches('+').parse().ok()?,
                        };
                        r.by_day.push((ord, wd));
                    }
                }
                "BYMONTHDAY" => {
                    for d in list() {
                        r.by_month_day.push(d.parse().ok()?);
                    }
                }
                "BYMONTH" => {
                    for m in list() {
                        r.by_month.push(m.parse().ok()?);
                    }
                }
                "BYSETPOS" => {
                    for p in list() {
                        r.by_set_pos.push(p.parse().ok()?);
                    }
                }
                "WKST" => {
                    r.wkst = DAYS
                        .iter()
                        .position(|n| n.eq_ignore_ascii_case(val))
                        .unwrap_or(0) as i64
                }
                _ => r.supported = false,
            }
        }
        r.freq = freq?;
        Some(r)
    }

    /// Occurrence starts of a series beginning at `start` (always its first occurrence) that fall
    /// in `[from, to)`, soonest first. `until` is `UNTIL` as wall-clock time in the series' zone,
    /// inclusive. All times are wall-clock seconds.
    pub fn expand(&self, start: i64, from: i64, to: i64, until: Option<i64>) -> Vec<i64> {
        let until = until.unwrap_or(i64::MAX);
        let mut out = Vec::new();
        if (from..to).contains(&start) {
            out.push(start);
        }
        if !self.supported {
            return out;
        }
        let (tod, d0) = (start.rem_euclid(86400), start.div_euclid(86400));
        let mut produced = 1; // DTSTART counts as the first occurrence
        // Without COUNT nothing before the range matters: start a period before it.
        let first = match self.count {
            None => (self.period_of(from.div_euclid(86400), d0) / self.interval - 1).max(0),
            Some(_) => 0,
        };
        for k in first..first + MAX_PERIODS {
            let days = self.candidates(d0, k * self.interval);
            let Some(&last) = days.last() else {
                continue;
            };
            for day in self.set_pos(days) {
                let t = day * 86400 + tod;
                if t <= start {
                    continue;
                }
                if t > until || t >= to || self.count.is_some_and(|c| produced >= c) {
                    return out;
                }
                produced += 1;
                if t >= from {
                    out.push(t);
                }
            }
            if last * 86400 > until.min(to) {
                break;
            }
        }
        out
    }

    /// Whole periods (days, weeks, months, years) from the one holding `d0` to the one holding `day`.
    fn period_of(&self, day: i64, d0: i64) -> i64 {
        let (y, m, _) = civil_from_days(day);
        let (y0, m0, _) = civil_from_days(d0);
        match self.freq {
            Freq::Daily => day - d0,
            Freq::Weekly => (self.week_start(day) - self.week_start(d0)) / 7,
            Freq::Monthly => (y * 12 + m) - (y0 * 12 + m0),
            Freq::Yearly => y - y0,
        }
    }

    fn week_start(&self, day: i64) -> i64 {
        day - (weekday(day) - self.wkst).rem_euclid(7)
    }

    /// Candidate days of the period `k` periods after the one holding `d0`, sorted, before
    /// `BYSETPOS`.
    fn candidates(&self, d0: i64, k: i64) -> Vec<i64> {
        let (y0, m0, md0) = civil_from_days(d0);
        let month_ok =
            |day: i64| self.by_month.is_empty() || self.by_month.contains(&civil_from_days(day).1);
        let mut days: Vec<i64> = match self.freq {
            Freq::Daily => {
                let day = d0 + k;
                let (_, _, md) = civil_from_days(day);
                let dim = {
                    let (y, m, _) = civil_from_days(day);
                    days_in_month(y, m)
                };
                let md_ok = self.by_month_day.is_empty()
                    || self
                        .by_month_day
                        .iter()
                        .any(|&d| month_day(d, dim) == Some(md));
                let wd_ok =
                    self.by_day.is_empty() || self.by_day.iter().any(|&(_, w)| w == weekday(day));
                if md_ok && wd_ok { vec![day] } else { vec![] }
            }
            Freq::Weekly => {
                let ws = self.week_start(d0) + 7 * k;
                let wds: Vec<i64> = if self.by_day.is_empty() {
                    vec![weekday(d0)]
                } else {
                    self.by_day.iter().map(|&(_, w)| w).collect()
                };
                wds.iter()
                    .map(|&w| ws + (w - self.wkst).rem_euclid(7))
                    .collect()
            }
            Freq::Monthly => {
                let mi = y0 * 12 + (m0 - 1) + k;
                self.in_month(mi.div_euclid(12), mi.rem_euclid(12) + 1, md0)
            }
            Freq::Yearly => {
                let y = y0 + k;
                if !self.by_month.is_empty() {
                    self.by_month
                        .iter()
                        .flat_map(|&m| self.in_month(y, m, md0))
                        .collect()
                } else if !self.by_day.is_empty() && self.by_month_day.is_empty() {
                    // Ordinals count within the year.
                    let (a, b) = (days_from_civil(y, 1, 1), days_from_civil(y + 1, 1, 1));
                    self.by_day
                        .iter()
                        .flat_map(|&(ord, w)| nth_weekdays(a, b, ord, w))
                        .collect()
                } else if !self.by_month_day.is_empty() {
                    (1..=12).flat_map(|m| self.in_month(y, m, md0)).collect()
                } else {
                    self.in_month(y, m0, md0)
                }
            }
        };
        days.retain(|&d| month_ok(d));
        days.sort();
        days.dedup();
        days
    }

    /// Days of month `m` of year `y` given by `BYMONTHDAY` and/or `BYDAY` (both: those in
    /// both), or else day `md0` of it when the month has one.
    fn in_month(&self, y: i64, m: i64, md0: i64) -> Vec<i64> {
        let first = days_from_civil(y, m, 1);
        let dim = days_in_month(y, m);
        let by_md: Vec<i64> = self
            .by_month_day
            .iter()
            .filter_map(|&d| month_day(d, dim))
            .map(|md| first + md - 1)
            .collect();
        let by_wd: Vec<i64> = self
            .by_day
            .iter()
            .flat_map(|&(ord, w)| nth_weekdays(first, first + dim, ord, w))
            .collect();
        match (self.by_month_day.is_empty(), self.by_day.is_empty()) {
            (false, false) => by_md.into_iter().filter(|d| by_wd.contains(d)).collect(),
            (false, true) => by_md,
            (true, false) => by_wd,
            (true, true) if md0 <= dim => vec![first + md0 - 1],
            (true, true) => vec![],
        }
    }

    fn set_pos(&self, days: Vec<i64>) -> Vec<i64> {
        if self.by_set_pos.is_empty() {
            return days;
        }
        let n = days.len() as i32;
        let mut out: Vec<i64> = self
            .by_set_pos
            .iter()
            .filter_map(|&p| {
                let i = if p > 0 { p - 1 } else { n + p };
                (0..n).contains(&i).then(|| days[i as usize])
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// Day of month (1-based) for a `BYMONTHDAY` value; negative counts from the end.
fn month_day(d: i32, dim: i64) -> Option<i64> {
    let md = if d > 0 { d as i64 } else { dim + 1 + d as i64 };
    (1..=dim).contains(&md).then_some(md)
}

/// Days in `[a, b)` that are weekday `w`: all of them (`ord` 0), the `ord`-th, or the `-ord`-th
/// from the end.
fn nth_weekdays(a: i64, b: i64, ord: i32, w: i64) -> Vec<i64> {
    let first = a + (w - weekday(a)).rem_euclid(7);
    let all: Vec<i64> = (first..b).step_by(7).collect();
    match ord {
        0 => all,
        o if o > 0 => all.get(o as usize - 1).copied().into_iter().collect(),
        o => all
            .len()
            .checked_sub(o.unsigned_abs() as usize)
            .map(|i| all[i])
            .into_iter()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ical::{fmt_utc, parse_time};

    fn t(s: &str) -> i64 {
        parse_time(s).unwrap()
    }

    /// Occurrences of `rule` from `start` in [from, to), as `YYYYMMDDTHHMMSS`.
    fn occ(rule: &str, start: &str, from: &str, to: &str) -> Vec<String> {
        let r = Rule::parse(rule).unwrap();
        let until = r.until.as_deref().map(|u| {
            let v = t(u);
            if u.len() == 8 { v + 86399 } else { v }
        });
        r.expand(t(start), t(from), t(to), until)
            .into_iter()
            .map(|x| fmt_utc(x)[..15].to_string())
            .collect()
    }

    #[test]
    fn weekly_by_day_until_inclusive() {
        // Every weekday from Monday 2018-07-16 09:00 until 2018-07-23 05:59:59.
        assert_eq!(
            occ(
                "FREQ=WEEKLY;WKST=MO;UNTIL=20180723T055959Z;BYDAY=MO,TU,WE,TH,FR",
                "20180716T090000",
                "20180101",
                "20190101"
            ),
            [
                "20180716T090000",
                "20180717T090000",
                "20180718T090000",
                "20180719T090000",
                "20180720T090000"
            ]
        );
        // UNTIL at the occurrence itself includes it.
        assert_eq!(
            occ(
                "FREQ=WEEKLY;UNTIL=20261027T100000;BYDAY=TU",
                "20261020T100000",
                "20261001",
                "20270101"
            ),
            ["20261020T100000", "20261027T100000"]
        );
    }

    #[test]
    fn yearly_birthdays_jump_to_the_range() {
        assert_eq!(
            occ("FREQ=YEARLY", "19850314", "20260101", "20280101"),
            ["20260314T000000", "20270314T000000"]
        );
        // 29 February only in leap years.
        assert_eq!(
            occ("FREQ=YEARLY", "20000229", "20250101", "20290101"),
            ["20280229T000000"]
        );
    }

    #[test]
    fn time_zone_rules_last_and_nth_sunday() {
        // Daylight saving from the last Sunday in March, standard from the last in October.
        assert_eq!(
            occ(
                "FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU",
                "19810329T020000",
                "20260101",
                "20270101"
            ),
            ["20260329T020000"]
        );
        assert_eq!(
            occ(
                "FREQ=YEARLY;INTERVAL=1;BYDAY=-1SU;BYMONTH=10",
                "16010101T030000",
                "20260101",
                "20280101"
            ),
            ["20261025T030000", "20271031T030000"]
        );
        // US: second Sunday in March, first in November.
        assert_eq!(
            occ(
                "FREQ=YEARLY;BYDAY=2SU;BYMONTH=3",
                "16010101T020000",
                "20260101",
                "20270101"
            ),
            ["20260308T020000"]
        );
        assert_eq!(
            occ(
                "FREQ=YEARLY;BYMONTH=11;BYDAY=1SU",
                "16010101T020000",
                "20260101",
                "20270101"
            ),
            ["20261101T020000"]
        );
    }

    #[test]
    fn monthly_forms() {
        // The 31st only in months that have one.
        assert_eq!(
            occ("FREQ=MONTHLY", "20260131T080000", "20260101", "20260601"),
            ["20260131T080000", "20260331T080000", "20260531T080000"]
        );
        // Last weekday of the month (Outlook).
        assert_eq!(
            occ(
                "FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1",
                "20260130T170000",
                "20260101",
                "20260501"
            ),
            [
                "20260130T170000",
                "20260227T170000",
                "20260331T170000",
                "20260430T170000"
            ]
        );
        // Second Tuesday, every other month, three times.
        assert_eq!(
            occ(
                "FREQ=MONTHLY;INTERVAL=2;BYDAY=2TU;COUNT=3",
                "20260113T100000",
                "20250101",
                "20300101"
            ),
            ["20260113T100000", "20260310T100000", "20260512T100000"]
        );
        // Friday the 13th.
        assert_eq!(
            occ(
                "FREQ=MONTHLY;BYDAY=FR;BYMONTHDAY=13",
                "20260213",
                "20260101",
                "20270101"
            ),
            ["20260213T000000", "20260313T000000", "20261113T000000"]
        );
        // Last day of the month.
        assert_eq!(
            occ(
                "FREQ=MONTHLY;BYMONTHDAY=-1",
                "20260131",
                "20260101",
                "20260401"
            ),
            ["20260131T000000", "20260228T000000", "20260331T000000"]
        );
    }

    #[test]
    fn daily_count_and_interval() {
        assert_eq!(
            occ(
                "FREQ=DAILY;INTERVAL=2;COUNT=3",
                "20261030T070000",
                "20261001",
                "20261201"
            ),
            ["20261030T070000", "20261101T070000", "20261103T070000"]
        );
        // The range cuts the series; COUNT still counts from the start.
        assert_eq!(
            occ("FREQ=DAILY;COUNT=5", "20261001", "20261004", "20261101"),
            ["20261004T000000", "20261005T000000"]
        );
    }

    #[test]
    fn weekly_interval_respects_wkst() {
        // RFC 5545's example: every other week on Tuesday and Sunday, from Tuesday 1997-08-05.
        assert_eq!(
            occ(
                "FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=MO",
                "19970805T090000",
                "19970101",
                "19980101"
            ),
            [
                "19970805T090000",
                "19970810T090000",
                "19970819T090000",
                "19970824T090000"
            ]
        );
        assert_eq!(
            occ(
                "FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=SU",
                "19970805T090000",
                "19970101",
                "19980101"
            ),
            [
                "19970805T090000",
                "19970817T090000",
                "19970819T090000",
                "19970831T090000"
            ]
        );
    }

    #[test]
    fn unsupported_rules_keep_only_the_start() {
        let r = Rule::parse("FREQ=YEARLY;BYWEEKNO=20;BYDAY=MO").unwrap();
        assert!(!r.supported);
        assert_eq!(
            r.expand(t("20260511"), t("20260101"), t("20300101"), None),
            [t("20260511")]
        );
        assert!(Rule::parse("BYDAY=MO").is_none(), "FREQ is required");
    }

    #[test]
    fn never_matching_rule_ends() {
        assert_eq!(
            occ(
                "FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=30",
                "20260101",
                "20260101",
                "29990101"
            ),
            ["20260101T000000"]
        );
    }
}
