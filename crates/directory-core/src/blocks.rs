//! Time blocks: scheduled work sessions on the calendar (DECISIONS D-047).
//! A block may belong to a task, and a task may have many. Finishing a
//! block doesn't finish its task, and closing a task keeps its blocks (its
//! scheduling history). Times are wall-clock values on one local date.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate::{self, double_option};
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 200;
pub const MAX_NOTES: usize = 5_000;
/// Most blocks an owner can keep; creating more is refused (DECISIONS D-022).
pub const MAX_BLOCKS: u32 = 5_000;
/// Longest actual time recorded for one block (a day).
pub const MAX_ACTUAL: u32 = 1_440;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockStatus {
    #[default]
    Planned,
    Done,
    Skipped,
}

impl BlockStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Done => "done",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "planned" => Self::Planned,
            "done" => Self::Done,
            "skipped" => Self::Skipped,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeBlock {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    /// The task this session works on; `None` = a standalone focus block.
    pub task_id: Option<String>,
    /// Required without a task; optional (a label) with one.
    pub title: String,
    pub date: String,
    /// `HH:MM`
    pub start_time: String,
    /// `HH:MM`, after `start_time`; `24:00` ends at midnight.
    pub end_time: String,
    /// IANA zone the times are in; `None` = the viewer's zone.
    pub timezone: Option<String>,
    /// Flexible by default: it can be moved or suggested elsewhere.
    pub fixed: bool,
    pub status: BlockStatus,
    /// Minutes actually worked, when recorded.
    pub actual_minutes: Option<u32>,
    pub notes: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl TimeBlock {
    /// Scheduled minutes (wall-clock).
    pub fn minutes(&self) -> u32 {
        let at = |t: &str| validate::clock(t, true).unwrap_or(0);
        at(&self.end_time).saturating_sub(at(&self.start_time))
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewBlock {
    /// Client-chosen id (offline create); must be a fresh 32-hex id.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub fixed: Option<bool>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// `null` clears a nullable field; an omitted field is unchanged.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BlockPatch {
    #[serde(default, deserialize_with = "double_option")]
    pub task_id: Option<Option<String>>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub timezone: Option<Option<String>>,
    #[serde(default)]
    pub fixed: Option<bool>,
    #[serde(default)]
    pub status: Option<BlockStatus>,
    #[serde(default, deserialize_with = "double_option")]
    pub actual_minutes: Option<Option<u32>>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockError {
    Invalid(&'static str, &'static str),
    /// A client-supplied id is already in use (a replayed offline create).
    Exists,
    NotFound,
    Backend(String),
}

pub trait BlockRepository {
    /// The owner's blocks by date and start time.
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<TimeBlock>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<TimeBlock>, String>;
    async fn count(&self, owner_id: &str) -> Result<u32, String>;
    async fn insert(&self, block: &TimeBlock) -> Result<(), String>;
    async fn update(&self, block: &TimeBlock) -> Result<bool, String>;
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    /// Whether `task_id` is a task owned by `owner_id`.
    async fn task_exists(&self, owner_id: &str, task_id: &str) -> Result<bool, String>;
}

fn backend(e: String) -> BlockError {
    BlockError::Backend(e)
}

fn field<T>(name: &'static str, r: Result<T, &'static str>) -> Result<T, BlockError> {
    r.map_err(|why| BlockError::Invalid(name, why))
}

/// Normalize and check a block's fields (create, update, and import).
pub fn check(mut b: TimeBlock) -> Result<TimeBlock, BlockError> {
    b.title = if b.task_id.is_some() {
        let t = field("title", validate::text(&b.title, MAX_TITLE))?;
        if t.contains('\n') {
            return Err(BlockError::Invalid("title", "invalid characters"));
        }
        t.trim().to_owned()
    } else {
        field("title", validate::line(&b.title, MAX_TITLE))?
    };
    if let Some(t) = &b.task_id {
        if !is_id(t) {
            return Err(BlockError::Invalid("task_id", "not found"));
        }
    }
    b.date = field("date", validate::date(&b.date))?;
    let start = field("start_time", validate::clock(&b.start_time, false))?;
    let end = field("end_time", validate::clock(&b.end_time, true))?;
    if end <= start {
        return Err(BlockError::Invalid("end_time", "must be after start_time"));
    }
    b.timezone = b
        .timezone
        .as_deref()
        .map(validate::timezone)
        .transpose()
        .map_err(|w| BlockError::Invalid("timezone", w))?;
    if b.actual_minutes.is_some_and(|m| m > MAX_ACTUAL) {
        return Err(BlockError::Invalid(
            "actual_minutes",
            "must be 0 to 1440 minutes",
        ));
    }
    b.notes = field("notes", validate::text(&b.notes, MAX_NOTES))?;
    Ok(b)
}

pub struct BlockService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: BlockRepository, E: Entropy + ?Sized> BlockService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    pub async fn list(&self, p: &Principal) -> Result<Vec<TimeBlock>, BlockError> {
        self.repo
            .list(&p.user_id, MAX_BLOCKS)
            .await
            .map_err(backend)
    }

    pub async fn get(&self, p: &Principal, id: &str) -> Result<TimeBlock, BlockError> {
        if !is_id(id) {
            return Err(BlockError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(BlockError::NotFound)
    }

    /// The linked task must be the caller's own.
    async fn check_task(&self, p: &Principal, task_id: Option<&str>) -> Result<(), BlockError> {
        match task_id {
            Some(t)
                if !is_id(t)
                    || !self
                        .repo
                        .task_exists(&p.user_id, t)
                        .await
                        .map_err(backend)? =>
            {
                Err(BlockError::Invalid("task_id", "not found"))
            }
            _ => Ok(()),
        }
    }

    pub async fn create(&self, p: &Principal, input: NewBlock) -> Result<TimeBlock, BlockError> {
        let id = match input.id {
            Some(id) if !is_id(&id) => {
                return Err(BlockError::Invalid("id", "expected 32 hex characters"))
            }
            Some(id)
                if self
                    .repo
                    .get(&p.user_id, &id)
                    .await
                    .map_err(backend)?
                    .is_some() =>
            {
                return Err(BlockError::Exists)
            }
            Some(id) => id,
            None => new_id(self.entropy),
        };
        let block = check(TimeBlock {
            id,
            owner_id: p.user_id.clone(),
            task_id: input.task_id,
            title: input.title.unwrap_or_default(),
            date: input.date,
            start_time: input.start_time,
            end_time: input.end_time,
            timezone: input.timezone,
            fixed: input.fixed.unwrap_or(false),
            status: BlockStatus::Planned,
            actual_minutes: None,
            notes: input.notes.unwrap_or_default(),
            created_at: self.now,
            updated_at: self.now,
        })?;
        self.check_task(p, block.task_id.as_deref()).await?;
        if self.repo.count(&p.user_id).await.map_err(backend)? >= MAX_BLOCKS {
            return Err(BlockError::Invalid(
                "blocks",
                "limit of 5,000 time blocks reached; delete old ones first",
            ));
        }
        self.repo.insert(&block).await.map_err(backend)?;
        Ok(block)
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: BlockPatch,
    ) -> Result<TimeBlock, BlockError> {
        let mut b = self.get(p, id).await?;
        if let Some(t) = patch.task_id {
            self.check_task(p, t.as_deref()).await?;
            b.task_id = t;
        }
        if let Some(v) = patch.title {
            b.title = v;
        }
        if let Some(v) = patch.date {
            b.date = v;
        }
        if let Some(v) = patch.start_time {
            b.start_time = v;
        }
        if let Some(v) = patch.end_time {
            b.end_time = v;
        }
        if let Some(v) = patch.timezone {
            b.timezone = v;
        }
        if let Some(v) = patch.fixed {
            b.fixed = v;
        }
        if let Some(v) = patch.status {
            b.status = v;
        }
        if let Some(v) = patch.actual_minutes {
            b.actual_minutes = v;
        }
        if let Some(v) = patch.notes {
            b.notes = v;
        }
        b.updated_at = self.now;
        let b = check(b)?;
        if !self.repo.update(&b).await.map_err(backend)? {
            return Err(BlockError::NotFound);
        }
        Ok(b)
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), BlockError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(BlockError::NotFound)
        }
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::cell::RefCell;

    /// Blocks, plus the `(owner_id, task_id)` pairs that exist.
    #[derive(Default)]
    pub struct MemoryBlocks(
        pub RefCell<Vec<TimeBlock>>,
        pub RefCell<Vec<(String, String)>>,
    );

    impl BlockRepository for MemoryBlocks {
        async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<TimeBlock>, String> {
            let mut all: Vec<TimeBlock> = self
                .0
                .borrow()
                .iter()
                .filter(|b| b.owner_id == owner_id)
                .cloned()
                .collect();
            all.sort_by(|a, b| (&a.date, &a.start_time).cmp(&(&b.date, &b.start_time)));
            all.truncate(limit as usize);
            Ok(all)
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<TimeBlock>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|b| b.owner_id == owner_id && b.id == id)
                .cloned())
        }
        async fn count(&self, owner_id: &str) -> Result<u32, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|b| b.owner_id == owner_id)
                .count() as u32)
        }
        async fn insert(&self, block: &TimeBlock) -> Result<(), String> {
            self.0.borrow_mut().push(block.clone());
            Ok(())
        }
        async fn update(&self, block: &TimeBlock) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            match all
                .iter_mut()
                .find(|b| b.owner_id == block.owner_id && b.id == block.id)
            {
                Some(b) => {
                    *b = block.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|b| !(b.owner_id == owner_id && b.id == id));
            Ok(all.len() < before)
        }
        async fn task_exists(&self, owner_id: &str, task_id: &str) -> Result<bool, String> {
            Ok(self
                .1
                .borrow()
                .iter()
                .any(|(o, t)| o == owner_id && t == task_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryBlocks;
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

    const TASK: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn session(start: &str, end: &str) -> NewBlock {
        NewBlock {
            task_id: Some(TASK.into()),
            date: "2026-11-02".into(),
            start_time: start.into(),
            end_time: end.into(),
            ..Default::default()
        }
    }

    fn setup() -> (MemoryBlocks, CountingEntropy) {
        let repo = MemoryBlocks::default();
        repo.1.borrow_mut().push(("u1".into(), TASK.into()));
        (repo, CountingEntropy::default())
    }

    #[test]
    fn many_sessions_per_task_are_flexible_by_default() {
        let (repo, e) = setup();
        let svc = BlockService::new(&repo, &e, 1);
        let me = owner("u1");
        let a = block_on(svc.create(&me, session("09:00", "10:30"))).unwrap();
        let b = block_on(svc.create(&me, session("14:00", "24:00"))).unwrap();
        assert_eq!((a.minutes(), b.minutes()), (90, 600));
        assert!(!a.fixed);
        assert_eq!(a.status, BlockStatus::Planned);
        assert_eq!(block_on(svc.list(&me)).unwrap().len(), 2);

        // Finishing a session records actual time; nothing else changes.
        let done = block_on(svc.update(
            &me,
            &a.id,
            BlockPatch {
                status: Some(BlockStatus::Done),
                actual_minutes: Some(Some(75)),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(
            (done.status, done.actual_minutes, done.task_id.as_deref()),
            (BlockStatus::Done, Some(75), Some(TASK))
        );

        // Moving keeps the task; unlinking needs a title.
        let moved = block_on(svc.update(
            &me,
            &b.id,
            BlockPatch {
                date: Some("2026-11-03".into()),
                start_time: Some("08:00".into()),
                end_time: Some("09:00".into()),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!((moved.date.as_str(), moved.minutes()), ("2026-11-03", 60));
        let unlink: BlockPatch = serde_json::from_str(r#"{"task_id":null}"#).unwrap();
        assert_eq!(
            block_on(svc.update(&me, &b.id, unlink)).unwrap_err(),
            BlockError::Invalid("title", "required")
        );
    }

    #[test]
    fn validation_and_ownership() {
        let (repo, e) = setup();
        let svc = BlockService::new(&repo, &e, 1);
        let me = owner("u1");
        let bad = |input: NewBlock| block_on(svc.create(&me, input)).unwrap_err();
        assert_eq!(
            bad(session("10:00", "10:00")),
            BlockError::Invalid("end_time", "must be after start_time")
        );
        assert_eq!(
            bad(session("10:00", "24:30")),
            BlockError::Invalid("end_time", "expected HH:MM")
        );
        assert_eq!(
            bad(NewBlock {
                task_id: None,
                ..session("10:00", "11:00")
            }),
            BlockError::Invalid("title", "required")
        );
        assert_eq!(
            bad(NewBlock {
                task_id: Some("b".repeat(32)),
                ..session("10:00", "11:00")
            }),
            BlockError::Invalid("task_id", "not found")
        );
        assert_eq!(
            bad(NewBlock {
                timezone: Some("../x".into()),
                ..session("10:00", "11:00")
            }),
            BlockError::Invalid("timezone", "expected an IANA time zone")
        );

        // Someone else's task, and someone else's block.
        let other = owner("u2");
        assert_eq!(
            block_on(svc.create(&other, session("10:00", "11:00"))).unwrap_err(),
            BlockError::Invalid("task_id", "not found")
        );
        let focus = block_on(svc.create(
            &me,
            NewBlock {
                task_id: None,
                title: Some(" Deep work ".into()),
                fixed: Some(true),
                ..session("10:00", "11:00")
            },
        ))
        .unwrap();
        assert_eq!((focus.title.as_str(), focus.fixed), ("Deep work", true));
        assert!(block_on(svc.list(&other)).unwrap().is_empty());
        assert_eq!(
            block_on(svc.update(&other, &focus.id, BlockPatch::default())).unwrap_err(),
            BlockError::NotFound
        );
        assert_eq!(
            block_on(svc.delete(&other, &focus.id)).unwrap_err(),
            BlockError::NotFound
        );
        assert_eq!(
            block_on(svc.update(
                &me,
                &focus.id,
                BlockPatch {
                    actual_minutes: Some(Some(2_000)),
                    ..Default::default()
                }
            ))
            .unwrap_err(),
            BlockError::Invalid("actual_minutes", "must be 0 to 1440 minutes")
        );
        block_on(svc.delete(&me, &focus.id)).unwrap();
    }

    #[test]
    fn client_ids_make_offline_creates_idempotent() {
        let (repo, e) = setup();
        let svc = BlockService::new(&repo, &e, 1);
        let me = owner("u1");
        let replay = || NewBlock {
            id: Some("c".repeat(32)),
            ..session("09:00", "10:00")
        };
        block_on(svc.create(&me, replay())).unwrap();
        assert_eq!(
            block_on(svc.create(&me, replay())).unwrap_err(),
            BlockError::Exists
        );
        assert_eq!(repo.0.borrow().len(), 1);
    }
}
