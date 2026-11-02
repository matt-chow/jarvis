//! Occurrences, alert times, missed-occurrence policies, and the
//! scheduler's per-reminder step (D-049). Pure: the time-zone answers come
//! from a [`ZoneClock`], and nothing here touches storage.
//!
//! A reminder's `due_date` is its current occurrence (the cursor). With a
//! calendar rule the occurrences after it follow the rule; with
//! completion-based advance the next one only exists once the current one is
//! completed.

use super::model::{minute_of, Advance, Alert, Missed, Reminder, DAY_ANCHOR_MINUTE};
use super::recurrence;
use super::zone::{local_at, local_to_utc, ZoneClock, DAY_MS, MINUTE_MS};
use crate::validate;

/// The furthest the scheduler sleeps before looking at a reminder again.
pub const MAX_SLEEP_MS: i64 = 30 * DAY_MS;
/// How far ahead [`next_wake`] searches for the next alert.
const LOOKAHEAD_MS: i64 = 60 * DAY_MS;
/// Bounds the walks over a rule.
const MAX_STEPS: usize = 2_000;
pub const MAX_AHEAD: usize = 100;

/// One alert of one occurrence, due at `at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fire {
    /// The occurrence date, or `-` when the reminder is undated (never, today).
    pub occurrence: String,
    /// `b<minutes>`, `a<days>-<HHMM>`, or `s<until>` for a snooze.
    pub slot: String,
    pub at: i64,
}

fn local_day_start(clock: &impl ZoneClock, r: &Reminder, date: &str) -> Option<i64> {
    local_to_utc(clock, &r.timezone, date, 0)
}

/// When an occurrence becomes current: its due time, or the start of its day.
pub fn start_at(clock: &impl ZoneClock, r: &Reminder, date: &str) -> Option<i64> {
    match &r.due_time {
        Some(t) => local_to_utc(clock, &r.timezone, date, minute_of(t)),
        None => local_day_start(clock, r, date),
    }
}

/// When an occurrence becomes overdue: its due time, or the end of its day.
pub fn overdue_at(clock: &impl ZoneClock, r: &Reminder, date: &str) -> Option<i64> {
    match &r.due_time {
        Some(t) => local_to_utc(clock, &r.timezone, date, minute_of(t)),
        None => local_day_start(clock, r, &validate::add_days(date, 1)),
    }
}

pub fn alert_at(clock: &impl ZoneClock, r: &Reminder, alert: &Alert, date: &str) -> Option<i64> {
    match alert {
        Alert::Before { minutes } => {
            let anchor = r.due_time.as_deref().map_or(DAY_ANCHOR_MINUTE, minute_of);
            Some(local_to_utc(clock, &r.timezone, date, anchor)? - i64::from(*minutes) * MINUTE_MS)
        }
        Alert::At { time, days_before } => local_to_utc(
            clock,
            &r.timezone,
            &validate::add_days(date, -i64::from(*days_before)),
            minute_of(time),
        ),
    }
}

fn calendar(r: &Reminder) -> Option<&recurrence::Rule> {
    r.recurrence
        .as_ref()
        .filter(|_| r.advance == Advance::Calendar && r.due_date.is_some())
}

/// The next occurrence after `date` that isn't already completed.
pub fn next_open(r: &Reminder, date: &str) -> Option<String> {
    let rule = calendar(r)?;
    let mut d = date.to_owned();
    for _ in 0..MAX_STEPS {
        d = recurrence::next_after(rule, &d)?;
        if !r.completed_ahead.contains(&d) {
            return Some(d);
        }
    }
    None
}

