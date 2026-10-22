//! Notes: plain-text/Markdown documents owned by a principal, with optimistic
//! concurrency so autosaves from two devices cannot silently overwrite each other.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 200;
pub const MAX_BODY: usize = 100_000;
pub const MAX_LIST: u32 = 1000;
/// Characters of body the repository returns for list excerpts.
pub const HEAD_CHARS: usize = 400;
const EXCERPT_CHARS: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Note {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub title: String,
    pub body: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// List row: everything but the full body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteSummary {
    pub id: String,
    /// Explicit title, or the first non-empty line of the body.
    pub title: String,
    pub excerpt: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// What a repository returns for listing: the first `HEAD_CHARS` of the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteHead {
    pub id: String,
    pub title: String,
    pub body_head: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<NoteHead> for NoteSummary {
    fn from(h: NoteHead) -> Self {
        let mut lines = h.body_head.lines().map(str::trim).filter(|l| !l.is_empty());
        let (title, rest): (String, Vec<&str>) = if h.title.is_empty() {
            let first = lines.next().unwrap_or("").trim_start_matches('#').trim();
            (first.to_owned(), lines.collect())
        } else {
            (h.title.clone(), lines.collect())
        };
        let excerpt: String = rest.join(" ").chars().take(EXCERPT_CHARS).collect();
        NoteSummary {
            id: h.id,
            title: if title.is_empty() {
                "Untitled".into()
            } else {
                title.chars().take(MAX_TITLE).collect()
            },
            excerpt,
            pinned: h.pinned,
            created_at: h.created_at,
            updated_at: h.updated_at,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewNote {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NotePatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    /// If set, the update only applies when the note is still at this version.
    #[serde(default)]
    pub expected_updated_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteError {
    Invalid(&'static str, &'static str),
    NotFound,
    /// The note changed since `expected_updated_at`; carries the current version.
    Conflict(Box<Note>),
    Backend(String),
}

pub trait NoteRepository {
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<NoteHead>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Note>, String>;
    async fn insert(&self, note: &Note) -> Result<(), String>;
    /// Write `note` only if the stored `updated_at` equals `expected_updated_at`.
    /// Returns `false` when nothing matched (missing, or a concurrent change).
    async fn update_if(&self, note: &Note, expected_updated_at: i64) -> Result<bool, String>;
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
}

fn backend(e: String) -> NoteError {
    NoteError::Backend(e)
}

fn title(raw: &str) -> Result<String, NoteError> {
    // Titles are optional: empty means "derive from the body".
    if raw.trim().is_empty() {
        return Ok(String::new());
    }
    validate::line(raw, MAX_TITLE).map_err(|w| NoteError::Invalid("title", w))
}

fn body(raw: &str) -> Result<String, NoteError> {
    validate::text(raw, MAX_BODY).map_err(|w| NoteError::Invalid("body", w))
}

pub struct NoteService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: NoteRepository, E: Entropy + ?Sized> NoteService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    /// Summaries, pinned first, then most recently updated.
    pub async fn list(&self, p: &Principal) -> Result<Vec<NoteSummary>, NoteError> {
        let mut out: Vec<NoteSummary> = self
            .repo
            .list(&p.user_id, MAX_LIST)
            .await
            .map_err(backend)?
            .into_iter()
            .map(NoteSummary::from)
            .collect();
        out.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then(b.updated_at.cmp(&a.updated_at))
        });
        Ok(out)
    }

    pub async fn get(&self, p: &Principal, id: &str) -> Result<Note, NoteError> {
        if !is_id(id) {
            return Err(NoteError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(NoteError::NotFound)
    }

    pub async fn create(&self, p: &Principal, input: NewNote) -> Result<Note, NoteError> {
        let note = Note {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            title: title(input.title.as_deref().unwrap_or(""))?,
            body: body(input.body.as_deref().unwrap_or(""))?,
            pinned: input.pinned.unwrap_or(false),
            created_at: self.now,
            updated_at: self.now,
        };
        self.repo.insert(&note).await.map_err(backend)?;
        Ok(note)
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: NotePatch,
    ) -> Result<Note, NoteError> {
        let current = self.get(p, id).await?;
        if patch
            .expected_updated_at
            .is_some_and(|v| v != current.updated_at)
        {
            return Err(NoteError::Conflict(Box::new(current)));
        }
        let mut next = current.clone();
        if let Some(t) = patch.title {
            next.title = title(&t)?;
        }
        if let Some(b) = patch.body {
            next.body = body(&b)?;
        }
        if let Some(pin) = patch.pinned {
            next.pinned = pin;
        }
        // Strictly increasing so every save is a distinct version.
        next.updated_at = self.now.max(current.updated_at + 1);
        if self
            .repo
            .update_if(&next, current.updated_at)
            .await
            .map_err(backend)?
        {
            return Ok(next);
        }
        // Lost a race between our read and write: report the winner.
        match self.repo.get(&p.user_id, id).await.map_err(backend)? {
            Some(winner) => Err(NoteError::Conflict(Box::new(winner))),
            None => Err(NoteError::NotFound),
        }
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), NoteError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(NoteError::NotFound)
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    pub use super::tests::Memory as MemoryNotes;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::entropy::testing::CountingEntropy;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct Memory(pub RefCell<Vec<Note>>);

    impl NoteRepository for Memory {
        async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<NoteHead>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|n| n.owner_id == owner_id)
                .take(limit as usize)
                .map(|n| NoteHead {
                    id: n.id.clone(),
                    title: n.title.clone(),
                    body_head: n.body.chars().take(HEAD_CHARS).collect(),
                    pinned: n.pinned,
                    created_at: n.created_at,
                    updated_at: n.updated_at,
                })
                .collect())
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Note>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|n| n.owner_id == owner_id && n.id == id)
                .cloned())
        }
        async fn insert(&self, note: &Note) -> Result<(), String> {
            self.0.borrow_mut().push(note.clone());
            Ok(())
        }
        async fn update_if(&self, note: &Note, expected: i64) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            match all.iter_mut().find(|n| {
                n.owner_id == note.owner_id && n.id == note.id && n.updated_at == expected
            }) {
                Some(n) => {
                    *n = note.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|n| !(n.owner_id == owner_id && n.id == id));
            Ok(all.len() < before)
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    #[test]
    fn summaries_derive_titles_and_order_pinned_first() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let svc = |t| NoteService::new(&repo, &e, t);
        block_on(svc(1).create(
            &me(),
            NewNote {
                body: Some("# Groceries\n\nmilk\neggs".into()),
                ..Default::default()
            },
        ))
        .unwrap();
        block_on(svc(2).create(
            &me(),
            NewNote {
                title: Some("Ideas".into()),
                body: Some("one\ntwo".into()),
                ..Default::default()
            },
        ))
        .unwrap();
        block_on(svc(3).create(
            &me(),
            NewNote {
                pinned: Some(true),
                ..Default::default()
            },
        ))
        .unwrap();

        let list = block_on(svc(4).list(&me())).unwrap();
        let titles: Vec<_> = list
            .iter()
            .map(|n| (n.title.as_str(), n.excerpt.as_str()))
            .collect();
        assert_eq!(
            titles,
            [
                ("Untitled", ""),
                ("Ideas", "one two"),
                ("Groceries", "milk eggs")
            ]
        );
        assert!(list[0].pinned);
    }

    #[test]
    fn optimistic_concurrency() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let n =
            block_on(NoteService::new(&repo, &e, 10).create(&me(), NewNote::default())).unwrap();

        // Device A saves against version 10.
        let a = NotePatch {
            body: Some("from A".into()),
            expected_updated_at: Some(10),
            ..Default::default()
        };
        let saved = block_on(NoteService::new(&repo, &e, 10).update(&me(), &n.id, a)).unwrap();
        assert!(
            saved.updated_at > 10,
            "versions strictly increase even within the same millisecond"
        );

        // Device B still thinks it's version 10: conflict, with A's text returned.
        let b = NotePatch {
            body: Some("from B".into()),
            expected_updated_at: Some(10),
            ..Default::default()
        };
        match block_on(NoteService::new(&repo, &e, 20).update(&me(), &n.id, b)) {
            Err(NoteError::Conflict(current)) => assert_eq!(current.body, "from A"),
            other => panic!("expected conflict, got {other:?}"),
        }

        // Without an expected version, last write wins (explicit overwrite).
        let force = NotePatch {
            body: Some("from B".into()),
            ..Default::default()
        };
        assert_eq!(
            block_on(NoteService::new(&repo, &e, 30).update(&me(), &n.id, force))
                .unwrap()
                .body,
            "from B"
        );
    }

    #[test]
    fn validation_and_scoping() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let svc = NoteService::new(&repo, &e, 1);
        assert_eq!(
            block_on(svc.create(
                &me(),
                NewNote {
                    body: Some("x".repeat(MAX_BODY + 1)),
                    ..Default::default()
                }
            ))
            .unwrap_err(),
            NoteError::Invalid("body", "too long")
        );
        let n = block_on(svc.create(
            &me(),
            NewNote {
                title: Some("   ".into()),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(n.title, "", "blank title means derived");
        let other = Principal {
            user_id: "u2".into(),
            method: AuthMethod::Session,
        };
        assert_eq!(
            block_on(svc.get(&other, &n.id)).unwrap_err(),
            NoteError::NotFound
        );
        assert_eq!(
            block_on(svc.delete(&other, &n.id)).unwrap_err(),
            NoteError::NotFound
        );
        block_on(svc.delete(&me(), &n.id)).unwrap();
    }
}
