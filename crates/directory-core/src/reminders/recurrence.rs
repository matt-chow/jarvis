//! Reminder repeat rules (D-049): daily, weekly (optionally on chosen
//! weekdays), monthly, or yearly, every `interval` periods from the anchor
//! (the first occurrence). Unlike calendar events, a day that a month lacks
//! clamps to its last day: the 31st repeats on Feb 28 (29 in leap years),
//! and Feb 29 yearly falls on Feb 28 in other years.

use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_INTERVAL: u32 = 999;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub freq: Freq,
    #[serde(default = "one")]
    pub interval: u32,
    /// Weekly only: 0 = Monday … 6 = Sunday. Empty means the anchor's weekday.
    #[serde(default)]
    pub weekdays: Vec<u32>,
    /// Last date an occurrence may fall on.
    #[serde(default)]
    pub until: Option<String>,
    /// The first occurrence; month and year rules keep its day. Set from the
    /// due date by the service, never trusted from a client.
    #[serde(default)]
    pub anchor: String,
}

/// Validate and normalise a rule anchored at `anchor` (a valid date).
pub fn check(mut r: Rule, anchor: &str) -> Result<Rule, (&'static str, &'static str)> {
    if !(1..=MAX_INTERVAL).contains(&r.interval) {
        return Err(("recurrence", "interval must be 1–999"));
    }
    if r.freq != Freq::Weekly && !r.weekdays.is_empty() {
        return Err(("recurrence", "weekdays are for weekly rules"));
    }
    if r.weekdays.iter().any(|d| *d > 6) {
        return Err(("recurrence", "weekdays are 0 (Monday) to 6"));
    }
    r.weekdays.sort_unstable();
    r.weekdays.dedup();
    if let Some(u) = &r.until {
        validate::date(u).map_err(|w| ("recurrence", w))?;
        if u.as_str() < anchor {
            return Err(("recurrence", "until is before the first date"));
        }
    }
    r.anchor = anchor.to_owned();
    Ok(r)
}