/// Open occurrences from the current one on, through `last` (at most `max`).
pub fn occurrences(r: &Reminder, last: &str, max: usize) -> Vec<String> {
    let Some(cursor) = r.due_date.clone() else {
        return Vec::new();
    };
    if r.completed_at.is_some() || r.deleted_at.is_some() {
        return Vec::new();
    }
    let mut out = vec![cursor.clone()];
    if calendar(r).is_some() {
        let mut d = cursor;
        while out.len() < max {
            match next_open(r, &d) {
                Some(n) if n.as_str() <= last => {
                    out.push(n.clone());
                    d = n;
                }
                _ => break,
            }
        }
    }
    out
}

/// Apply the missed-occurrence policy at `now`. Returns the occurrence
/// dates recorded as missed; the cursor moves past them.
pub fn settle(clock: &impl ZoneClock, r: &mut Reminder, now: i64) -> Vec<String> {
    let mut missed = Vec::new();
    if r.completed_at.is_some() || r.deleted_at.is_some() || calendar(r).is_none() {
        return missed;
    }
    for _ in 0..MAX_STEPS {
        let Some(cursor) = r.due_date.clone() else {
            break;
        };
        match r.missed {
            Missed::Each => break,
            Missed::One => match next_open(r, &cursor) {
                Some(n) if start_at(clock, r, &n).is_some_and(|t| t <= now) => {
                    missed.push(cursor);
                    r.due_date = Some(n);
                }
                _ => break,
            },
            Missed::Skip => {
                let ends = local_day_start(clock, r, &validate::add_days(&cursor, 1));
                if !ends.is_some_and(|t| t <= now) {
                    break;
                }
                missed.push(cursor.clone());
                match next_open(r, &cursor) {
                    Some(n) => r.due_date = Some(n),
                    None => {
                        r.completed_at = Some(now);
                        break;
                    }
                }
            }
        }
    }
    if !missed.is_empty() {
        r.snoozed_until = None;
        prune_ahead(r);
    }
    missed
}

pub fn prune_ahead(r: &mut Reminder) {
    let cursor = r.due_date.clone().unwrap_or_default();
    r.completed_ahead.retain(|d| *d > cursor);
}

/// Alerts due in `(after, through]`, oldest first.
pub fn fires(clock: &impl ZoneClock, r: &Reminder, after: i64, through: i64) -> Vec<Fire> {
    let mut out = Vec::new();
    if r.completed_at.is_some()
        || r.deleted_at.is_some()
        || r.due_date.is_none()
        || after >= through
    {
        return out;
    }
    let in_window = |t: i64| t > after && t <= through;
    if let (Some(until), Some(d)) = (r.snoozed_until, &r.due_date) {
        if in_window(until) {
            out.push(Fire {
                occurrence: d.clone(),
                slot: format!("s{until}"),
                at: until,
            });
        }
    }
    // An occurrence's alerts fall between 28 days before its date and the
    // end of it, so only dates near the window can have one inside it.
    let (Some((from, _)), Some((to, _))) = (
        local_at(clock, &r.timezone, after),
        local_at(clock, &r.timezone, through),
    ) else {
        return out;
    };
    let first = validate::add_days(&from, -1);
    let last = validate::add_days(&to, i64::from(super::model::MAX_LEAD_DAYS) + 1);
    for d in occurrences(r, &last, 400) {
        if d < first {
            continue;
        }
        for a in &r.alerts {
            if let Some(at) = alert_at(clock, r, a, &d).filter(|t| in_window(*t)) {
                out.push(Fire {
                    occurrence: d.clone(),
                    slot: a.slot(),
                    at,
                });
            }
        }
    }
    out.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.slot.cmp(&b.slot)));
    out
}

