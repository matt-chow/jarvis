//! Calendar events: single or multi-day, all-day or timed, optionally
//! repeating. Dates and times are wall-clock values in the event's time zone
//! (the owner's local zone when none is recorded; DECISIONS D-021, D-047).

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 200;
pub const MAX_LOCATION: usize = 200;
pub const MAX_NOTES: usize = 5_000;
pub const MAX_EVENTS: u32 = 2_000;
/// Longest single occurrence.
pub const MAX_SPAN_DAYS: i64 = 366;
/// Longest range `occurrences` expands.
pub const MAX_RANGE_DAYS: i64 = 400;
/// "Every N days/weeks/…": the largest N.
pub const MAX_INTERVAL: u32 = 99;
/// Most occurrences a counted series can have.
pub const MAX_COUNT: u32 = 500;
/// Most skipped dates one series keeps.
pub const MAX_SKIPS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Repeat {
    #[default]
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Repeat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "daily" => Self::Daily,
            "weekly" => Self::Weekly,
            "monthly" => Self::Monthly,
            "yearly" => Self::Yearly,
            _ => return None,
        })
    }
}

const fn yes() -> bool {
    true
}

const fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub title: String,
    pub start_date: String,
    pub end_date: String,
    /// `HH:MM`; `None` = all day.
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub location: String,
    pub notes: String,
    pub repeat: Repeat,
    pub repeat_until: Option<String>,
    /// A fixed commitment (never moved by suggestions) or flexible time.
    /// Older exports have no field: they were all commitments.
    #[serde(default = "yes")]
    pub fixed: bool,
    /// IANA zone the times are in; `None` = the viewer's zone.
    #[serde(default)]
    pub timezone: Option<String>,
    /// Every N days, weeks, months, or years.
    #[serde(default = "one")]
    pub repeat_interval: u32,
    /// Stop after this many occurrences (sets `repeat_until`).
    #[serde(default)]
    pub repeat_count: Option<u32>,
    /// Occurrence start dates that were cancelled or moved out of the series.
    #[serde(default)]
    pub skip_dates: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One occurrence of an event within a range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Occurrence {
    pub event_id: String,
    pub title: String,
    pub start_date: String,
    pub end_date: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub fixed: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewEvent {
    /// Client-chosen id (offline create); must be a fresh 32-hex id.
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    pub start_date: String,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub repeat: Option<Repeat>,
    #[serde(default)]
    pub repeat_until: Option<String>,
    /// Default: fixed.
    #[serde(default)]
    pub fixed: Option<bool>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub repeat_interval: Option<u32>,
    #[serde(default)]
    pub repeat_count: Option<u32>,
}

/// `null` clears a nullable field; an omitted field is unchanged.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EventPatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub start_time: Option<Option<String>>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub end_time: Option<Option<String>>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub repeat: Option<Repeat>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub repeat_until: Option<Option<String>>,
    #[serde(default)]
    pub fixed: Option<bool>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub timezone: Option<Option<String>>,
    #[serde(default)]
    pub repeat_interval: Option<u32>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub repeat_count: Option<Option<u32>>,
    /// Replaces the skipped dates (e.g. to restore a cancelled occurrence).
    #[serde(default)]
    pub skip_dates: Option<Vec<String>>,
}

/// Which occurrences of a repeating event a change applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitScope {
    /// Just the occurrence on `date`.
    One,
    /// The occurrence on `date` and every later one.
    Following,
}

