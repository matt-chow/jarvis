//! Deterministic scheduling for JARVIS (DECISIONS D-048): free time, work
//! sessions, and rearranging flexible sessions around fixed commitments.
//!
//! Times are wall-clock minutes on each date, the same model as the client
//! planner (`apps/web/src/lib/planner.ts`) and events (D-021): the caller's
//! local date and time decide "today" and "now", so DST changes never shift a
//! session. Every timed event and non-skipped session counts as busy; all-day
//! events don't. Nothing here writes: callers turn the result into steps the
//! owner approves.

use crate::blocks::{BlockStatus, TimeBlock};
use crate::calendar::Occurrence;
use crate::preferences::Preferences;
use crate::validate;
use serde::{Deserialize, Serialize};

/// Furthest ahead JARVIS looks for time.
pub const MAX_DAYS: i64 = 62;
/// Shortest session JARVIS proposes.
pub const MIN_SESSION: u32 = 15;
/// Sessions start on the quarter hour.
const STEP: u32 = 15;
const DAY: u32 = 24 * 60;

/// Working hours and JARVIS preferences, resolved to minutes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    pub day_start: u32,
    pub day_end: u32,
    /// 0 = Monday … 6 = Sunday.
    pub work_days: Vec<u8>,
    /// Preferred session length.
    pub session: u32,
    pub break_minutes: u32,
    pub max_daily: Option<u32>,
    pub earliest_first: bool,
    /// Assumed length of a task without an estimate.
    pub default_task: u32,
}

impl Default for Rules {
    /// The client's defaults: 09:00–17:00, Monday to Friday, 60-minute sessions.
    fn default() -> Self {
        Self {
            day_start: 9 * 60,
            day_end: 17 * 60,
            work_days: vec![0, 1, 2, 3, 4],
            session: 60,
            break_minutes: 0,
            max_daily: None,
            earliest_first: false,
            default_task: 60,
        }
    }
}

impl Rules {
    pub fn from_prefs(p: &Preferences) -> Self {
        let mut r = Self::default();
        if let Some(h) = &p.planner {
            r.day_start = validate::clock(&h.day_start, false).unwrap_or(r.day_start);
            r.day_end = validate::clock(&h.day_end, true).unwrap_or(r.day_end);
            r.work_days = h.work_days.clone();
            r.session = h.session_minutes;
        }
        if let Some(j) = &p.jarvis {
            if let Some(m) = j
                .avoid_after
                .as_deref()
                .and_then(|t| validate::clock(t, true).ok())
            {
                r.day_end = r.day_end.min(m);
            }
            r.break_minutes = j.break_minutes;
            r.max_daily = j.max_daily_minutes;
            r.earliest_first = j.earliest_first;
            r.default_task = j.default_task_minutes.unwrap_or(r.session);
        } else {
            r.default_task = r.session;
        }
        r
    }

    /// "09:00–17:00, Mon–Fri"-style description for messages.
    pub fn hours_label(&self) -> String {
        format!("{}–{}", clock(self.day_start), clock(self.day_end))
    }
}

/// `HH:MM` for minutes since midnight.
pub fn clock(m: u32) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// The caller's local date and minutes since midnight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Now {
    pub date: String,
    pub minutes: u32,
}

/// Something occupying time on one date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Busy {
    pub date: String,
    pub start: u32,
    pub end: u32,
    pub title: String,
    /// A fixed commitment (event or fixed session).
    pub fixed: bool,
    /// The session's id, when this is a time block.
    pub block_id: Option<String>,
}

/// A time range kept free ("leave Thursday evening free"); no date = every day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// `HH:MM`
    pub start: String,
    /// `HH:MM`, after `start` (`24:00` allowed).
    pub end: String,
}

impl Keep {
    /// Validated minutes, or why not.
    pub fn minutes(&self) -> Result<(u32, u32), &'static str> {
        let s = validate::clock(&self.start, false)?;
        let e = validate::clock(&self.end, true)?;
        if e <= s {
            return Err("must end after it starts");
        }
        if let Some(d) = &self.date {
            validate::date(d)?;
        }
        Ok((s, e))
    }

    pub fn describe(&self) -> String {
        match &self.date {
            Some(d) => format!("{d} {}–{}", self.start, self.end),
            None => format!("{}–{} every day", self.start, self.end),
        }
    }
}