/// When the scheduler should next look at the reminder: its next alert or
/// snooze, or the moment the missed-occurrence policy moves the cursor.
pub fn next_wake(clock: &impl ZoneClock, r: &Reminder, now: i64) -> Option<i64> {
    if r.completed_at.is_some() || r.deleted_at.is_some() {
        return None;
    }
    let cursor = r.due_date.as_deref()?;
    let mut best = fires(clock, r, now, now + LOOKAHEAD_MS)
        .first()
        .map(|f| f.at);
    let mut consider = |t: Option<i64>| {
        if let Some(t) = t.filter(|t| *t > now) {
            best = Some(best.map_or(t, |b| b.min(t)));
        }
    };
    if calendar(r).is_some() {
        match r.missed {
            Missed::Each => {}
            Missed::One => consider(next_open(r, cursor).and_then(|n| start_at(clock, r, &n))),
            Missed::Skip => consider(local_day_start(clock, r, &validate::add_days(cursor, 1))),
        }
    }
    consider(r.snoozed_until);
    // Something may fire beyond the lookahead: check back in a while.
    let later = |a: &Alert| alert_at(clock, r, a, cursor).is_some_and(|t| t > now);
    let more = calendar(r).is_some() && !r.alerts.is_empty() || r.alerts.iter().any(later);
    if best.is_none() && more {
        best = Some(now + MAX_SLEEP_MS);
    }
    best.map(|b| b.min(now + MAX_SLEEP_MS))
}

/// Recompute the derived fields after any change.
pub fn refresh(clock: &impl ZoneClock, r: &mut Reminder, now: i64) {
    prune_ahead(r);
    r.due_at = r.due_date.as_deref().and_then(|d| overdue_at(clock, r, d));
    r.backlog = match (r.missed, r.due_date.as_deref()) {
        (Missed::Each, Some(cursor)) if calendar(r).is_some() && r.completed_at.is_none() => {
            let today = local_at(clock, &r.timezone, now)
                .map(|(d, _)| d)
                .unwrap_or_default();
            occurrences(r, &today, 1_000)
                .iter()
                .filter(|d| {
                    d.as_str() > cursor && overdue_at(clock, r, d).is_some_and(|t| t <= now)
                })
                .count() as u32
        }
        _ => 0,
    };
    r.wake_at = next_wake(clock, r, now);
}

/// One scheduler visit at `now`: settle missed occurrences, pick the alert
/// to send (only the newest of any that came due together; the rest are
/// superseded), and move the reminder's high-water mark to `now`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scan {
    pub missed: Vec<String>,
    pub send: Option<Fire>,
    pub superseded: Vec<Fire>,
}

pub fn scan(clock: &impl ZoneClock, r: &mut Reminder, now: i64) -> Scan {
    let missed = settle(clock, r, now);
    let mut due = fires(clock, r, r.notified_through, now);
    let send = due.pop();
    r.notified_through = r.notified_through.max(now);
    refresh(clock, r, now);
    Scan {
        missed,
        send,
        superseded: due,
    }
}

/// The result of completing an occurrence (see [`complete`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completion {
    /// It was already completed; nothing changed.
    Already,
    /// `occurrence` (a date, or `-`) is now completed.
    Done { occurrence: String },
}

