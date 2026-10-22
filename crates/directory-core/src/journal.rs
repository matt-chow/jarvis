//! Journal: at most one entry per local calendar date per owner.
//! Uses the same optimistic concurrency as notes (DECISIONS D-014).

use crate::identity::Principal;
use crate::validate::{self, double_option};
use serde::{Deserialize, Serialize};

pub const MAX_BODY: usize = 50_000;
/// Longest range one list request may cover.
pub const MAX_RANGE_DAYS: i64 = 366;
pub const HEAD_CHARS: usize = 400;
const EXCERPT_CHARS: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    #[serde(skip)]
    pub owner_id: String,
    /// `YYYY-MM-DD`, the entry's identity.
    pub date: String,
    pub body: String,
    /// 1 (awful) to 5 (great).
    pub mood: Option<u8>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntrySummary {
    pub date: String,
    pub excerpt: String,
    pub mood: Option<u8>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryHead {
    pub date: String,
    pub body_head: String,
    pub mood: Option<u8>,
    pub updated_at: i64,
}

impl From<EntryHead> for EntrySummary {
    fn from(h: EntryHead) -> Self {
        let excerpt = h
            .body_head
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(EXCERPT_CHARS)
            .collect();
        Self {
            date: h.date,
            excerpt,
            mood: h.mood,
            updated_at: h.updated_at,
        }
    }
}

/// Upsert body for `PUT /journal/{date}`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PutEntry {
    #[serde(default)]
    pub body: Option<String>,
    /// `null` clears the mood; omitted leaves it unchanged.
    #[serde(default, deserialize_with = "double_option")]
    pub mood: Option<Option<u8>>,
    /// Version the client last saw; omit when creating or overwriting.
    #[serde(default)]
    pub expected_updated_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    Invalid(&'static str, &'static str),
    NotFound,
    Conflict(Box<Entry>),
    Backend(String),
}

pub trait JournalRepository {
    /// Heads for dates in `[from, to]`, newest first.
    async fn list(&self, owner_id: &str, from: &str, to: &str) -> Result<Vec<EntryHead>, String>;
    async fn get(&self, owner_id: &str, date: &str) -> Result<Option<Entry>, String>;
    /// Insert unless an entry for that date exists; `false` if it did.
    async fn insert_if_absent(&self, entry: &Entry) -> Result<bool, String>;
    /// Write only if the stored version equals `expected_updated_at`.
    async fn update_if(&self, entry: &Entry, expected_updated_at: i64) -> Result<bool, String>;
    async fn delete(&self, owner_id: &str, date: &str) -> Result<bool, String>;
}

fn backend(e: String) -> JournalError {
    JournalError::Backend(e)
}

fn date(raw: &str, field: &'static str) -> Result<String, JournalError> {
    validate::date(raw).map_err(|w| JournalError::Invalid(field, w))
}

fn mood(m: Option<u8>) -> Result<Option<u8>, JournalError> {
    match m {
        Some(v) if !(1..=5).contains(&v) => Err(JournalError::Invalid("mood", "expected 1 to 5")),
        other => Ok(other),
    }
}

/// Days from 0000-03-01 (proleptic Gregorian), for range-length checks.
fn day_number(d: &str) -> i64 {
    let n = |r: std::ops::Range<usize>| d[r].parse::<i64>().unwrap_or(0);
    let (mut y, m, day) = (n(0..4), n(5..7), n(8..10));
    let m = if m <= 2 {
        y -= 1;
        m + 9
    } else {
        m - 3
    };
    365 * y + y / 4 - y / 100 + y / 400 + (153 * m + 2) / 5 + day - 1
}

pub struct JournalService<'a, R> {
    repo: &'a R,
    now: i64,
}

impl<'a, R: JournalRepository> JournalService<'a, R> {
    pub fn new(repo: &'a R, now: i64) -> Self {
        Self { repo, now }
    }

    pub async fn list(
        &self,
        p: &Principal,
        from: &str,
        to: &str,
    ) -> Result<Vec<EntrySummary>, JournalError> {
        let (from, to) = (date(from, "from")?, date(to, "to")?);
        if from > to {
            return Err(JournalError::Invalid("to", "before from"));
        }
        if day_number(&to) - day_number(&from) >= MAX_RANGE_DAYS {
            return Err(JournalError::Invalid("to", "range longer than a year"));
        }
        let heads = self
            .repo
            .list(&p.user_id, &from, &to)
            .await
            .map_err(backend)?;
        Ok(heads.into_iter().map(EntrySummary::from).collect())
    }