/// A proposed session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub date: String,
    pub start: u32,
    pub end: u32,
}

/// Busy time from event occurrences and sessions between `from` and `to`.
/// Multi-day timed events cover the days in between; skipped sessions and
/// all-day events don't count.
pub fn busy(occurrences: &[Occurrence], blocks: &[TimeBlock], from: &str, to: &str) -> Vec<Busy> {
    let mut out = Vec::new();
    for o in occurrences {
        let (Some(s), Some(e)) = (&o.start_time, &o.end_time) else {
            continue;
        };
        let (Ok(s), Ok(e)) = (validate::clock(s, false), validate::clock(e, true)) else {
            continue;
        };
        let first = validate::days_from_civil(&o.start_date);
        let last = validate::days_from_civil(&o.end_date);
        for d in first..=last {
            let date = validate::civil_from_days(d);
            if date.as_str() < from || date.as_str() > to {
                continue;
            }
            let start = if d == first { s } else { 0 };
            let end = if d == last { e } else { DAY };
            if end > start {
                out.push(Busy {
                    date,
                    start,
                    end,
                    title: o.title.clone(),
                    fixed: o.fixed,
                    block_id: None,
                });
            }
        }
    }
    for b in blocks {
        if b.status == BlockStatus::Skipped || b.date.as_str() < from || b.date.as_str() > to {
            continue;
        }
        let (Ok(s), Ok(e)) = (
            validate::clock(&b.start_time, false),
            validate::clock(&b.end_time, true),
        ) else {
            continue;
        };
        out.push(Busy {
            date: b.date.clone(),
            start: s,
            end: e,
            title: b.title.clone(),
            fixed: b.fixed,
            block_id: Some(b.id.clone()),
        });
    }
    out
}

/// Items on `date` overlapping `start..end`, except sessions in `ignore`.
pub fn overlaps<'b>(
    busy: &'b [Busy],
    date: &str,
    start: u32,
    end: u32,
    ignore: &[&str],
) -> Vec<&'b Busy> {
    busy.iter()
        .filter(|b| {
            b.date == date
                && b.start < end
                && start < b.end
                && !b.block_id.as_deref().is_some_and(|id| ignore.contains(&id))
        })
        .collect()
}

/// The part of `date` JARVIS may schedule in, if any.
fn window(rules: &Rules, date: &str, now: &Now) -> Option<(u32, u32)> {
    if date < now.date.as_str() || !rules.work_days.contains(&(validate::weekday(date) as u8)) {
        return None;
    }
    let mut start = rules.day_start;
    if date == now.date {
        // From the next quarter hour.
        start = start.max((now.minutes + 1).div_ceil(STEP) * STEP);
    }
    (rules.day_end >= start + MIN_SESSION).then_some((start, rules.day_end))
}

/// Free gaps in `window` on `date`, keeping `pad` minutes around busy time
/// and leaving `keep` ranges alone. Gaps start on the quarter hour.
fn gaps(
    busy: &[Busy],
    taken: &[Session],
    keep: &[(Option<String>, u32, u32)],
    date: &str,
    window: (u32, u32),
    pad: u32,
) -> Vec<(u32, u32)> {
    let mut spans: Vec<(u32, u32)> = busy
        .iter()
        .filter(|b| b.date == date)
        .map(|b| (b.start.saturating_sub(pad), (b.end + pad).min(DAY)))
        .chain(
            taken
                .iter()
                .filter(|s| s.date == date)
                .map(|s| (s.start.saturating_sub(pad), (s.end + pad).min(DAY))),
        )
        .chain(
            keep.iter()
                .filter(|(d, _, _)| d.as_deref().is_none_or(|d| d == date))
                .map(|(_, s, e)| (*s, *e)),
        )
        .collect();
    spans.sort_unstable();
    let mut out = Vec::new();
    let mut at = window.0;
    for (s, e) in spans {
        if e <= at {
            continue;
        }
        if s >= window.1 {
            break;
        }
        if s > at {
            out.push((at, s.min(window.1)));
        }
        at = at.max(e);
    }
    if at < window.1 {
        out.push((at, window.1));
    }
    out.into_iter()
        .map(|(s, e)| (s.div_ceil(STEP) * STEP, e))
        .filter(|(s, e)| e >= &(s + MIN_SESSION))
        .collect()
}