/// Complete `occurrence` (default: the current one), for occurrences from
/// the current one on. The caller handles dates before it, which live in
/// the occurrence history.
pub fn complete(
    clock: &impl ZoneClock,
    r: &mut Reminder,
    occurrence: Option<&str>,
    now: i64,
) -> Result<Completion, &'static str> {
    let Some(rule) = r.recurrence.clone().filter(|_| r.due_date.is_some()) else {
        if r.completed_at.is_some() {
            return Ok(Completion::Already);
        }
        r.completed_at = Some(now);
        r.snoozed_until = None;
        return Ok(Completion::Done {
            occurrence: r.due_date.clone().unwrap_or_else(|| "-".into()),
        });
    };
    let cursor = r.due_date.clone().unwrap_or_default();
    let d = occurrence.unwrap_or(&cursor).to_owned();
    if d == cursor && r.completed_at.is_some() {
        return Ok(Completion::Already);
    }
    if d > cursor {
        if r.completed_at.is_some() || r.completed_ahead.contains(&d) {
            return Ok(Completion::Already);
        }
        let today = local_at(clock, &r.timezone, now)
            .map(|(t, _)| t)
            .unwrap_or_default();
        let ok = r.missed == Missed::Each
            && calendar(r).is_some()
            && recurrence::matches(&rule, &d)
            && d <= today;
        if !ok {
            return Err("only the current occurrence can be completed");
        }
        if r.completed_ahead.len() >= MAX_AHEAD {
            return Err("complete the earlier occurrences first");
        }
        r.completed_ahead.push(d.clone());
        r.completed_ahead.sort();
        return Ok(Completion::Done { occurrence: d });
    }
    let next = match r.advance {
        Advance::Calendar => next_open(r, &cursor),
        Advance::Completion => {
            let today = local_at(clock, &r.timezone, now)
                .map(|(t, _)| t)
                .unwrap_or_else(|| cursor.clone());
            recurrence::after_completion(&rule, today.max(cursor.clone()).as_str())
        }
    };
    r.snoozed_until = None;
    match next {
        Some(n) => {
            if r.advance == Advance::Completion {
                if let Some(rule) = r.recurrence.as_mut() {
                    rule.anchor = n.clone();
                }
            }
            r.due_date = Some(n);
        }
        None => r.completed_at = Some(now),
    }
    Ok(Completion::Done { occurrence: d })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::model::{ChecklistItem, Reminder};
    use super::super::recurrence::{check, Freq, Rule};
    use super::super::zone::testing::FakeZones;
    use super::*;
    use crate::tasks::Priority;

    pub fn at(date: &str, minute: u32) -> i64 {
        local_to_utc(&FakeZones, "Test/Toronto", date, minute).unwrap()
    }

    pub fn reminder(due: Option<&str>, time: Option<&str>) -> Reminder {
        Reminder {
            id: "r".repeat(32),
            owner_id: "u".into(),
            title: "Water plants".into(),
            notes: String::new(),
            links: vec![],
            due_date: due.map(Into::into),
            due_time: time.map(Into::into),
            timezone: "Test/Toronto".into(),
            list_id: None,
            category_ids: vec![],
            priority: Priority::None,
            flagged: false,
            project_id: None,
            task_id: None,
            complete_with_task: false,
            alerts: super::super::model::default_alerts(time),
            recurrence: None,
            advance: Advance::Calendar,
            missed: Missed::One,
            checklist: Vec::<ChecklistItem>::new(),
            snoozed_until: None,
            completed_ahead: vec![],
            backlog: 0,
            completed_at: None,
            deleted_at: None,
            created_at: 0,
            updated_at: 0,
            version: 1,
            field_versions: Default::default(),
            due_at: None,
            notified_through: 0,
            wake_at: None,
        }
    }

    pub fn repeating(r: &mut Reminder, freq: Freq, interval: u32, weekdays: &[u32]) {
        let rule = Rule {
            freq,
            interval,
            weekdays: weekdays.to_vec(),
            until: None,
            anchor: String::new(),
        };
        r.recurrence = Some(check(rule, r.due_date.as_deref().unwrap()).unwrap());
    }

    #[test]
    fn unscheduled_reminders_never_fire() {
        let r = reminder(None, None);
        assert!(fires(&FakeZones, &r, 0, i64::MAX / 2).is_empty());
        assert_eq!(next_wake(&FakeZones, &r, 0), None);
    }

    #[test]
    fn alerts_follow_local_time_across_dst() {
        let mut r = reminder(Some("2026-10-31"), Some("20:00"));
        r.alerts = vec![
            Alert::Before { minutes: 0 },
            Alert::Before { minutes: 60 },
            Alert::At {
                time: "08:00".into(),
                days_before: 1,
            },
        ];
        repeating(&mut r, Freq::Daily, 1, &[]);
        let from = at("2026-10-30", 0);
        let got = fires(&FakeZones, &r, from, at("2026-11-02", 0));
        let times: Vec<(String, String)> = got
            .iter()
            .map(|f| (f.occurrence.clone(), f.slot.clone()))
            .collect();
        assert_eq!(times[0], ("2026-10-31".into(), "a1-0800".into()));
        // 20:00 local on both sides of the change: 00:00Z (EDT), then 01:00Z (EST).
        let due: Vec<i64> = got
            .iter()
            .filter(|f| f.slot == "b0")
            .map(|f| f.at)
            .collect();
        assert_eq!(due[0], validate::days_from_civil("2026-11-01") * DAY_MS);
        assert_eq!(
            due[1],
            validate::days_from_civil("2026-11-02") * DAY_MS + 3_600_000
        );
        assert_eq!(next_wake(&FakeZones, &r, from), Some(got[0].at));
    }

    #[test]
    fn date_only_reminders_alert_at_nine_and_are_overdue_after_the_day() {
        let r = reminder(Some("2026-11-03"), None);
        let f = fires(&FakeZones, &r, at("2026-11-01", 0), at("2026-11-05", 0));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].at, at("2026-11-03", 540));
        assert_eq!(
            overdue_at(&FakeZones, &r, "2026-11-03"),
            Some(at("2026-11-04", 0))
        );
    }

    #[test]
    fn carry_forward_one_records_the_missed_and_keeps_the_newest() {
        let mut r = reminder(Some("2026-11-02"), Some("08:00"));
        repeating(&mut r, Freq::Daily, 1, &[]);
        // Three days later, before that day's 08:00.
        let missed = settle(&FakeZones, &mut r, at("2026-11-05", 7 * 60));
        assert_eq!(missed, ["2026-11-02", "2026-11-03"]);
        assert_eq!(r.due_date.as_deref(), Some("2026-11-04"));
    }

    #[test]
    fn skip_moves_on_at_the_end_of_the_day_and_each_keeps_all() {
        let mut r = reminder(Some("2026-11-02"), Some("08:00"));
        repeating(&mut r, Freq::Daily, 1, &[]);
        r.missed = Missed::Skip;
        assert!(settle(&FakeZones, &mut r, at("2026-11-02", 23 * 60)).is_empty());
        assert_eq!(
            settle(&FakeZones, &mut r, at("2026-11-03", 9 * 60)),
            ["2026-11-02"]
        );
        assert_eq!(r.due_date.as_deref(), Some("2026-11-03"));

        let mut e = reminder(Some("2026-11-02"), Some("08:00"));
        repeating(&mut e, Freq::Daily, 1, &[]);
        e.missed = Missed::Each;
        let now = at("2026-11-05", 9 * 60);
        assert!(settle(&FakeZones, &mut e, now).is_empty());
        refresh(&FakeZones, &mut e, now);
        assert_eq!((e.due_date.as_deref(), e.backlog), (Some("2026-11-02"), 3));
        // Complete a later one out of order, then the current one: the cursor skips it.
        assert_eq!(
            complete(&FakeZones, &mut e, Some("2026-11-03"), now),
            Ok(Completion::Done {
                occurrence: "2026-11-03".into()
            })
        );
        assert_eq!(
            complete(&FakeZones, &mut e, Some("2026-11-03"), now),
            Ok(Completion::Already)
        );
        complete(&FakeZones, &mut e, None, now).unwrap();
        assert_eq!(e.due_date.as_deref(), Some("2026-11-04"));
        refresh(&FakeZones, &mut e, now);
        assert!(e.completed_ahead.is_empty());
        assert_eq!(e.backlog, 1);
        assert!(complete(&FakeZones, &mut e, Some("2026-11-09"), now).is_err());
    }

    #[test]
    fn completing_advances_by_calendar_or_from_completion() {
        let mut r = reminder(Some("2026-11-02"), Some("08:00"));
        repeating(&mut r, Freq::Weekly, 1, &[0, 2]);
        complete(&FakeZones, &mut r, None, at("2026-11-02", 9 * 60)).unwrap();
        assert_eq!(r.due_date.as_deref(), Some("2026-11-04"));

        let mut c = reminder(Some("2026-11-02"), None);
        repeating(&mut c, Freq::Daily, 3, &[]);
        c.advance = Advance::Completion;
        complete(&FakeZones, &mut c, None, at("2026-11-04", 600)).unwrap();
        assert_eq!(c.due_date.as_deref(), Some("2026-11-07"));
        assert_eq!(c.recurrence.as_ref().unwrap().anchor, "2026-11-07");
        // Only the current occurrence exists in completion mode.
        assert_eq!(occurrences(&c, "2026-12-31", 10), ["2026-11-07"]);

        let mut once = reminder(Some("2026-11-02"), None);
        assert_eq!(
            complete(&FakeZones, &mut once, None, 5),
            Ok(Completion::Done {
                occurrence: "2026-11-02".into()
            })
        );
        assert_eq!(
            complete(&FakeZones, &mut once, None, 6),
            Ok(Completion::Already)
        );
        assert_eq!(
            (once.completed_at, next_wake(&FakeZones, &once, 6)),
            (Some(5), None)
        );
    }

    #[test]
    fn the_series_ends_at_until() {
        let mut r = reminder(Some("2026-11-02"), None);
        repeating(&mut r, Freq::Daily, 1, &[]);
        r.recurrence.as_mut().unwrap().until = Some("2026-11-03".into());
        complete(&FakeZones, &mut r, None, 1).unwrap();
        complete(&FakeZones, &mut r, None, 2).unwrap();
        assert_eq!(
            (r.due_date.as_deref(), r.completed_at),
            (Some("2026-11-03"), Some(2))
        );
        assert_eq!(
            complete(&FakeZones, &mut r, None, 3),
            Ok(Completion::Already)
        );
    }

    #[test]
    fn scan_sends_the_newest_due_alert_once_and_snooze_fires_once() {
        let mut r = reminder(Some("2026-11-02"), Some("08:00"));
        r.alerts = vec![Alert::Before { minutes: 30 }, Alert::Before { minutes: 0 }];
        r.notified_through = at("2026-11-01", 0);
        let start = r.notified_through;
        refresh(&FakeZones, &mut r, start);
        assert_eq!(r.wake_at, Some(at("2026-11-02", 450)));
        // The scheduler was late: both alerts are due; only the newest is sent.
        let s = scan(&FakeZones, &mut r, at("2026-11-02", 481));
        assert_eq!(s.send.as_ref().map(|f| f.slot.as_str()), Some("b0"));
        assert_eq!(s.superseded.len(), 1);
        assert_eq!(r.wake_at, None);
        assert_eq!(
            scan(&FakeZones, &mut r, at("2026-11-02", 482)),
            Scan::default()
        );
        // Snoozing re-alerts this occurrence only.
        r.snoozed_until = Some(at("2026-11-02", 600));
        refresh(&FakeZones, &mut r, at("2026-11-02", 482));
        assert_eq!(r.wake_at, r.snoozed_until);
        let s = scan(&FakeZones, &mut r, at("2026-11-02", 601));
        assert_eq!(s.send.unwrap().slot, format!("s{}", at("2026-11-02", 600)));
        assert!(r.due_at.is_some_and(|t| t <= at("2026-11-02", 601)));
    }

    #[test]
    fn far_future_reminders_check_back_within_the_sleep_limit() {
        let mut r = reminder(Some("2027-10-01"), None);
        repeating(&mut r, Freq::Yearly, 1, &[]);
        let now = at("2026-11-01", 0);
        assert_eq!(next_wake(&FakeZones, &r, now), Some(now + MAX_SLEEP_MS));
        r.alerts.clear();
        r.missed = Missed::Each;
        assert_eq!(next_wake(&FakeZones, &r, now), None);
    }
}
