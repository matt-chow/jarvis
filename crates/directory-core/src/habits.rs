//! Habits: named routines on chosen weekdays, with one check-in per day and a
//! current streak. Dates are the owner's local `YYYY-MM-DD`.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_NAME: usize = 120;
pub const MAX_HABITS: u32 = 200;
/// How far back check-ins are read to compute a streak.
pub const STREAK_LOOKBACK_DAYS: i64 = 366;
/// Longest check-in history a list request can ask for.
pub const MAX_WINDOW_DAYS: i64 = 366;
pub const DEFAULT_WINDOW_DAYS: i64 = 28;
const MS_PER_DAY: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Habit {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub name: String,
    /// Scheduled weekdays, 0 = Monday … 6 = Sunday, sorted and unique.
    pub days: Vec<u8>,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A habit as listed: recent check-ins and the current streak.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HabitView {
    #[serde(flatten)]
    pub habit: Habit,
    /// Checked dates within the requested window, oldest first.
    pub checkins: Vec<String>,
    pub streak: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckIn {
    pub habit_id: String,
    pub date: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewHabit {
    /// Client-chosen id (offline create); must be a fresh 32-hex id.
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    /// Defaults to every day.
    #[serde(default)]
    pub days: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HabitPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub days: Option<Vec<u8>>,
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HabitError {
    Invalid(&'static str, &'static str),
    /// A client-supplied id is already in use (a replayed offline create).
    Exists,
    NotFound,
    Backend(String),
}

pub trait HabitRepository {
    /// All habits (archived too), oldest first.
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Habit>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Habit>, String>;
    async fn insert(&self, habit: &Habit) -> Result<(), String>;
    /// How many the owner has (create checks the cap; DECISIONS D-022).
    async fn count(&self, owner_id: &str) -> Result<u32, String>;
    async fn update(&self, habit: &Habit) -> Result<bool, String>;
    /// Delete the habit and its check-ins.
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    /// Check-ins of all the owner's habits from `from` to `to` (inclusive).
    async fn checkins(&self, owner_id: &str, from: &str, to: &str) -> Result<Vec<CheckIn>, String>;
    /// Record a check-in; a repeat is a no-op.
    async fn check_in(
        &self,
        owner_id: &str,
        habit_id: &str,
        date: &str,
        now: i64,
    ) -> Result<(), String>;
    /// Remove a check-in; a missing one is a no-op.
    async fn uncheck(&self, owner_id: &str, habit_id: &str, date: &str) -> Result<(), String>;
}

/// Weekday list → bit mask (bit 0 = Monday).
pub fn days_to_mask(days: &[u8]) -> u8 {
    days.iter().fold(0, |m, d| m | (1 << d))
}

/// Bit mask → sorted weekday list.
pub fn mask_to_days(mask: u8) -> Vec<u8> {
    (0..7).filter(|d| mask & (1 << d) != 0).collect()
}

fn valid_days(days: Option<Vec<u8>>) -> Result<Vec<u8>, HabitError> {
    let Some(days) = days else {
        return Ok((0..7).collect());
    };
    if days.iter().any(|d| *d > 6) {
        return Err(HabitError::Invalid(
            "days",
            "expected weekdays 0 (Monday) to 6",
        ));
    }
    let days = mask_to_days(days_to_mask(&days));
    if days.is_empty() {
        return Err(HabitError::Invalid("days", "pick at least one day"));
    }
    Ok(days)
}

/// Consecutive check-ins ending today. Scheduled days without a check-in end
/// the streak, except today (it isn't over yet); unscheduled days are skipped,
/// and a check-in on one still counts.
pub fn streak(days: &[u8], checked: &HashSet<&str>, today: &str) -> u32 {
    let mut n = 0;
    for back in 0..STREAK_LOOKBACK_DAYS {
        let d = validate::add_days(today, -back);
        if checked.contains(d.as_str()) {
            n += 1;
        } else if back > 0 && days.contains(&(validate::weekday(&d) as u8)) {
            break;
        }
    }
    n
}

fn backend(e: String) -> HabitError {
    HabitError::Backend(e)
}

fn name(raw: &str) -> Result<String, HabitError> {
    validate::line(raw, MAX_NAME).map_err(|w| HabitError::Invalid("name", w))
}

pub struct HabitService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: HabitRepository, E: Entropy + ?Sized> HabitService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    /// Habits with check-ins from the last `window` days up to `today`, and streaks.
    pub async fn list(
        &self,
        p: &Principal,
        today: &str,
        window: i64,
    ) -> Result<Vec<HabitView>, HabitError> {
        let today = validate::date(today).map_err(|w| HabitError::Invalid("today", w))?;
        if !(1..=MAX_WINDOW_DAYS).contains(&window) {
            return Err(HabitError::Invalid("days", "expected 1 to 366"));
        }
        let habits = self
            .repo
            .list(&p.user_id, MAX_HABITS)
            .await
            .map_err(backend)?;
        let lookback = window.max(STREAK_LOOKBACK_DAYS);
        let from = validate::add_days(&today, -(lookback - 1));
        let mut by_habit: HashMap<String, Vec<String>> = HashMap::new();
        for c in self
            .repo
            .checkins(&p.user_id, &from, &today)
            .await
            .map_err(backend)?
        {
            by_habit.entry(c.habit_id).or_default().push(c.date);
        }
        let window_from = validate::add_days(&today, -(window - 1));
        Ok(habits
            .into_iter()
            .map(|habit| {
                let mut dates = by_habit.remove(&habit.id).unwrap_or_default();
                dates.sort();
                let checked: HashSet<&str> = dates.iter().map(String::as_str).collect();
                let streak = streak(&habit.days, &checked, &today);
                let checkins = dates
                    .iter()
                    .filter(|d| d.as_str() >= window_from.as_str())
                    .cloned()
                    .collect();
                HabitView {
                    habit,
                    checkins,
                    streak,
                }
            })
            .collect())
    }

    pub async fn get(&self, p: &Principal, id: &str) -> Result<Habit, HabitError> {
        if !is_id(id) {
            return Err(HabitError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(HabitError::NotFound)
    }

    pub async fn create(&self, p: &Principal, input: NewHabit) -> Result<Habit, HabitError> {
        let id = match input.id {
            Some(id) if !is_id(&id) => {
                return Err(HabitError::Invalid("id", "expected 32 hex characters"))
            }
            Some(id)
                if self
                    .repo
                    .get(&p.user_id, &id)
                    .await
                    .map_err(backend)?
                    .is_some() =>
            {
                return Err(HabitError::Exists)
            }
            Some(id) => id,
            None => new_id(self.entropy),
        };
        let habit = Habit {
            id,
            owner_id: p.user_id.clone(),
            name: name(&input.name)?,
            days: valid_days(input.days)?,
            archived: false,
            created_at: self.now,
            updated_at: self.now,
        };
        if self.repo.count(&p.user_id).await.map_err(backend)? >= MAX_HABITS {
            return Err(HabitError::Invalid(
                "habits",
                "limit of 200 habits reached; delete some first",
            ));
        }
        self.repo.insert(&habit).await.map_err(backend)?;
        Ok(habit)
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: HabitPatch,
    ) -> Result<Habit, HabitError> {
        let mut habit = self.get(p, id).await?;
        if let Some(n) = patch.name {
            habit.name = name(&n)?;
        }
        if patch.days.is_some() {
            habit.days = valid_days(patch.days)?;
        }
        if let Some(a) = patch.archived {
            habit.archived = a;
        }
        habit.updated_at = self.now;
        if !self.repo.update(&habit).await.map_err(backend)? {
            return Err(HabitError::NotFound);
        }
        Ok(habit)
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), HabitError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(HabitError::NotFound)
        }
    }

    /// Mark `date` done (or not). Idempotent, so offline replays are safe.
    /// Dates up to one day past the server's UTC date are accepted (time zones).
    pub async fn set_check(
        &self,
        p: &Principal,
        id: &str,
        date: &str,
        done: bool,
    ) -> Result<(), HabitError> {
        let date = validate::date(date).map_err(|w| HabitError::Invalid("date", w))?;
        let latest = validate::civil_from_days(self.now.div_euclid(MS_PER_DAY) + 1);
        if date > latest {
            return Err(HabitError::Invalid("date", "in the future"));
        }
        self.get(p, id).await?;
        let r = if done {
            self.repo.check_in(&p.user_id, id, &date, self.now).await
        } else {
            self.repo.uncheck(&p.user_id, id, &date).await
        };
        r.map_err(backend)
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct MemoryHabits(pub RefCell<Vec<Habit>>, pub RefCell<Vec<(String, CheckIn)>>);

    impl HabitRepository for MemoryHabits {
        async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Habit>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|h| h.owner_id == owner_id)
                .take(limit as usize)
                .cloned()
                .collect())
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Habit>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|h| h.owner_id == owner_id && h.id == id)
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
        async fn insert(&self, habit: &Habit) -> Result<(), String> {
            self.0.borrow_mut().push(habit.clone());
            Ok(())
        }
        async fn update(&self, habit: &Habit) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            match all
                .iter_mut()
                .find(|h| h.owner_id == habit.owner_id && h.id == habit.id)
            {
                Some(h) => {
                    *h = habit.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|h| !(h.owner_id == owner_id && h.id == id));
            self.1.borrow_mut().retain(|(_, c)| c.habit_id != id);
            Ok(all.len() < before)
        }
        async fn checkins(
            &self,
            owner_id: &str,
            from: &str,
            to: &str,
        ) -> Result<Vec<CheckIn>, String> {
            Ok(self
                .1
                .borrow()
                .iter()
                .filter(|(o, c)| o == owner_id && c.date.as_str() >= from && c.date.as_str() <= to)
                .map(|(_, c)| c.clone())
                .collect())
        }
        async fn check_in(
            &self,
            owner_id: &str,
            habit_id: &str,
            date: &str,
            _now: i64,
        ) -> Result<(), String> {
            let mut all = self.1.borrow_mut();
            if !all
                .iter()
                .any(|(_, c)| c.habit_id == habit_id && c.date == date)
            {
                all.push((
                    owner_id.into(),
                    CheckIn {
                        habit_id: habit_id.into(),
                        date: date.into(),
                    },
                ));
            }
            Ok(())
        }
        async fn uncheck(&self, _owner_id: &str, habit_id: &str, date: &str) -> Result<(), String> {
            self.1
                .borrow_mut()
                .retain(|(_, c)| !(c.habit_id == habit_id && c.date == date));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryHabits;
    use super::*;
    use crate::entropy::testing::CountingEntropy;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;

    /// 2026-10-22 (a Thursday) at noon UTC.
    const NOW: i64 = 1_792_670_400_000;
    const TODAY: &str = "2026-10-22";

    fn owner(id: &str) -> Principal {
        Principal {
            user_id: id.into(),
            method: AuthMethod::Session,
        }
    }

    #[test]
    fn streaks_skip_unscheduled_days_and_spare_today() {
        let every: Vec<u8> = (0..7).collect();
        let set = |d: &[&'static str]| d.iter().copied().collect::<HashSet<&str>>();
        // Today not done yet: yesterday and the day before still count.
        assert_eq!(
            streak(&every, &set(&["2026-10-21", "2026-10-20"]), TODAY),
            2
        );
        assert_eq!(streak(&every, &set(&[TODAY, "2026-10-21"]), TODAY), 2);
        // A missed scheduled day ends it.
        assert_eq!(streak(&every, &set(&[TODAY, "2026-10-20"]), TODAY), 1);
        // Weekdays only: Monday 19th, then the weekend (17th, 18th) is skipped.
        let weekdays: Vec<u8> = (0..5).collect();
        let around_weekend = set(&["2026-10-19", "2026-10-16", "2026-10-15"]);
        assert_eq!(streak(&weekdays, &around_weekend, "2026-10-19"), 3);
        // A bonus check-in on an unscheduled day counts.
        assert_eq!(
            streak(
                &weekdays,
                &set(&["2026-10-19", "2026-10-18", "2026-10-16"]),
                "2026-10-19"
            ),
            3
        );
        assert_eq!(streak(&every, &set(&[]), TODAY), 0);
    }

    #[test]
    fn lifecycle_checkins_and_validation() {
        let repo = MemoryHabits::default();
        let e = CountingEntropy::default();
        let svc = HabitService::new(&repo, &e, NOW);
        let me = owner("u1");

        let h = block_on(svc.create(
            &me,
            NewHabit {
                name: " Stretch ".into(),
                days: Some(vec![4, 0, 2, 2]),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            (h.name.as_str(), h.days.clone()),
            ("Stretch", vec![0, 2, 4])
        );
        assert_eq!(days_to_mask(&h.days), 0b10101);

        for (days, why) in [
            (vec![], "pick at least one day"),
            (vec![7], "expected weekdays 0 (Monday) to 6"),
        ] {
            assert_eq!(
                block_on(svc.create(
                    &me,
                    NewHabit {
                        name: "x".into(),
                        days: Some(days),
                        ..Default::default()
                    }
                ))
                .unwrap_err(),
                HabitError::Invalid("days", why)
            );
        }

        // Check-ins are idempotent; tomorrow (UTC+) is allowed, later isn't.
        for _ in 0..2 {
            block_on(svc.set_check(&me, &h.id, "2026-10-21", true)).unwrap();
        }
        block_on(svc.set_check(&me, &h.id, "2026-10-23", true)).unwrap();
        assert_eq!(
            block_on(svc.set_check(&me, &h.id, "2026-10-24", true)).unwrap_err(),
            HabitError::Invalid("date", "in the future")
        );
        block_on(svc.set_check(&me, &h.id, "2026-10-23", false)).unwrap();
        block_on(svc.set_check(&me, &h.id, "2026-10-23", false)).unwrap();

        let list = block_on(svc.list(&me, TODAY, 7)).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].checkins, vec!["2026-10-21".to_string()]);
        // Wed 21st done; Thursday isn't scheduled; Monday 19th missed.
        assert_eq!(list[0].streak, 1);

        // Another owner can't see or touch it.
        let other = owner("u2");
        assert!(block_on(svc.list(&other, TODAY, 7)).unwrap().is_empty());
        assert_eq!(
            block_on(svc.set_check(&other, &h.id, TODAY, true)).unwrap_err(),
            HabitError::NotFound
        );

        let archived = block_on(svc.update(
            &me,
            &h.id,
            HabitPatch {
                archived: Some(true),
                ..Default::default()
            },
        ))
        .unwrap();
        assert!(archived.archived);
        block_on(svc.delete(&me, &h.id)).unwrap();
        assert!(repo.1.borrow().is_empty(), "check-ins go with the habit");
        assert_eq!(
            block_on(svc.delete(&me, &h.id)).unwrap_err(),
            HabitError::NotFound
        );
    }

    #[test]
    fn creates_stop_at_the_cap_so_lists_never_drop_habits() {
        let repo = MemoryHabits::default();
        let e = CountingEntropy::default();
        let svc = HabitService::new(&repo, &e, NOW);
        let new = |name: &str| NewHabit {
            name: name.into(),
            ..Default::default()
        };
        for i in 0..MAX_HABITS {
            block_on(svc.create(&owner("u1"), new(&format!("h{i}")))).unwrap();
        }
        assert_eq!(
            block_on(svc.create(&owner("u1"), new("one more"))).unwrap_err(),
            HabitError::Invalid("habits", "limit of 200 habits reached; delete some first")
        );
        // Per owner.
        block_on(svc.create(&owner("u2"), new("theirs"))).unwrap();
        assert_eq!(
            block_on(svc.list(&owner("u1"), TODAY, 1)).unwrap().len(),
            MAX_HABITS as usize
        );
    }

    #[test]
    fn client_ids_make_offline_creates_idempotent() {
        let repo = MemoryHabits::default();
        let e = CountingEntropy::default();
        let svc = HabitService::new(&repo, &e, NOW);
        let new = || NewHabit {
            id: Some("b".repeat(32)),
            name: "Read".into(),
            days: None,
        };
        assert_eq!(
            block_on(svc.create(&owner("u1"), new()))
                .unwrap()
                .days
                .len(),
            7
        );
        assert_eq!(
            block_on(svc.create(&owner("u1"), new())).unwrap_err(),
            HabitError::Exists
        );
    }
}