fn keep_minutes(keep: &[Keep]) -> Vec<(Option<String>, u32, u32)> {
    keep.iter()
        .filter_map(|k| k.minutes().ok().map(|(s, e)| (k.date.clone(), s, e)))
        .collect()
}

/// Dates from `from` to `to` (at most [`MAX_DAYS`]).
pub fn dates(from: &str, to: &str) -> Vec<String> {
    let a = validate::days_from_civil(from);
    let b = validate::days_from_civil(to).min(a + MAX_DAYS - 1);
    (a..=b).map(validate::civil_from_days).collect()
}

/// What `find_sessions` managed: the sessions, and minutes it couldn't place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub sessions: Vec<Session>,
    pub short: u32,
}

/// Sessions adding up to `total` minutes between `from` and `to`, within
/// working hours, around busy time and kept-free ranges. By default one
/// session per day per pass (spread out); `earliest_first` fills each day.
/// A session shrinks to fit a gap of at least half its length (and 30 min).
#[allow(clippy::too_many_arguments)]
pub fn find_sessions(
    rules: &Rules,
    total: u32,
    session: u32,
    from: &str,
    to: &str,
    now: &Now,
    busy: &[Busy],
    keep: &[Keep],
) -> Found {
    let keep = keep_minutes(keep);
    let start = if from < now.date.as_str() {
        now.date.as_str()
    } else {
        from
    };
    let days: Vec<(String, (u32, u32))> = if start > to {
        vec![]
    } else {
        dates(start, to)
            .into_iter()
            .filter_map(|d| window(rules, &d, now).map(|w| (d, w)))
            .collect()
    };
    let session = session.max(MIN_SESSION);
    let mut taken: Vec<Session> = Vec::new();
    let mut remaining = total;
    let used = |taken: &[Session], date: &str| -> u32 {
        taken
            .iter()
            .filter(|s| s.date == date)
            .map(|s| s.end - s.start)
            .sum()
    };
    // Full-length sessions first; shorter ones only for what's left.
    for shrink in [false, true] {
        loop {
            let mut placed = false;
            for (date, w) in &days {
                loop {
                    if remaining == 0 {
                        break;
                    }
                    let mut want = session.min(remaining);
                    if let Some(cap) = rules.max_daily {
                        want = want.min(cap.saturating_sub(used(&taken, date)));
                    }
                    if want == 0 || want < MIN_SESSION.min(remaining) {
                        break;
                    }
                    let free = gaps(busy, &taken, &keep, date, *w, rules.break_minutes);
                    let slot = free
                        .iter()
                        .find(|(s, e)| e - s >= want)
                        .map(|(s, _)| (*s, want));
                    let slot = slot.or_else(|| {
                        let best = free.iter().max_by_key(|(s, e)| e - s)?;
                        let len = (best.1 - best.0) / 5 * 5;
                        (shrink && len >= 30 && len * 2 >= want).then_some((best.0, len))
                    });
                    let Some((s, len)) = slot else { break };
                    taken.push(Session {
                        date: date.clone(),
                        start: s,
                        end: s + len,
                    });
                    remaining -= len;
                    placed = true;
                    if !rules.earliest_first {
                        break;
                    }
                }
            }
            if !placed || remaining == 0 {
                break;
            }
        }
    }
    taken.sort_by(|a, b| a.date.cmp(&b.date).then(a.start.cmp(&b.start)));
    Found {
        sessions: taken,
        short: remaining,
    }
}

/// A flexible session to place again, and its length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movable {
    pub id: String,
    pub start: u32,
    pub end: u32,
}