fn ymd(d: &str) -> (i64, i64, i64) {
    let n = |r: std::ops::Range<usize>| d[r].parse::<i64>().unwrap_or(0);
    (n(0..4), n(5..7), n(8..10))
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The date `months` months after year/month `y`/`m`, on `day` or the month's last day.
fn clamped(y: i64, m: i64, months: i64, day: i64) -> String {
    let idx = y * 12 + (m - 1) + months;
    let (y, m) = (idx.div_euclid(12), idx.rem_euclid(12) + 1);
    format!("{y:04}-{m:02}-{:02}", day.min(days_in_month(y, m)))
}

fn day_no(d: &str) -> i64 {
    validate::days_from_civil(d)
}

fn weekdays(r: &Rule) -> Vec<u32> {
    if r.weekdays.is_empty() {
        vec![validate::weekday(&r.anchor)]
    } else {
        r.weekdays.clone()
    }
}

fn within(r: &Rule, d: &str) -> bool {
    d >= r.anchor.as_str() && r.until.as_deref().is_none_or(|u| d <= u)
}

/// Whether `date` is an occurrence.
pub fn matches(r: &Rule, date: &str) -> bool {
    if !within(r, date) {
        return false;
    }
    let i = i64::from(r.interval);
    let (ay, am, ad) = ymd(&r.anchor);
    match r.freq {
        Freq::Daily => (day_no(date) - day_no(&r.anchor)) % i == 0,
        Freq::Weekly => {
            let monday = |d: &str| day_no(d) - i64::from(validate::weekday(d));
            (monday(date) - monday(&r.anchor)) / 7 % i == 0
                && weekdays(r).contains(&validate::weekday(date))
        }
        Freq::Monthly => {
            let (y, m, _) = ymd(date);
            let k = (y * 12 + m) - (ay * 12 + am);
            k % i == 0 && clamped(ay, am, k, ad) == date
        }
        Freq::Yearly => {
            let (y, _, _) = ymd(date);
            (y - ay) % i == 0 && clamped(ay, am, (y - ay) * 12, ad) == date
        }
    }
}

/// The first occurrence strictly after `date`.
pub fn next_after(r: &Rule, date: &str) -> Option<String> {
    let from = if date < r.anchor.as_str() {
        validate::add_days(&r.anchor, -1)
    } else {
        date.to_owned()
    };
    let i = i64::from(r.interval);
    let (ay, am, ad) = ymd(&r.anchor);
    let next = match r.freq {
        Freq::Daily => {
            let n = (day_no(&from) - day_no(&r.anchor)).div_euclid(i) + 1;
            validate::add_days(&r.anchor, n * i)
        }
        Freq::Weekly => (1..=7 * i + 7)
            .map(|k| validate::add_days(&from, k))
            .find(|d| matches(r, d))?,
        Freq::Monthly | Freq::Yearly => {
            let step = if r.freq == Freq::Monthly { i } else { 12 * i };
            let (y, m, _) = ymd(&from);
            let mut k = ((y * 12 + m) - (ay * 12 + am)).max(0) / step * step;
            loop {
                let d = clamped(ay, am, k, ad);
                if d.as_str() > from.as_str() {
                    break d;
                }
                k += step;
            }
        }
    };
    within(r, &next).then_some(next)
}

/// The next occurrence after completing one on `completed` (local date),
/// counted from that day: `interval` periods later, or for weekly rules
/// with weekdays, the next chosen weekday.
pub fn after_completion(r: &Rule, completed: &str) -> Option<String> {
    let i = i64::from(r.interval);
    let (y, m, d) = ymd(completed);
    let next = match r.freq {
        Freq::Daily => validate::add_days(completed, i),
        Freq::Weekly if !r.weekdays.is_empty() => (1..=7)
            .map(|k| validate::add_days(completed, k))
            .find(|x| r.weekdays.contains(&validate::weekday(x)))?,
        Freq::Weekly => validate::add_days(completed, 7 * i),
        Freq::Monthly => clamped(y, m, i, d),
        Freq::Yearly => clamped(y, m, 12 * i, d),
    };
    r.until
        .as_deref()
        .is_none_or(|u| next.as_str() <= u)
        .then_some(next)
}

/// Occurrences in `from..=to`, at most `max`.
pub fn between(r: &Rule, from: &str, to: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = if matches(r, from) {
        Some(from.to_owned())
    } else {
        next_after(r, from)
    };
    while let Some(d) = cur {
        if d.as_str() > to || out.len() >= max {
            break;
        }
        cur = next_after(r, &d);
        out.push(d);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(freq: Freq, interval: u32, weekdays: &[u32], anchor: &str) -> Rule {
        check(
            Rule {
                freq,
                interval,
                weekdays: weekdays.to_vec(),
                until: None,
                anchor: String::new(),
            },
            anchor,
        )
        .unwrap()
    }

    #[test]
    fn validates() {
        let base = Rule {
            freq: Freq::Daily,
            interval: 0,
            weekdays: vec![],
            until: None,
            anchor: "x".into(),
        };
        assert!(check(base.clone(), "2026-01-01").is_err());
        assert!(check(
            Rule {
                interval: 1,
                weekdays: vec![1],
                ..base.clone()
            },
            "2026-01-01"
        )
        .is_err());
        let weekly = Rule {
            freq: Freq::Weekly,
            interval: 1,
            weekdays: vec![9],
            ..base.clone()
        };
        assert!(check(weekly.clone(), "2026-01-01").is_err());
        let before = Rule {
            interval: 1,
            until: Some("2025-12-31".into()),
            ..base.clone()
        };
        assert!(check(before, "2026-01-01").is_err());
        let ok = check(
            Rule {
                weekdays: vec![2, 0, 2],
                ..weekly
            },
            "2026-01-05",
        )
        .unwrap();
        assert_eq!(
            (ok.weekdays, ok.anchor.as_str()),
            (vec![0, 2], "2026-01-05")
        );
    }

    #[test]
    fn daily_and_custom_interval() {
        let r = rule(Freq::Daily, 3, &[], "2026-11-01");
        assert_eq!(
            between(&r, "2026-10-01", "2026-11-10", 10),
            ["2026-11-01", "2026-11-04", "2026-11-07", "2026-11-10"]
        );
        assert_eq!(next_after(&r, "2026-11-05").as_deref(), Some("2026-11-07"));
        assert!(matches(&r, "2026-11-07") && !matches(&r, "2026-11-06"));
    }

    #[test]
    fn weekly_on_chosen_days_every_other_week() {
        // Monday and Wednesday, every 2 weeks, from Mon 2026-11-02.
        let r = rule(Freq::Weekly, 2, &[0, 2], "2026-11-02");
        assert_eq!(
            between(&r, "2026-11-01", "2026-11-30", 10),
            [
                "2026-11-02",
                "2026-11-04",
                "2026-11-16",
                "2026-11-18",
                "2026-11-30"
            ]
        );
        // No weekdays: the anchor's weekday.
        let plain = rule(Freq::Weekly, 1, &[], "2026-11-05");
        assert_eq!(
            next_after(&plain, "2026-11-05").as_deref(),
            Some("2026-11-12")
        );
    }

    #[test]
    fn month_end_and_leap_years_clamp() {
        let r = rule(Freq::Monthly, 1, &[], "2026-01-31");
        assert_eq!(
            between(&r, "2026-01-01", "2026-05-31", 10),
            [
                "2026-01-31",
                "2026-02-28",
                "2026-03-31",
                "2026-04-30",
                "2026-05-31"
            ]
        );
        let leap = rule(Freq::Monthly, 1, &[], "2028-01-30");
        assert_eq!(
            next_after(&leap, "2028-01-30").as_deref(),
            Some("2028-02-29")
        );
        let feb29 = rule(Freq::Yearly, 1, &[], "2028-02-29");
        assert_eq!(
            between(&feb29, "2028-01-01", "2032-12-31", 10),
            [
                "2028-02-29",
                "2029-02-28",
                "2030-02-28",
                "2031-02-28",
                "2032-02-29"
            ]
        );
        let quarterly = rule(Freq::Monthly, 3, &[], "2026-11-30");
        assert_eq!(
            next_after(&quarterly, "2026-11-30").as_deref(),
            Some("2027-02-28")
        );
        assert_eq!(
            next_after(&quarterly, "2027-02-28").as_deref(),
            Some("2027-05-30")
        );
    }

    #[test]
    fn until_ends_the_series() {
        let mut r = rule(Freq::Daily, 1, &[], "2026-11-01");
        r.until = Some("2026-11-02".into());
        assert_eq!(next_after(&r, "2026-11-01").as_deref(), Some("2026-11-02"));
        assert_eq!(next_after(&r, "2026-11-02"), None);
        assert_eq!(after_completion(&r, "2026-11-02"), None);
    }

    #[test]
    fn completion_based_counts_from_the_completion_day() {
        let r = rule(Freq::Daily, 3, &[], "2026-11-01");
        assert_eq!(
            after_completion(&r, "2026-11-05").as_deref(),
            Some("2026-11-08")
        );
        let m = rule(Freq::Monthly, 1, &[], "2026-01-31");
        assert_eq!(
            after_completion(&m, "2026-01-31").as_deref(),
            Some("2026-02-28")
        );
        let w = rule(Freq::Weekly, 2, &[0, 2], "2026-11-02");
        // Done on Thursday: the next chosen weekday is Monday.
        assert_eq!(
            after_completion(&w, "2026-11-05").as_deref(),
            Some("2026-11-09")
        );
    }
}
