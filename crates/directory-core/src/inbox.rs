//! Universal inbox: capture anything quickly, then triage it into a task, a
//! note, or a journal entry. Triage goes through the target module's service
//! (its validation and ownership rules), never through its tables directly.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::journal::{self, JournalError, JournalRepository, JournalService};
use crate::notes::{self, NewNote, NoteError, NoteRepository, NoteService};
use crate::tasks::{self, NewTask, TaskError, TaskRepository, TaskService};
use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_TEXT: usize = 2_000;
pub const MAX_LIST: u32 = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InboxItem {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Capture {
    /// Client-chosen id (offline capture); must be a fresh 32-hex id.
    #[serde(default)]
    pub id: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriageTarget {
    Task,
    Note,
    Journal,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Triage {
    pub to: TriageTarget,
    /// Journal only: the local date to append to (`YYYY-MM-DD`).
    #[serde(default)]
    pub date: Option<String>,
}

/// What an item became.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Triaged {
    Task { task: tasks::Task },
    Note { note: notes::Note },
    Journal { entry: journal::Entry },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboxError {
    Invalid(&'static str, &'static str),
    /// A client-supplied id is already in use (a replayed offline capture).
    Exists,
    NotFound,
    Backend(String),
}

pub trait InboxRepository {
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<InboxItem>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<InboxItem>, String>;
    async fn insert(&self, item: &InboxItem) -> Result<(), String>;
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
}

fn backend(e: String) -> InboxError {
    InboxError::Backend(e)
}

impl From<TaskError> for InboxError {
    fn from(e: TaskError) -> Self {
        match e {
            TaskError::Invalid(f, w) => Self::Invalid(f, w),
            TaskError::NotFound | TaskError::Exists => Self::NotFound,
            TaskError::Backend(b) => Self::Backend(b),
        }
    }
}

impl From<NoteError> for InboxError {
    fn from(e: NoteError) -> Self {
        match e {
            NoteError::Invalid(f, w) => Self::Invalid(f, w),
            NoteError::NotFound | NoteError::Exists => Self::NotFound,
            NoteError::Conflict(_) => Self::Backend("note conflict during create".into()),
            NoteError::Backend(b) => Self::Backend(b),
        }
    }
}

impl From<JournalError> for InboxError {
    fn from(e: JournalError) -> Self {
        match e {
            JournalError::Invalid(f, w) => Self::Invalid(f, w),
            JournalError::NotFound => Self::NotFound,
            JournalError::Conflict(_) => {
                Self::Backend("journal changed during append; retry".into())
            }
            JournalError::Backend(b) => Self::Backend(b),
        }
    }
}

/// Split captured text into a task title (first line, ≤ 200 chars) and notes.
pub fn task_from_text(text: &str) -> NewTask {
    let mut lines = text.trim().splitn(2, '\n');
    let first = lines.next().unwrap_or("").trim();
    let rest = lines.next().unwrap_or("").trim();
    let title: String = first.chars().take(tasks::MAX_TITLE).collect();
    // Keep anything the title had to drop.
    let overflow = first.chars().count() > tasks::MAX_TITLE;
    let notes = match (overflow, rest.is_empty()) {
        (true, true) => first.to_owned(),
        (true, false) => format!("{first}\n{rest}"),
        (false, _) => rest.to_owned(),
    };
    NewTask {
        id: None,
        title,
        notes: (!notes.is_empty()).then_some(notes),
        priority: None,
        due_date: None,
        parent_id: None,
        project_id: None,
    }
}

/// The module repositories triage writes through.
pub struct Targets<'a, T, N, J> {
    pub tasks: &'a T,
    pub notes: &'a N,
    pub journal: &'a J,
}

pub struct InboxService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: InboxRepository, E: Entropy + ?Sized> InboxService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    /// Oldest first: an inbox is processed in arrival order.
    pub async fn list(&self, p: &Principal) -> Result<Vec<InboxItem>, InboxError> {
        self.repo.list(&p.user_id, MAX_LIST).await.map_err(backend)
    }

    pub async fn capture(&self, p: &Principal, input: Capture) -> Result<InboxItem, InboxError> {
        let text = validate::text(input.text.trim_start(), MAX_TEXT)
            .map_err(|w| InboxError::Invalid("text", w))?;
        if text.is_empty() {
            return Err(InboxError::Invalid("text", "required"));
        }
        let id = match input.id {
            Some(id) if !is_id(&id) => {
                return Err(InboxError::Invalid("id", "expected 32 hex characters"))
            }
            Some(id)
                if self
                    .repo
                    .get(&p.user_id, &id)
                    .await
                    .map_err(backend)?
                    .is_some() =>
            {
                return Err(InboxError::Exists)
            }
            Some(id) => id,
            None => new_id(self.entropy),
        };
        let item = InboxItem {
            id,
            owner_id: p.user_id.clone(),
            text,
            created_at: self.now,
        };
        self.repo.insert(&item).await.map_err(backend)?;
        Ok(item)
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), InboxError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(InboxError::NotFound)
        }
    }

    /// Convert an item, then remove it. The target is written first, so a
    /// failure can at worst leave the item in the inbox; it is never lost.
    pub async fn triage<T, N, J>(
        &self,
        p: &Principal,
        id: &str,
        how: Triage,
        targets: Targets<'_, T, N, J>,
    ) -> Result<Triaged, InboxError>
    where
        T: TaskRepository,
        N: NoteRepository,
        J: JournalRepository,
    {
        if !is_id(id) {
            return Err(InboxError::NotFound);
        }
        let item = self
            .repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(InboxError::NotFound)?;
        let result = match how.to {
            TriageTarget::Task => {
                let svc = TaskService::new(targets.tasks, self.entropy, self.now);
                Triaged::Task {
                    task: svc.create(p, task_from_text(&item.text)).await?,
                }
            }
            TriageTarget::Note => {
                let svc = NoteService::new(targets.notes, self.entropy, self.now);
                let input = NewNote {
                    body: Some(item.text.clone()),
                    ..Default::default()
                };
                Triaged::Note {
                    note: svc.create(p, input).await?,
                }
            }
            TriageTarget::Journal => {
                let date = how
                    .date
                    .ok_or(InboxError::Invalid("date", "required for journal"))?;
                let svc = JournalService::new(targets.journal, self.now);
                Triaged::Journal {
                    entry: svc.append(p, &date, &item.text).await?,
                }
            }
        };
        self.repo.delete(&p.user_id, id).await.map_err(backend)?;
        Ok(result)
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct MemoryInbox(pub RefCell<Vec<InboxItem>>);

    impl InboxRepository for MemoryInbox {
        async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<InboxItem>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|i| i.owner_id == owner_id)
                .take(limit as usize)
                .cloned()
                .collect())
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<InboxItem>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|i| i.owner_id == owner_id && i.id == id)
                .cloned())
        }
        async fn insert(&self, item: &InboxItem) -> Result<(), String> {
            self.0.borrow_mut().push(item.clone());
            Ok(())
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|i| !(i.owner_id == owner_id && i.id == id));
            Ok(all.len() < before)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryInbox;
    use super::*;
    use crate::entropy::testing::CountingEntropy;
    use crate::identity::AuthMethod;
    use crate::journal::testing::MemoryJournal;
    use crate::notes::testing::MemoryNotes;
    use crate::tasks::memory::MemoryTasks;
    use crate::test_util::block_on;

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    struct F {
        inbox: MemoryInbox,
        tasks: MemoryTasks,
        notes: MemoryNotes,
        journal: MemoryJournal,
        e: CountingEntropy,
    }

    impl F {
        fn new() -> Self {
            Self {
                inbox: MemoryInbox::default(),
                tasks: MemoryTasks::default(),
                notes: MemoryNotes::default(),
                journal: MemoryJournal::default(),
                e: CountingEntropy::default(),
            }
        }
        fn svc(&self) -> InboxService<'_, MemoryInbox, CountingEntropy> {
            InboxService::new(&self.inbox, &self.e, 100)
        }
        fn targets(&self) -> Targets<'_, MemoryTasks, MemoryNotes, MemoryJournal> {
            Targets {
                tasks: &self.tasks,
                notes: &self.notes,
                journal: &self.journal,
            }
        }
        fn capture(&self, text: &str) -> InboxItem {
            block_on(self.svc().capture(
                &me(),
                Capture {
                    id: None,
                    text: text.into(),
                },
            ))
            .unwrap()
        }
        fn triage(
            &self,
            id: &str,
            to: TriageTarget,
            date: Option<&str>,
        ) -> Result<Triaged, InboxError> {
            block_on(self.svc().triage(
                &me(),
                id,
                Triage {
                    to,
                    date: date.map(Into::into),
                },
                self.targets(),
            ))
        }
    }

    #[test]
    fn capture_validates() {
        let f = F::new();
        assert_eq!(
            block_on(f.svc().capture(
                &me(),
                Capture {
                    id: None,
                    text: "  \n ".into()
                }
            ))
            .unwrap_err(),
            InboxError::Invalid("text", "required")
        );
        assert!(matches!(
            block_on(f.svc().capture(
                &me(),
                Capture {
                    id: None,
                    text: "x".repeat(MAX_TEXT + 1)
                }
            )),
            Err(InboxError::Invalid("text", "too long"))
        ));
        assert_eq!(f.capture("  buy milk").text, "buy milk");
    }

    #[test]
    fn triage_to_task_and_note_removes_item() {
        let f = F::new();
        let a = f.capture("Call the plumber\nabout the leak under the sink");
        match f.triage(&a.id, TriageTarget::Task, None).unwrap() {
            Triaged::Task { task } => {
                assert_eq!(task.title, "Call the plumber");
                assert_eq!(task.notes, "about the leak under the sink");
            }
            other => panic!("{other:?}"),
        }
        let b = f.capture("Book idea: a lighthouse keeper");
        assert!(
            matches!(f.triage(&b.id, TriageTarget::Note, None).unwrap(), Triaged::Note { note } if note.body.starts_with("Book idea"))
        );
        assert!(block_on(f.svc().list(&me())).unwrap().is_empty());
        assert_eq!(
            f.triage(&a.id, TriageTarget::Task, None).unwrap_err(),
            InboxError::NotFound,
            "already triaged"
        );
    }

    #[test]
    fn triage_to_journal_appends() {
        let f = F::new();
        let a = f.capture("Morning run, 5k");
        let b = f.capture("Lunch with Sam");
        f.triage(&a.id, TriageTarget::Journal, Some("2026-10-21"))
            .unwrap();
        let Triaged::Journal { entry } = f
            .triage(&b.id, TriageTarget::Journal, Some("2026-10-21"))
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(entry.body, "Morning run, 5k\n\nLunch with Sam");
        let c = f.capture("x");
        assert_eq!(
            f.triage(&c.id, TriageTarget::Journal, None).unwrap_err(),
            InboxError::Invalid("date", "required for journal")
        );
        assert_eq!(
            block_on(f.svc().list(&me())).unwrap().len(),
            1,
            "failed triage keeps the item"
        );
    }

    #[test]
    fn long_first_line_keeps_everything() {
        let long = "y".repeat(250);
        let t = task_from_text(&long);
        assert_eq!(t.title.chars().count(), tasks::MAX_TITLE);
        assert_eq!(t.notes.as_deref(), Some(long.as_str()));
        assert_eq!(task_from_text("just a title").notes, None);
    }

    #[test]
    fn client_ids_make_offline_captures_idempotent() {
        let f = F::new();
        let id = "e".repeat(32);
        let first = block_on(f.svc().capture(
            &me(),
            Capture {
                id: Some(id.clone()),
                text: "offline thought".into(),
            },
        ))
        .unwrap();
        assert_eq!(first.id, id);
        assert_eq!(
            block_on(f.svc().capture(
                &me(),
                Capture {
                    id: Some(id),
                    text: "offline thought".into()
                }
            ))
            .unwrap_err(),
            InboxError::Exists
        );
        assert_eq!(block_on(f.svc().list(&me())).unwrap().len(), 1);
    }
}