    pub async fn get(&self, p: &Principal, d: &str) -> Result<Entry, JournalError> {
        let d = date(d, "date").map_err(|_| JournalError::NotFound)?;
        self.repo
            .get(&p.user_id, &d)
            .await
            .map_err(backend)?
            .ok_or(JournalError::NotFound)
    }

    /// Create or update the entry for `d`.
    pub async fn put(
        &self,
        p: &Principal,
        d: &str,
        input: PutEntry,
    ) -> Result<Entry, JournalError> {
        let d = date(d, "date")?;
        let body = input
            .body
            .as_deref()
            .map(|b| validate::text(b, MAX_BODY).map_err(|w| JournalError::Invalid("body", w)))
            .transpose()?;
        let new_mood = input.mood.map(mood).transpose()?;

        let Some(current) = self.repo.get(&p.user_id, &d).await.map_err(backend)? else {
            let entry = Entry {
                owner_id: p.user_id.clone(),
                date: d.clone(),
                body: body.unwrap_or_default(),
                mood: new_mood.flatten(),
                created_at: self.now,
                updated_at: self.now,
            };
            if self.repo.insert_if_absent(&entry).await.map_err(backend)? {
                return Ok(entry);
            }
            // Another device created this day first.
            return match self.repo.get(&p.user_id, &d).await.map_err(backend)? {
                Some(winner) => Err(JournalError::Conflict(Box::new(winner))),
                None => Err(JournalError::Backend("entry vanished during create".into())),
            };
        };

        if input
            .expected_updated_at
            .is_some_and(|v| v != current.updated_at)
        {
            return Err(JournalError::Conflict(Box::new(current)));
        }
        let mut next = current.clone();
        if let Some(b) = body {
            next.body = b;
        }
        if let Some(m) = new_mood {
            next.mood = m;
        }
        next.updated_at = self.now.max(current.updated_at + 1);
        if self
            .repo
            .update_if(&next, current.updated_at)
            .await
            .map_err(backend)?
        {
            return Ok(next);
        }
        match self.repo.get(&p.user_id, &d).await.map_err(backend)? {
            Some(winner) => Err(JournalError::Conflict(Box::new(winner))),
            None => Err(JournalError::NotFound),
        }
    }