/// New times for flexible sessions on `date` within `range`: each keeps its
/// length and order and goes into the earliest free time around everything
/// else (fixed commitments never move). Returns the moves (only sessions
/// whose time changes) and the ids that no longer fit (left where they are).
pub fn rearrange(
    rules: &Rules,
    date: &str,
    range: (u32, u32),
    now: &Now,
    busy: &[Busy],
    movable: &[Movable],
    keep: &[Keep],
) -> (Vec<(String, u32, u32)>, Vec<String>) {
    let keep = keep_minutes(keep);
    let mut w = range;
    if date == now.date {
        w.0 = w.0.max((now.minutes + 1).div_ceil(STEP) * STEP);
    }
    let mut order = movable.to_vec();
    order.sort_by_key(|m| m.start);
    let mut taken: Vec<Session> = Vec::new();
    let mut moves = Vec::new();
    let mut unplaced = Vec::new();
    for m in &order {
        let len = m.end - m.start;
        let free = if w.1 > w.0 {
            gaps(busy, &taken, &keep, date, w, rules.break_minutes)
        } else {
            vec![]
        };
        match free.iter().find(|(s, e)| e - s >= len) {
            Some((s, _)) => {
                taken.push(Session {
                    date: date.into(),
                    start: *s,
                    end: s + len,
                });
                if *s != m.start {
                    moves.push((m.id.clone(), *s, s + len));
                }
            }
            None => unplaced.push(m.id.clone()),
        }
    }
    (moves, unplaced)
}