/// `POST /events/{id}/split`: cancel or replace part of a series, atomically.
/// Without a replacement, `one` skips the occurrence and `following` ends
/// the series before `date`. With one, the replacement is created as well
/// (a one-off event for `one`, a new series for `following`).
#[derive(Debug, Clone, Deserialize)]
pub struct Split {
    pub date: String,
    pub scope: SplitScope,
    #[serde(default)]
    pub replacement: Option<NewEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SplitResult {
    pub series: Event,
    pub replacement: Option<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarError {
    Invalid(&'static str, &'static str),
    /// A client-supplied id is already in use (a replayed offline create).
    Exists,
    NotFound,
    Backend(String),
}

pub trait EventRepository {
    /// All the owner's events, by start date.
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Event>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Event>, String>;
    async fn insert(&self, event: &Event) -> Result<(), String>;
    /// How many the owner has (create checks the cap; DECISIONS D-022).
    async fn count(&self, owner_id: &str) -> Result<u32, String>;
    async fn update(&self, event: &Event) -> Result<bool, String>;
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    /// Update `series` and insert `replacement` in one transaction.
    async fn split(&self, series: &Event, replacement: Option<&Event>) -> Result<bool, String>;
}

fn backend(e: String) -> CalendarError {
    CalendarError::Backend(e)
}

fn time(raw: &str, field: &'static str) -> Result<String, CalendarError> {
    validate::clock(raw, false)
        .map(|_| raw.to_owned())
        .map_err(|w| CalendarError::Invalid(field, w))
}

fn date(raw: &str, field: &'static str) -> Result<String, CalendarError> {
    validate::date(raw).map_err(|w| CalendarError::Invalid(field, w))
}

/// Normalize and check an event's fields (shared by create, update, and import).
pub(crate) fn check(mut e: Event) -> Result<Event, CalendarError> {
    e.title =
        validate::line(&e.title, MAX_TITLE).map_err(|w| CalendarError::Invalid("title", w))?;
    e.start_date = date(&e.start_date, "start_date")?;
    e.end_date = date(&e.end_date, "end_date")?;
    let span = validate::days_from_civil(&e.end_date) - validate::days_from_civil(&e.start_date);
    if span < 0 {
        return Err(CalendarError::Invalid("end_date", "before start_date"));
    }
    if span >= MAX_SPAN_DAYS {
        return Err(CalendarError::Invalid("end_date", "event is too long"));
    }
    e.start_time = e
        .start_time
        .as_deref()
        .map(|t| time(t, "start_time"))
        .transpose()?;
    e.end_time = e
        .end_time
        .as_deref()
        .map(|t| time(t, "end_time"))
        .transpose()?;
    match (&e.start_time, &e.end_time) {
        (None, Some(_)) => return Err(CalendarError::Invalid("end_time", "needs a start_time")),
        (Some(s), Some(t)) if span == 0 && t < s => {
            return Err(CalendarError::Invalid("end_time", "before start_time"))
        }
        _ => {}
    }
    e.location = validate::text(&e.location, MAX_LOCATION)
        .map_err(|w| CalendarError::Invalid("location", w))?
        .trim()
        .to_owned();
    e.notes =
        validate::text(&e.notes, MAX_NOTES).map_err(|w| CalendarError::Invalid("notes", w))?;
    e.timezone = e
        .timezone
        .as_deref()
        .map(validate::timezone)
        .transpose()
        .map_err(|w| CalendarError::Invalid("timezone", w))?;
    if !(1..=MAX_INTERVAL).contains(&e.repeat_interval) {
        return Err(CalendarError::Invalid("repeat_interval", "must be 1 to 99"));
    }
    if e.repeat == Repeat::None {
        e.repeat_until = None;
        e.repeat_count = None;
        e.repeat_interval = 1;
        e.skip_dates.clear();
    }
    if e.skip_dates.len() > MAX_SKIPS {
        return Err(CalendarError::Invalid("skip_dates", "too many"));
    }
    for d in &e.skip_dates {
        date(d, "skip_dates")?;
    }
    e.skip_dates.sort();
    e.skip_dates.dedup();
    if let Some(n) = e.repeat_count {
        if !(1..=MAX_COUNT).contains(&n) {
            return Err(CalendarError::Invalid("repeat_count", "must be 1 to 500"));
        }
        // The count decides the last date (a later start moves it too).
        e.repeat_until = nth_start(&e, n);
    }
    if let Some(u) = &e.repeat_until {
        let u = date(u, "repeat_until")?;
        if u < e.start_date {
            return Err(CalendarError::Invalid("repeat_until", "before start_date"));
        }
        e.repeat_until = Some(u);
    }
    Ok(e)
}

fn ymd(d: &str) -> (i64, i64, i64) {
    let n = |r: std::ops::Range<usize>| d[r].parse::<i64>().unwrap_or(0);
    (n(0..4), n(5..7), n(8..10))
}

/// `y-m-d` if that day exists (e.g. no 31st in April, no Feb 29 off leap years).
fn real_date(y: i64, m: i64, d: i64) -> Option<String> {
    validate::date(&format!("{y:04}-{m:02}-{d:02}")).ok()
}

/// Months since year 0 (for month arithmetic).
fn months(y: i64, m: i64) -> i64 {
    y * 12 + (m - 1)
}

/// The start of the `k`-th repeat period (0 = the first), if that day exists.
fn period_start(e: &Event, k: i64) -> Option<String> {
    let n = i64::from(e.repeat_interval.max(1));
    match e.repeat {
        Repeat::None => (k == 0).then(|| e.start_date.clone()),
        Repeat::Daily | Repeat::Weekly => {
            let step = if e.repeat == Repeat::Daily { n } else { 7 * n };
            let d = validate::add_days(&e.start_date, k * step);
            (d.len() == 10).then_some(d)
        }
        Repeat::Monthly | Repeat::Yearly => {
            let (sy, sm, sd) = ymd(&e.start_date);
            let per = if e.repeat == Repeat::Monthly {
                n
            } else {
                12 * n
            };
            let m = months(sy, sm) + k * per;
            real_date(m.div_euclid(12), m.rem_euclid(12) + 1, sd)
        }
    }
}

/// Start date of the `n`-th occurrence (1-based), skipped dates included:
/// skipping an occurrence uses up one of the count.
pub fn nth_start(e: &Event, n: u32) -> Option<String> {
    let mut found = 0;
    // A yearly Feb 29 exists at most every 8 years; nothing needs more.
    for k in 0..i64::from(n) * 8 + 8 {
        if let Some(d) = period_start(e, k) {
            if d.as_str() > "9999-12-31" {
                return None;
            }
            found += 1;
            if found == n {
                return Some(d);
            }
        } else if e.repeat == Repeat::Daily || e.repeat == Repeat::Weekly {
            return None;
        }
    }
    None
}

/// Start dates of an event's occurrences that overlap `from..=to`.
pub fn occurrence_starts(e: &Event, from: &str, to: &str) -> Vec<String> {
    let span = validate::days_from_civil(&e.end_date) - validate::days_from_civil(&e.start_date);
    // An occurrence starting this early still overlaps `from`.
    let earliest = validate::add_days(from, -span);
    let last = match &e.repeat_until {
        Some(u) if u.as_str() < to => u.clone(),
        _ => to.to_owned(),
    };
    let mut out = Vec::new();
    let mut push = |d: String| {
        if d >= earliest
            && d <= last
            && d >= e.start_date
            && e.skip_dates.binary_search(&d).is_err()
        {
            out.push(d);
        }
    };
    let start = validate::days_from_civil(&e.start_date);
    let n = i64::from(e.repeat_interval.max(1));
    match e.repeat {
        Repeat::None => push(e.start_date.clone()),
        Repeat::Daily | Repeat::Weekly => {
            let step = if e.repeat == Repeat::Daily { n } else { 7 * n };
            // Jump straight to the first step at or after `earliest`.
            let skip = (validate::days_from_civil(&earliest) - start).max(0);
            let mut d = start + (skip + step - 1) / step * step;
            while d <= validate::days_from_civil(&last) {
                push(validate::civil_from_days(d));
                d += step;
            }
        }
        Repeat::Monthly | Repeat::Yearly => {
            let (sy, sm, sd) = ymd(&e.start_date);
            let (ey, em, _) = ymd(&earliest);
            let (ly, lm, _) = ymd(&last);
            let per = if e.repeat == Repeat::Monthly {
                n
            } else {
                12 * n
            };
            let base = months(sy, sm);
            for k in months(ey, em).max(base)..=months(ly, lm) {
                if (k - base) % per != 0 {
                    continue;
                }
                if let Some(d) = real_date(k.div_euclid(12), k.rem_euclid(12) + 1, sd) {
                    push(d);
                }
            }
        }
    }
    out
}

/// All occurrences overlapping `from..=to`, by start date then time.
pub fn occurrences(events: &[Event], from: &str, to: &str) -> Vec<Occurrence> {
    let mut out: Vec<Occurrence> = events
        .iter()
        .flat_map(|e| {
            let span =
                validate::days_from_civil(&e.end_date) - validate::days_from_civil(&e.start_date);
            occurrence_starts(e, from, to)
                .into_iter()
                .map(move |start| Occurrence {
                    event_id: e.id.clone(),
                    title: e.title.clone(),
                    end_date: validate::add_days(&start, span),
                    start_date: start,
                    start_time: e.start_time.clone(),
                    end_time: e.end_time.clone(),
                    fixed: e.fixed,
                })
        })
        .collect();
    out.sort_by(|a, b| {
        a.start_date
            .cmp(&b.start_date)
            .then(a.start_time.is_some().cmp(&b.start_time.is_some()))
            .then(a.start_time.cmp(&b.start_time))
    });
    out
}

pub struct CalendarService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: EventRepository, E: Entropy + ?Sized> CalendarService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    pub async fn list(&self, p: &Principal) -> Result<Vec<Event>, CalendarError> {
        self.repo
            .list(&p.user_id, MAX_EVENTS)
            .await
            .map_err(backend)
    }

    /// Occurrences overlapping `from..=to` (at most `MAX_RANGE_DAYS`).
    pub async fn agenda(
        &self,
        p: &Principal,
        from: &str,
        to: &str,
    ) -> Result<Vec<Occurrence>, CalendarError> {
        let (from, to) = (date(from, "from")?, date(to, "to")?);
        let days = validate::days_from_civil(&to) - validate::days_from_civil(&from);
        if !(0..MAX_RANGE_DAYS).contains(&days) {
            return Err(CalendarError::Invalid("to", "range must be 1 to 400 days"));
        }
        Ok(occurrences(&self.list(p).await?, &from, &to))
    }

    pub async fn get(&self, p: &Principal, id: &str) -> Result<Event, CalendarError> {
        if !is_id(id) {
            return Err(CalendarError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(CalendarError::NotFound)
    }

    /// A new, checked event (not saved), with its id checked for replays.
    async fn build(&self, p: &Principal, input: NewEvent) -> Result<Event, CalendarError> {
        let id = match input.id {
            Some(id) if !is_id(&id) => {
                return Err(CalendarError::Invalid("id", "expected 32 hex characters"))
            }
            Some(id)
                if self
                    .repo
                    .get(&p.user_id, &id)
                    .await
                    .map_err(backend)?
                    .is_some() =>
            {
                return Err(CalendarError::Exists)
            }
            Some(id) => id,
            None => new_id(self.entropy),
        };
        let event = check(Event {
            id,
            owner_id: p.user_id.clone(),
            title: input.title,
            end_date: input.end_date.unwrap_or_else(|| input.start_date.clone()),
            start_date: input.start_date,
            start_time: input.start_time,
            end_time: input.end_time,
            location: input.location.unwrap_or_default(),
            notes: input.notes.unwrap_or_default(),
            repeat: input.repeat.unwrap_or_default(),
            repeat_until: input.repeat_until,
            fixed: input.fixed.unwrap_or(true),
            timezone: input.timezone,
            repeat_interval: input.repeat_interval.unwrap_or(1),
            repeat_count: input.repeat_count,
            skip_dates: Vec::new(),
            created_at: self.now,
            updated_at: self.now,
        })?;
        if self.repo.count(&p.user_id).await.map_err(backend)? >= MAX_EVENTS {
            return Err(CalendarError::Invalid(
                "events",
                "limit of 2,000 events reached; delete some first",
            ));
        }
        Ok(event)
    }

    pub async fn create(&self, p: &Principal, input: NewEvent) -> Result<Event, CalendarError> {
        let event = self.build(p, input).await?;
        self.repo.insert(&event).await.map_err(backend)?;
        Ok(event)
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: EventPatch,
    ) -> Result<Event, CalendarError> {
        let mut e = self.get(p, id).await?;
        // Moving the start keeps the duration unless an end is given too.
        if let Some(start) = patch.start_date {
            let span =
                validate::days_from_civil(&e.end_date) - validate::days_from_civil(&e.start_date);
            let start = date(&start, "start_date")?;
            e.end_date = validate::add_days(&start, span);
            e.start_date = start;
        }
        if let Some(v) = patch.end_date {
            e.end_date = v;
        }
        if let Some(v) = patch.title {
            e.title = v;
        }
        if let Some(v) = patch.start_time {
            e.start_time = v;
        }
        if let Some(v) = patch.end_time {
            e.end_time = v;
        }
        if e.start_time.is_none() {
            e.end_time = None;
        }
        if let Some(v) = patch.location {
            e.location = v;
        }
        if let Some(v) = patch.notes {
            e.notes = v;
        }
        if let Some(v) = patch.repeat {
            e.repeat = v;
        }
        if let Some(v) = patch.repeat_until {
            e.repeat_until = v;
            // An explicit end date replaces a count.
            e.repeat_count = None;
        }
        if let Some(v) = patch.fixed {
            e.fixed = v;
        }
        if let Some(v) = patch.timezone {
            e.timezone = v;
        }
        if let Some(v) = patch.repeat_interval {
            e.repeat_interval = v;
        }
        if let Some(v) = patch.repeat_count {
            e.repeat_count = v;
        }
        if let Some(v) = patch.skip_dates {
            e.skip_dates = v;
        }
        e.updated_at = self.now;
        let e = check(e)?;
        if !self.repo.update(&e).await.map_err(backend)? {
            return Err(CalendarError::NotFound);
        }
        Ok(e)
    }

    /// Change part of a repeating series (see [`Split`]). Safe to replay:
    /// a replacement that already exists answers `Exists`, and skipping or
    /// ending again changes nothing.
    pub async fn split(
        &self,
        p: &Principal,
        id: &str,
        input: Split,
    ) -> Result<SplitResult, CalendarError> {
        let mut series = self.get(p, id).await?;
        if series.repeat == Repeat::None {
            return Err(CalendarError::Invalid("id", "not a repeating event"));
        }
        let day = date(&input.date, "date")?;
        let replacement = match input.replacement {
            Some(r) => Some(self.build(p, r).await?),
            None => None,
        };
        match input.scope {
            SplitScope::One => {
                if series.skip_dates.binary_search(&day).is_err() {
                    if !occurrence_starts(&series, &day, &day).contains(&day) {
                        return Err(CalendarError::Invalid("date", "not an occurrence"));
                    }
                    series.skip_dates.push(day);
                }
            }
            SplitScope::Following => {
                if day <= series.start_date {
                    return Err(CalendarError::Invalid(
                        "date",
                        "that is the first occurrence; change the whole series",
                    ));
                }
                let before = validate::add_days(&day, -1);
                if series.repeat_until.as_ref().is_none_or(|u| *u > before) {
                    series.repeat_until = Some(before);
                    series.repeat_count = None;
                }
            }
        }
        series.updated_at = self.now;
        let series = check(series)?;
        if !self
            .repo
            .split(&series, replacement.as_ref())
            .await
            .map_err(backend)?
        {
            return Err(CalendarError::NotFound);
        }
        Ok(SplitResult {
            series,
            replacement,
        })
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), CalendarError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(CalendarError::NotFound)
        }
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct MemoryEvents(pub RefCell<Vec<Event>>);

    impl EventRepository for MemoryEvents {
        async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Event>, String> {
            let mut all: Vec<Event> = self
                .0
                .borrow()
                .iter()
                .filter(|e| e.owner_id == owner_id)
                .cloned()
                .collect();
            all.sort_by(|a, b| a.start_date.cmp(&b.start_date));
            all.truncate(limit as usize);
            Ok(all)
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Event>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|e| e.owner_id == owner_id && e.id == id)
                .cloned())
        }
        async fn count(&self, owner_id: &str) -> Result<u32, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|x| x.owner_id == owner_id)
                .count() as u32)
        }
        async fn insert(&self, event: &Event) -> Result<(), String> {
            self.0.borrow_mut().push(event.clone());
            Ok(())
        }
        async fn update(&self, event: &Event) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            match all
                .iter_mut()
                .find(|e| e.owner_id == event.owner_id && e.id == event.id)
            {
                Some(e) => {
                    *e = event.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|e| !(e.owner_id == owner_id && e.id == id));
            Ok(all.len() < before)
        }
        async fn split(&self, series: &Event, replacement: Option<&Event>) -> Result<bool, String> {
            if !self.update(series).await? {
                return Ok(false);
            }
            if let Some(r) = replacement {
                self.insert(r).await?;
            }
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryEvents;
    use super::*;
    use crate::entropy::testing::CountingEntropy;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;

    fn owner(id: &str) -> Principal {
        Principal {
            user_id: id.into(),
            method: AuthMethod::Session,
        }
    }

    fn ev(start: &str, end: &str, repeat: Repeat, until: Option<&str>) -> Event {
        Event {
            id: "e".into(),
            owner_id: "u".into(),
            title: "T".into(),
            start_date: start.into(),
            end_date: end.into(),
            start_time: None,
            end_time: None,
            location: String::new(),
            notes: String::new(),
            repeat,
            repeat_until: until.map(Into::into),
            fixed: true,
            timezone: None,
            repeat_interval: 1,
            repeat_count: None,
            skip_dates: Vec::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn expands_repeats_within_a_range() {
        let starts = |e: &Event, from, to| occurrence_starts(e, from, to);
        let once = ev("2026-10-22", "2026-10-22", Repeat::None, None);
        assert_eq!(
            starts(&once, "2026-10-01", "2026-10-31"),
            vec!["2026-10-22"]
        );
        assert!(starts(&once, "2026-11-01", "2026-11-30").is_empty());

        // A multi-day event that started before the range still shows.
        let trip = ev("2026-09-29", "2026-10-02", Repeat::None, None);
        assert_eq!(
            starts(&trip, "2026-10-01", "2026-10-31"),
            vec!["2026-09-29"]
        );

        let weekly = ev(
            "2026-10-01",
            "2026-10-01",
            Repeat::Weekly,
            Some("2026-10-22"),
        );
        assert_eq!(
            starts(&weekly, "2026-10-10", "2026-10-31"),
            vec!["2026-10-15", "2026-10-22"]
        );
        let daily = ev("2026-10-30", "2026-10-30", Repeat::Daily, None);
        assert_eq!(
            starts(&daily, "2026-10-01", "2026-11-02"),
            vec!["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"]
        );

        // Monthly on the 31st skips short months; yearly on Feb 29 waits for leap years.
        let monthly = ev("2026-01-31", "2026-01-31", Repeat::Monthly, None);
        assert_eq!(
            starts(&monthly, "2026-02-01", "2026-05-31"),
            vec!["2026-03-31", "2026-05-31"]
        );
        let leap = ev("2024-02-29", "2024-02-29", Repeat::Yearly, None);
        assert!(starts(&leap, "2025-01-01", "2027-12-31").is_empty());
        assert_eq!(
            starts(&leap, "2028-01-01", "2028-12-31"),
            vec!["2028-02-29"]
        );
        let birthday = ev("1990-06-15", "1990-06-15", Repeat::Yearly, None);
        assert_eq!(
            starts(&birthday, "2026-01-01", "2026-12-31"),
            vec!["2026-06-15"]
        );
    }

    #[test]
    fn occurrences_sort_all_day_first_then_by_time() {
        let mut a = ev("2026-10-22", "2026-10-22", Repeat::None, None);
        a.start_time = Some("09:00".into());
        a.id = "a".into();
        let mut b = a.clone();
        b.start_time = None;
        b.id = "b".into();
        let mut c = a.clone();
        c.start_time = Some("08:30".into());
        c.id = "c".into();
        let ids: Vec<String> = occurrences(&[a, b, c], "2026-10-22", "2026-10-22")
            .into_iter()
            .map(|o| o.event_id)
            .collect();
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    #[test]
    fn create_update_validate_and_scope() {
        let repo = MemoryEvents::default();
        let e = CountingEntropy::default();
        let svc = CalendarService::new(&repo, &e, 5);
        let me = owner("u1");
        let new = |title: &str, start: &str| NewEvent {
            title: title.into(),
            start_date: start.into(),
            ..Default::default()
        };

        let dentist = block_on(svc.create(
            &me,
            NewEvent {
                start_time: Some("09:30".into()),
                end_time: Some("10:15".into()),
                location: Some(" Main St ".into()),
                ..new(" Dentist ", "2026-10-23")
            },
        ))
        .unwrap();
        assert_eq!(
            (
                dentist.title.as_str(),
                dentist.end_date.as_str(),
                dentist.location.as_str()
            ),
            ("Dentist", "2026-10-23", "Main St")
        );

        let bad = |input: NewEvent| block_on(svc.create(&me, input)).unwrap_err();
        assert_eq!(
            bad(NewEvent {
                end_date: Some("2026-10-01".into()),
                ..new("x", "2026-10-23")
            }),
            CalendarError::Invalid("end_date", "before start_date")
        );
        assert_eq!(
            bad(NewEvent {
                start_time: Some("25:00".into()),
                ..new("x", "2026-10-23")
            }),
            CalendarError::Invalid("start_time", "expected HH:MM")
        );
        assert_eq!(
            bad(NewEvent {
                end_time: Some("10:00".into()),
                ..new("x", "2026-10-23")
            }),
            CalendarError::Invalid("end_time", "needs a start_time")
        );
        assert_eq!(
            bad(NewEvent {
                start_time: Some("10:00".into()),
                end_time: Some("09:00".into()),
                ..new("x", "2026-10-23")
            }),
            CalendarError::Invalid("end_time", "before start_time")
        );

        // Moving the start keeps the length; clearing the start time clears the end.
        let trip = block_on(svc.create(
            &me,
            NewEvent {
                end_date: Some("2026-11-03".into()),
                ..new("Trip", "2026-11-01")
            },
        ))
        .unwrap();
        let moved = block_on(svc.update(
            &me,
            &trip.id,
            EventPatch {
                start_date: Some("2026-11-10".into()),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            (moved.start_date.as_str(), moved.end_date.as_str()),
            ("2026-11-10", "2026-11-12")
        );
        let all_day: EventPatch = serde_json::from_str(r#"{"start_time":null}"#).unwrap();
        let cleared = block_on(svc.update(&me, &dentist.id, all_day)).unwrap();
        assert_eq!((cleared.start_time, cleared.end_time), (None, None));

        let agenda = block_on(svc.agenda(&me, "2026-10-01", "2026-11-30")).unwrap();
        assert_eq!(agenda.len(), 2);
        assert_eq!(
            block_on(svc.agenda(&me, "2026-10-01", "2028-01-01")).unwrap_err(),
            CalendarError::Invalid("to", "range must be 1 to 400 days")
        );

        let other = owner("u2");
        assert!(block_on(svc.list(&other)).unwrap().is_empty());
        assert_eq!(
            block_on(svc.delete(&other, &trip.id)).unwrap_err(),
            CalendarError::NotFound
        );
        block_on(svc.delete(&me, &trip.id)).unwrap();

        let replay = || NewEvent {
            id: Some("c".repeat(32)),
            ..new("Call", "2026-10-24")
        };
        block_on(svc.create(&me, replay())).unwrap();
        assert_eq!(
            block_on(svc.create(&me, replay())).unwrap_err(),
            CalendarError::Exists
        );
    }

    #[test]
    fn intervals_counts_and_skips() {
        let starts = |e: &Event, from, to| occurrence_starts(e, from, to);
        // Every 2 weeks, from a Monday.
        let mut fortnight = ev("2026-10-05", "2026-10-05", Repeat::Weekly, None);
        fortnight.repeat_interval = 2;
        assert_eq!(
            starts(&fortnight, "2026-10-01", "2026-11-15"),
            vec!["2026-10-05", "2026-10-19", "2026-11-02"]
        );
        // Every 3 months on the 31st: Jan, (Apr has no 31st), Jul, Oct.
        let mut quarterly = ev("2026-01-31", "2026-01-31", Repeat::Monthly, None);
        quarterly.repeat_interval = 3;
        assert_eq!(
            starts(&quarterly, "2026-01-01", "2026-12-31"),
            vec!["2026-01-31", "2026-07-31", "2026-10-31"]
        );
        let mut biennial = ev("2026-03-01", "2026-03-01", Repeat::Yearly, None);
        biennial.repeat_interval = 2;
        assert_eq!(
            starts(&biennial, "2026-01-01", "2031-12-31"),
            vec!["2026-03-01", "2028-03-01", "2030-03-01"]
        );

        // A count sets the last date; skipped dates still use up the count.
        let mut five = ev("2026-10-26", "2026-10-26", Repeat::Daily, None);
        five.repeat_count = Some(5);
        five.skip_dates = vec!["2026-10-28".into()];
        let five = check(five).unwrap();
        assert_eq!(five.repeat_until.as_deref(), Some("2026-10-30"));
        assert_eq!(
            starts(&five, "2026-10-01", "2026-11-30"),
            vec!["2026-10-26", "2026-10-27", "2026-10-29", "2026-10-30"]
        );
        let mut leap = ev("2024-02-29", "2024-02-29", Repeat::Yearly, None);
        leap.repeat_count = Some(2);
        assert_eq!(
            check(leap).unwrap().repeat_until.as_deref(),
            Some("2028-02-29")
        );

        // Not repeating: repeat fields are cleared.
        let mut once = ev("2026-10-26", "2026-10-26", Repeat::None, None);
        once.repeat_count = Some(3);
        once.repeat_interval = 4;
        once.skip_dates = vec!["2026-10-26".into()];
        let once = check(once).unwrap();
        assert_eq!(
            (
                once.repeat_count,
                once.repeat_interval,
                once.skip_dates.len()
            ),
            (None, 1, 0)
        );
        let mut bad = ev("2026-10-26", "2026-10-26", Repeat::Daily, None);
        bad.repeat_interval = 0;
        assert!(check(bad.clone()).is_err());
        bad.repeat_interval = 1;
        bad.timezone = Some("not a zone".into());
        assert_eq!(
            check(bad).unwrap_err(),
            CalendarError::Invalid("timezone", "expected an IANA time zone")
        );
    }

    #[test]
    fn split_skips_detaches_and_ends_series_once() {
        let repo = MemoryEvents::default();
        let e = CountingEntropy::default();
        let svc = CalendarService::new(&repo, &e, 5);
        let me = owner("u1");
        let class = block_on(svc.create(
            &me,
            NewEvent {
                title: "Class".into(),
                start_date: "2026-10-05".into(),
                start_time: Some("10:00".into()),
                end_time: Some("11:00".into()),
                repeat: Some(Repeat::Weekly),
                timezone: Some("America/Toronto".into()),
                ..Default::default()
            },
        ))
        .unwrap();
        assert!(class.fixed, "events are commitments by default");
        let day = |d: &str, scope, replacement| Split {
            date: d.into(),
            scope,
            replacement,
        };
        let split = |d: &str, scope, replacement| {
            block_on(svc.split(&me, &class.id, day(d, scope, replacement)))
        };

        // Cancel one occurrence (twice: a replay changes nothing).
        for _ in 0..2 {
            let r = split("2026-10-12", SplitScope::One, None).unwrap();
            assert_eq!(r.series.skip_dates, vec!["2026-10-12"]);
        }
        assert_eq!(
            split("2026-10-13", SplitScope::One, None).unwrap_err(),
            CalendarError::Invalid("date", "not an occurrence")
        );

        // Move one occurrence: skipped in the series, plus a one-off event.
        let moved = || NewEvent {
            id: Some("d".repeat(32)),
            title: "Class (moved)".into(),
            start_date: "2026-10-20".into(),
            start_time: Some("14:00".into()),
            end_time: Some("15:00".into()),
            ..Default::default()
        };
        let r = split("2026-10-19", SplitScope::One, Some(moved())).unwrap();
        assert_eq!(r.series.skip_dates, vec!["2026-10-12", "2026-10-19"]);
        assert_eq!(r.replacement.unwrap().start_date, "2026-10-20");
        assert_eq!(
            split("2026-10-19", SplitScope::One, Some(moved())).unwrap_err(),
            CalendarError::Exists,
            "a replayed split doesn't create the replacement twice"
        );

        // This and following: the series ends the day before.
        let r = split("2026-11-02", SplitScope::Following, None).unwrap();
        assert_eq!(r.series.repeat_until.as_deref(), Some("2026-11-01"));
        let agenda = block_on(svc.agenda(&me, "2026-10-01", "2026-11-30")).unwrap();
        let dates: Vec<&str> = agenda.iter().map(|o| o.start_date.as_str()).collect();
        assert_eq!(dates, vec!["2026-10-05", "2026-10-20", "2026-10-26"]);
        assert_eq!(
            split("2026-10-05", SplitScope::Following, None).unwrap_err(),
            CalendarError::Invalid(
                "date",
                "that is the first occurrence; change the whole series"
            )
        );

        // Owner-scoped; one-off events can't be split.
        assert_eq!(
            block_on(svc.split(
                &owner("u2"),
                &class.id,
                day("2026-10-26", SplitScope::One, None)
            ))
            .unwrap_err(),
            CalendarError::NotFound
        );
        let once = block_on(svc.list(&me))
            .unwrap()
            .into_iter()
            .find(|x| x.repeat == Repeat::None)
            .unwrap();
        assert_eq!(
            block_on(svc.split(&me, &once.id, day("2026-10-20", SplitScope::One, None)))
                .unwrap_err(),
            CalendarError::Invalid("id", "not a repeating event")
        );
    }

    #[test]
    fn old_exports_default_to_fixed_unzoned_events() {
        let e: Event = serde_json::from_str(
            r#"{"id":"e","title":"T","start_date":"2026-10-22","end_date":"2026-10-22",
                "start_time":null,"end_time":null,"location":"","notes":"","repeat":"none",
                "repeat_until":null,"created_at":1,"updated_at":1}"#,
        )
        .unwrap();
        assert!(e.fixed);
        assert_eq!(
            (e.timezone, e.repeat_interval, e.skip_dates.len()),
            (None, 1, 0)
        );
    }
}