    pub async fn delete(&self, p: &Principal, d: &str) -> Result<(), JournalError> {
        let d = date(d, "date").map_err(|_| JournalError::NotFound)?;
        if self.repo.delete(&p.user_id, &d).await.map_err(backend)? {
            Ok(())
        } else {
            Err(JournalError::NotFound)
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    pub use super::tests::Memory as MemoryJournal;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct Memory(RefCell<Vec<Entry>>);

    impl JournalRepository for Memory {
        async fn list(
            &self,
            owner_id: &str,
            from: &str,
            to: &str,
        ) -> Result<Vec<EntryHead>, String> {
            let mut v: Vec<_> = self
                .0
                .borrow()
                .iter()
                .filter(|e| {
                    e.owner_id == owner_id && e.date.as_str() >= from && e.date.as_str() <= to
                })
                .map(|e| EntryHead {
                    date: e.date.clone(),
                    body_head: e.body.chars().take(HEAD_CHARS).collect(),
                    mood: e.mood,
                    updated_at: e.updated_at,
                })
                .collect();
            v.sort_by(|a, b| b.date.cmp(&a.date));
            Ok(v)
        }
        async fn get(&self, owner_id: &str, date: &str) -> Result<Option<Entry>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|e| e.owner_id == owner_id && e.date == date)
                .cloned())
        }
        async fn insert_if_absent(&self, entry: &Entry) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            if all
                .iter()
                .any(|e| e.owner_id == entry.owner_id && e.date == entry.date)
            {
                return Ok(false);
            }
            all.push(entry.clone());
            Ok(true)
        }
        async fn update_if(&self, entry: &Entry, expected: i64) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            match all.iter_mut().find(|e| {
                e.owner_id == entry.owner_id && e.date == entry.date && e.updated_at == expected
            }) {
                Some(e) => {
                    *e = entry.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, date: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|e| !(e.owner_id == owner_id && e.date == date));
            Ok(all.len() < before)
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    fn put(body: &str) -> PutEntry {
        PutEntry {
            body: Some(body.into()),
            ..Default::default()
        }
    }

    #[test]
    fn upsert_versions_and_mood() {
        let repo = Memory::default();
        let created = block_on(JournalService::new(&repo, 10).put(
            &me(),
            "2026-10-21",
            PutEntry {
                mood: Some(Some(4)),
                ..put("Good day")
            },
        ))
        .unwrap();
        assert_eq!((created.mood, created.created_at), (Some(4), 10));

        let edited = block_on(JournalService::new(&repo, 10).put(
            &me(),
            "2026-10-21",
            PutEntry {
                expected_updated_at: Some(10),
                ..put("Good day, long walk")
            },
        ))
        .unwrap();
        assert_eq!(
            (edited.mood, edited.updated_at),
            (Some(4), 11),
            "mood kept; version bumped"
        );

        let stale = PutEntry {
            expected_updated_at: Some(10),
            ..put("from phone")
        };
        assert!(matches!(
            block_on(JournalService::new(&repo, 20).put(&me(), "2026-10-21", stale)),
            Err(JournalError::Conflict(e)) if e.body == "Good day, long walk"
        ));

        let cleared: PutEntry = serde_json::from_str(r#"{"mood":null}"#).unwrap();
        assert_eq!(
            block_on(JournalService::new(&repo, 30).put(&me(), "2026-10-21", cleared))
                .unwrap()
                .mood,
            None
        );
    }

    #[test]
    fn validation() {
        let repo = Memory::default();
        let svc = JournalService::new(&repo, 1);
        assert_eq!(
            block_on(svc.put(&me(), "2026-02-30", put("x"))).unwrap_err(),
            JournalError::Invalid("date", "invalid date")
        );
        let bad_mood = PutEntry {
            mood: Some(Some(6)),
            ..Default::default()
        };
        assert_eq!(
            block_on(svc.put(&me(), "2026-10-21", bad_mood)).unwrap_err(),
            JournalError::Invalid("mood", "expected 1 to 5")
        );
        assert!(matches!(
            block_on(svc.list(&me(), "2026-10-21", "2026-10-01")),
            Err(JournalError::Invalid("to", _))
        ));
        assert!(matches!(
            block_on(svc.list(&me(), "2025-01-01", "2026-01-02")),
            Err(JournalError::Invalid("to", _))
        ));
        assert!(block_on(svc.list(&me(), "2025-01-01", "2025-12-31")).is_ok());
        assert_eq!(
            block_on(svc.get(&me(), "nope")).unwrap_err(),
            JournalError::NotFound
        );
    }

    #[test]
    fn list_is_scoped_ranged_and_excerpted() {
        let repo = Memory::default();
        let svc = JournalService::new(&repo, 1);
        block_on(svc.put(&me(), "2026-10-01", put("first\n\nof   the month"))).unwrap();
        block_on(svc.put(&me(), "2026-10-20", put("later"))).unwrap();
        block_on(svc.put(&me(), "2026-11-01", put("next month"))).unwrap();
        let other = Principal {
            user_id: "u2".into(),
            method: AuthMethod::Session,
        };
        block_on(svc.put(&other, "2026-10-05", put("not mine"))).unwrap();

        let oct = block_on(svc.list(&me(), "2026-10-01", "2026-10-31")).unwrap();
        let got: Vec<_> = oct
            .iter()
            .map(|e| (e.date.as_str(), e.excerpt.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("2026-10-20", "later"),
                ("2026-10-01", "first of the month")
            ]
        );

        block_on(svc.delete(&me(), "2026-10-20")).unwrap();
        assert_eq!(
            block_on(svc.delete(&me(), "2026-10-20")).unwrap_err(),
            JournalError::NotFound
        );
    }

    #[test]
    fn day_numbers_count_leap_years() {
        assert_eq!(day_number("2024-03-01") - day_number("2024-02-28"), 2);
        assert_eq!(day_number("2025-01-01") - day_number("2024-01-01"), 366);
    }
}
