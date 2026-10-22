//! Calendar events: single or multi-day, all-day or timed, optionally
//! repeating. Dates and times are the owner's local wall-clock values.

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
}

fn backend(e: String) -> CalendarError {
    CalendarError::Backend(e)
}

fn time(raw: &str, field: &'static str) -> Result<String, CalendarError> {
    let b = raw.as_bytes();
    let ok = b.len() == 5
        && b[2] == b':'
        && raw[..2].parse::<u8>().is_ok_and(|h| h < 24)
        && raw[3..].parse::<u8>().is_ok_and(|m| m < 60)
        && b[..2].iter().chain(&b[3..]).all(u8::is_ascii_digit);
    if ok {
        Ok(raw.to_owned())
    } else {
        Err(CalendarError::Invalid(field, "expected HH:MM"))
    }
}

fn date(raw: &str, field: &'static str) -> Result<String, CalendarError> {
    validate::date(raw).map_err(|w| CalendarError::Invalid(field, w))
}

/// Normalize and check an event's fields (shared by create and update).
fn check(mut e: Event) -> Result<Event, CalendarError> {
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
    if e.repeat == Repeat::None {
        e.repeat_until = None;
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
        if d >= earliest && d <= last && d >= e.start_date {
            out.push(d);
        }
    };
    let start = validate::days_from_civil(&e.start_date);
    match e.repeat {
        Repeat::None => push(e.start_date.clone()),
        Repeat::Daily | Repeat::Weekly => {
            let step = if e.repeat == Repeat::Daily { 1 } else { 7 };
            // Jump straight to the first step at or after `earliest`.
            let skip = (validate::days_from_civil(&earliest) - start).max(0);
            let mut n = start + (skip + step - 1) / step * step;
            while n <= validate::days_from_civil(&last) {
                push(validate::civil_from_days(n));
                n += step;
            }
        }
        Repeat::Monthly | Repeat::Yearly => {
            let (sy, sm, sd) = ymd(&e.start_date);
            let (ey, em, _) = ymd(&earliest);
            let (ly, lm, _) = ymd(&last);
            let months = |y: i64, m: i64| y * 12 + (m - 1);
            let first = months(ey, em).max(months(sy, sm));
            for k in first..=months(ly, lm) {
                let (y, m) = (k.div_euclid(12), k.rem_euclid(12) + 1);
                if e.repeat == Repeat::Yearly && m != sm {
                    continue;
                }
                if let Some(d) = real_date(y, m, sd) {
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

    pub async fn create(&self, p: &Principal, input: NewEvent) -> Result<Event, CalendarError> {
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
            created_at: self.now,
            updated_at: self.now,
        })?;
        if self.repo.count(&p.user_id).await.map_err(backend)? >= MAX_EVENTS {
            return Err(CalendarError::Invalid(
                "events",
                "limit of 2,000 events reached; delete some first",
            ));
        }
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
        }
        e.updated_at = self.now;
        let e = check(e)?;
        if !self.repo.update(&e).await.map_err(backend)? {
            return Err(CalendarError::NotFound);
        }
        Ok(e)
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
}