/// Free time per day between `from` and `to` within working hours.
pub fn free_time(
    rules: &Rules,
    from: &str,
    to: &str,
    now: &Now,
    busy: &[Busy],
) -> Vec<(String, Vec<(u32, u32)>)> {
    let start = if from < now.date.as_str() {
        now.date.as_str()
    } else {
        from
    };
    if start > to {
        return vec![];
    }
    dates(start, to)
        .into_iter()
        .filter_map(|d| {
            let w = window(rules, &d, now)?;
            Some((d.clone(), gaps(busy, &[], &[], &d, w, 0)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(date: &str, hm: u32) -> Now {
        Now {
            date: date.into(),
            minutes: hm,
        }
    }

    fn b(date: &str, s: u32, e: u32, fixed: bool) -> Busy {
        Busy {
            date: date.into(),
            start: s,
            end: e,
            title: "Class".into(),
            fixed,
            block_id: None,
        }
    }

    // 2026-11-02 is a Monday.
    const MON: &str = "2026-11-02";

    #[test]
    fn sessions_spread_one_per_working_day_around_busy_time() {
        let r = Rules::default();
        let busy = [b(MON, 9 * 60, 11 * 60, true)];
        let f = find_sessions(
            &r,
            240,
            60,
            MON,
            "2026-11-06",
            &now(MON, 8 * 60),
            &busy,
            &[],
        );
        assert_eq!(f.short, 0);
        let got: Vec<(&str, u32)> = f
            .sessions
            .iter()
            .map(|s| (s.date.as_str(), s.start))
            .collect();
        assert_eq!(
            got,
            [
                (MON, 11 * 60),
                ("2026-11-03", 9 * 60),
                ("2026-11-04", 9 * 60),
                ("2026-11-05", 9 * 60)
            ]
        );
    }

    #[test]
    fn earliest_first_fills_a_day_and_respects_breaks_and_caps() {
        let r = Rules {
            earliest_first: true,
            break_minutes: 15,
            max_daily: Some(120),
            ..Rules::default()
        };
        let f = find_sessions(&r, 180, 60, MON, "2026-11-06", &now(MON, 8 * 60), &[], &[]);
        let got: Vec<(&str, u32, u32)> = f
            .sessions
            .iter()
            .map(|s| (s.date.as_str(), s.start, s.end))
            .collect();
        assert_eq!(
            got,
            [(MON, 540, 600), (MON, 615, 675), ("2026-11-03", 540, 600)]
        );
    }

    #[test]
    fn today_starts_after_now_and_weekends_and_kept_ranges_are_skipped() {
        let r = Rules::default();
        // Friday 16:10: nothing fits today; the weekend is off.
        let fri = "2026-11-06";
        let keep = [Keep {
            date: Some("2026-11-09".into()),
            start: "09:00".into(),
            end: "12:00".into(),
        }];
        let f = find_sessions(
            &r,
            60,
            60,
            fri,
            "2026-11-09",
            &now(fri, 16 * 60 + 10),
            &[],
            &keep,
        );
        assert_eq!(f.sessions.len(), 1);
        assert_eq!(f.sessions[0].date, "2026-11-09");
        assert_eq!(f.sessions[0].start, 12 * 60);
    }

    #[test]
    fn not_enough_time_reports_the_shortfall_and_shrinks_sessions() {
        let r = Rules::default();
        // Only 10:00–10:45 free on Monday.
        let busy = [
            b(MON, 9 * 60, 10 * 60, true),
            b(MON, 10 * 60 + 45, 17 * 60, true),
        ];
        let f = find_sessions(&r, 120, 60, MON, MON, &now(MON, 0), &busy, &[]);
        assert_eq!(f.sessions.len(), 1);
        assert_eq!((f.sessions[0].start, f.sessions[0].end), (600, 645));
        assert_eq!(f.short, 75);
    }

    #[test]
    fn avoid_after_and_custom_hours_come_from_preferences() {
        let prefs: Preferences = serde_json::from_str(
            r#"{"planner":{"day_start":"08:00","day_end":"22:00","work_days":[0,1,2,3,4,5,6],"session_minutes":50},
               "jarvis":{"avoid_after":"20:00","break_minutes":10}}"#,
        )
        .unwrap();
        let r = Rules::from_prefs(&prefs);
        assert_eq!((r.day_start, r.day_end, r.session), (480, 1200, 50));
        assert_eq!(r.default_task, 50);
        assert_eq!(r.hours_label(), "08:00–20:00");
    }

    #[test]
    fn rearranging_packs_flexible_sessions_around_fixed_classes() {
        let r = Rules::default();
        let busy = [b(MON, 13 * 60, 14 * 60, true)];
        let movable = [
            Movable {
                id: "a".into(),
                start: 15 * 60,
                end: 16 * 60,
            },
            Movable {
                id: "b".into(),
                start: 16 * 60,
                end: 17 * 60,
            },
        ];
        let keep = [Keep {
            date: None,
            start: "12:00".into(),
            end: "12:30".into(),
        }];
        let (moves, unplaced) = rearrange(
            &r,
            MON,
            (12 * 60, 17 * 60),
            &now(MON, 9 * 60),
            &busy,
            &movable,
            &keep,
        );
        assert_eq!(
            moves,
            [
                ("a".into(), 14 * 60, 15 * 60),
                ("b".into(), 15 * 60, 16 * 60)
            ]
        );
        assert!(unplaced.is_empty());
    }

    #[test]
    fn busy_time_covers_multi_day_events_but_not_all_day_or_skipped_items() {
        let occ = [
            Occurrence {
                event_id: "e".into(),
                title: "Trip".into(),
                start_date: MON.into(),
                end_date: "2026-11-03".into(),
                start_time: Some("18:00".into()),
                end_time: Some("09:00".into()),
                fixed: true,
            },
            Occurrence {
                event_id: "f".into(),
                title: "Holiday".into(),
                start_date: MON.into(),
                end_date: MON.into(),
                start_time: None,
                end_time: None,
                fixed: true,
            },
        ];
        let got = busy(&occ, &[], MON, "2026-11-03");
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].start, got[0].end), (1080, DAY));
        assert_eq!((got[1].start, got[1].end), (0, 540));
        assert_eq!(overlaps(&got, MON, 1000, 1100, &[]).len(), 1);
    }

    #[test]
    fn free_time_lists_gaps_within_working_hours() {
        let r = Rules::default();
        let busy = [b(MON, 10 * 60, 11 * 60, false)];
        let got = free_time(&r, MON, MON, &now(MON, 0), &busy);
        assert_eq!(got, [(MON.to_string(), vec![(540, 600), (660, 1020)])]);
    }
}
