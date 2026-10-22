//! Tasks and subtasks (one level deep), owned by a principal.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate::{self, double_option};
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 200;
pub const MAX_NOTES: usize = 10_000;
/// Upper bound for one list response (pagination arrives with sync in M3).
pub const MAX_LIST: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Todo,
    InProgress,
    Done,
    Cancelled,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "todo" => Self::Todo,
            "in_progress" => Self::InProgress,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }

    pub fn is_open(self) -> bool {
        matches!(self, Self::Todo | Self::InProgress)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    None,
    Low,
    Medium,
    High,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Task {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub parent_id: Option<String>,
    /// Top-level tasks only; subtasks belong to their parent's project.
    pub project_id: Option<String>,
    pub title: String,
    pub notes: String,
    pub status: Status,
    pub priority: Priority,
    /// `YYYY-MM-DD`
    pub due_date: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    #[default]
    All,
    /// todo + in_progress
    Open,
    /// done + cancelled
    Closed,
}

impl Filter {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "all" => Self::All,
            "open" => Self::Open,
            "closed" => Self::Closed,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewTask {
    /// Client-chosen id (offline creates); must be a fresh 32-hex id.
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
}

/// Partial update. For nullable fields, `Some(None)` (JSON `null`) clears.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TaskPatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub status: Option<Status>,
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub project_id: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    /// (field, reason)
    Invalid(&'static str, &'static str),
    /// A client-supplied id is already in use (a replayed offline create).
    Exists,
    NotFound,
    Backend(String),
}

pub trait TaskRepository {
    async fn list(&self, owner_id: &str, filter: Filter, limit: u32) -> Result<Vec<Task>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Task>, String>;
    async fn insert(&self, task: &Task) -> Result<(), String>;
    /// Replace all mutable fields; `false` if no such task for this owner.
    async fn update(&self, task: &Task) -> Result<bool, String>;
    /// Delete a task and its subtasks; `false` if no such task for this owner.
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    /// Whether `project_id` is a project owned by `owner_id`.
    async fn project_exists(&self, owner_id: &str, project_id: &str) -> Result<bool, String>;
}

fn backend(e: String) -> TaskError {
    TaskError::Backend(e)
}

fn field<T>(name: &'static str, r: Result<T, &'static str>) -> Result<T, TaskError> {
    r.map_err(|why| TaskError::Invalid(name, why))
}

pub struct TaskService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: TaskRepository, E: Entropy + ?Sized> TaskService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    pub async fn list(&self, p: &Principal, filter: Filter) -> Result<Vec<Task>, TaskError> {
        self.repo
            .list(&p.user_id, filter, MAX_LIST)
            .await
            .map_err(backend)
    }

    pub async fn get(&self, p: &Principal, id: &str) -> Result<Task, TaskError> {
        if !is_id(id) {
            return Err(TaskError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(TaskError::NotFound)
    }

    async fn check_project(&self, p: &Principal, project_id: &str) -> Result<(), TaskError> {
        if is_id(project_id)
            && self
                .repo
                .project_exists(&p.user_id, project_id)
                .await
                .map_err(backend)?
        {
            Ok(())
        } else {
            Err(TaskError::Invalid("project_id", "not found"))
        }
    }

    pub async fn create(&self, p: &Principal, input: NewTask) -> Result<Task, TaskError> {
        let title = field("title", validate::line(&input.title, MAX_TITLE))?;
        let notes = field(
            "notes",
            validate::text(input.notes.as_deref().unwrap_or(""), MAX_NOTES),
        )?;
        let due_date = input
            .due_date
            .as_deref()
            .map(validate::date)
            .transpose()
            .map_err(|w| TaskError::Invalid("due_date", w))?;
        if let Some(parent_id) = &input.parent_id {
            // Subtasks are one level deep: the parent must be a top-level task.
            let parent = self.get(p, parent_id).await.map_err(|e| match e {
                TaskError::NotFound => TaskError::Invalid("parent_id", "not found"),
                other => other,
            })?;
            if parent.parent_id.is_some() {
                return Err(TaskError::Invalid(
                    "parent_id",
                    "subtasks cannot have subtasks",
                ));
            }
        }
        if let Some(project_id) = &input.project_id {
            if input.parent_id.is_some() {
                return Err(TaskError::Invalid(
                    "project_id",
                    "subtasks use their parent's project",
                ));
            }
            self.check_project(p, project_id).await?;
        }
        let id = match input.id {
            Some(id) if !is_id(&id) => {
                return Err(TaskError::Invalid("id", "expected 32 hex characters"))
            }
            // Replaying an offline create that already reached the server.
            Some(id)
                if self
                    .repo
                    .get(&p.user_id, &id)
                    .await
                    .map_err(backend)?
                    .is_some() =>
            {
                return Err(TaskError::Exists)
            }
            Some(id) => id,
            None => new_id(self.entropy),
        };
        let task = Task {
            id,
            owner_id: p.user_id.clone(),
            parent_id: input.parent_id,
            project_id: input.project_id,
            title,
            notes,
            status: Status::Todo,
            priority: input.priority.unwrap_or(Priority::None),
            due_date,
            created_at: self.now,
            updated_at: self.now,
            completed_at: None,
        };
        self.repo.insert(&task).await.map_err(backend)?;
        Ok(task)
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: TaskPatch,
    ) -> Result<Task, TaskError> {
        let mut task = self.get(p, id).await?;
        if let Some(t) = patch.title {
            task.title = field("title", validate::line(&t, MAX_TITLE))?;
        }
        if let Some(n) = patch.notes {
            task.notes = field("notes", validate::text(&n, MAX_NOTES))?;
        }
        if let Some(pr) = patch.priority {
            task.priority = pr;
        }
        if let Some(due) = patch.due_date {
            task.due_date = due
                .as_deref()
                .map(validate::date)
                .transpose()
                .map_err(|w| TaskError::Invalid("due_date", w))?;
        }
        if let Some(project) = patch.project_id {
            if task.parent_id.is_some() {
                return Err(TaskError::Invalid(
                    "project_id",
                    "subtasks use their parent's project",
                ));
            }
            if let Some(id) = &project {
                self.check_project(p, id).await?;
            }
            task.project_id = project;
        }
        if let Some(status) = patch.status {
            if status != task.status {
                task.completed_at = (!status.is_open()).then_some(self.now);
                task.status = status;
            }
        }
        task.updated_at = self.now;
        if !self.repo.update(&task).await.map_err(backend)? {
            return Err(TaskError::NotFound);
        }
        Ok(task)
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), TaskError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(TaskError::NotFound)
        }
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::cell::RefCell;

    /// Tasks, plus the `(owner_id, project_id)` pairs that exist.
    #[derive(Default)]
    pub struct MemoryTasks(pub RefCell<Vec<Task>>, pub RefCell<Vec<(String, String)>>);

    impl TaskRepository for MemoryTasks {
        async fn list(
            &self,
            owner_id: &str,
            filter: Filter,
            limit: u32,
        ) -> Result<Vec<Task>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|t| t.owner_id == owner_id)
                .filter(|t| match filter {
                    Filter::All => true,
                    Filter::Open => t.status.is_open(),
                    Filter::Closed => !t.status.is_open(),
                })
                .take(limit as usize)
                .cloned()
                .collect())
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Task>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|t| t.owner_id == owner_id && t.id == id)
                .cloned())
        }
        async fn insert(&self, task: &Task) -> Result<(), String> {
            self.0.borrow_mut().push(task.clone());
            Ok(())
        }
        async fn update(&self, task: &Task) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            match all
                .iter_mut()
                .find(|t| t.owner_id == task.owner_id && t.id == task.id)
            {
                Some(t) => {
                    *t = task.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|t| {
                !(t.owner_id == owner_id && (t.id == id || t.parent_id.as_deref() == Some(id)))
            });
            Ok(all.len() < before)
        }
        async fn project_exists(&self, owner_id: &str, project_id: &str) -> Result<bool, String> {
            Ok(self
                .1
                .borrow()
                .iter()
                .any(|(o, p)| o == owner_id && p == project_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryTasks;
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

    fn new(title: &str) -> NewTask {
        NewTask {
            id: None,
            title: title.into(),
            notes: None,
            priority: None,
            due_date: None,
            parent_id: None,
            project_id: None,
        }
    }

    #[test]
    fn project_assignment_is_owner_checked_and_top_level_only() {
        let f = F::new();
        let me = owner("u1");
        let project = "a".repeat(32);
        f.repo.1.borrow_mut().push(("u1".into(), project.clone()));
        let svc = f.at(1);
        let in_project = |title: &str| NewTask {
            project_id: Some(project.clone()),
            ..new(title)
        };

        let t = block_on(svc.create(&me, in_project("In project"))).unwrap();
        assert_eq!(t.project_id.as_deref(), Some(project.as_str()));
        assert_eq!(
            block_on(svc.create(&owner("u2"), in_project("Other owner"))).unwrap_err(),
            TaskError::Invalid("project_id", "not found")
        );
        let sub_with_project = NewTask {
            parent_id: Some(t.id.clone()),
            ..in_project("Sub")
        };
        assert!(matches!(
            block_on(svc.create(&me, sub_with_project)),
            Err(TaskError::Invalid("project_id", _))
        ));
        let sub = block_on(svc.create(
            &me,
            NewTask {
                parent_id: Some(t.id.clone()),
                ..new("Sub")
            },
        ))
        .unwrap();
        let assign: TaskPatch =
            serde_json::from_str(&format!(r#"{{"project_id":"{project}"}}"#)).unwrap();
        assert!(matches!(
            block_on(svc.update(&me, &sub.id, assign)),
            Err(TaskError::Invalid("project_id", _))
        ));

        let clear: TaskPatch = serde_json::from_str(r#"{"project_id":null}"#).unwrap();
        assert_eq!(
            block_on(svc.update(&me, &t.id, clear)).unwrap().project_id,
            None
        );
        let bogus: TaskPatch = serde_json::from_str(r#"{"project_id":"nope"}"#).unwrap();
        assert_eq!(
            block_on(svc.update(&me, &t.id, bogus)).unwrap_err(),
            TaskError::Invalid("project_id", "not found")
        );
    }

    struct F {
        repo: MemoryTasks,
        e: CountingEntropy,
    }

    impl F {
        fn new() -> Self {
            Self {
                repo: MemoryTasks::default(),
                e: CountingEntropy::default(),
            }
        }
        fn at(&self, now: i64) -> TaskService<'_, MemoryTasks, CountingEntropy> {
            TaskService::new(&self.repo, &self.e, now)
        }
    }

    #[test]
    fn create_defaults_and_validation() {
        let f = F::new();
        let me = owner("u1");
        let t = block_on(f.at(10).create(
            &me,
            NewTask {
                due_date: Some("2026-11-01".into()),
                ..new("  Write report ")
            },
        ))
        .unwrap();
        assert_eq!(
            (t.title.as_str(), t.status, t.priority),
            ("Write report", Status::Todo, Priority::None)
        );
        assert_eq!((t.created_at, t.updated_at, t.completed_at), (10, 10, None));
        assert!(is_id(&t.id));

        let svc = f.at(10);
        assert_eq!(
            block_on(svc.create(&me, new(" "))).unwrap_err(),
            TaskError::Invalid("title", "required")
        );
        assert_eq!(
            block_on(svc.create(
                &me,
                NewTask {
                    due_date: Some("2026-02-30".into()),
                    ..new("x")
                }
            ))
            .unwrap_err(),
            TaskError::Invalid("due_date", "invalid date")
        );
        assert!(matches!(
            block_on(svc.create(&me, new(&"x".repeat(MAX_TITLE + 1)))),
            Err(TaskError::Invalid("title", _))
        ));
    }

    #[test]
    fn subtasks_are_one_level_and_owner_scoped() {
        let f = F::new();
        let (me, other) = (owner("u1"), owner("u2"));
        let svc = f.at(1);
        let parent = block_on(svc.create(&me, new("Parent"))).unwrap();
        let child = block_on(svc.create(
            &me,
            NewTask {
                parent_id: Some(parent.id.clone()),
                ..new("Child")
            },
        ))
        .unwrap();
        assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(
            block_on(svc.create(
                &me,
                NewTask {
                    parent_id: Some(child.id.clone()),
                    ..new("Grandchild")
                }
            ))
            .unwrap_err(),
            TaskError::Invalid("parent_id", "subtasks cannot have subtasks")
        );
        // Another owner cannot attach to, read, or delete my task.
        assert_eq!(
            block_on(svc.create(
                &other,
                NewTask {
                    parent_id: Some(parent.id.clone()),
                    ..new("Sneaky")
                }
            ))
            .unwrap_err(),
            TaskError::Invalid("parent_id", "not found")
        );
        assert_eq!(
            block_on(svc.get(&other, &parent.id)).unwrap_err(),
            TaskError::NotFound
        );
        assert_eq!(
            block_on(svc.delete(&other, &parent.id)).unwrap_err(),
            TaskError::NotFound
        );
        assert!(block_on(svc.list(&other, Filter::All)).unwrap().is_empty());

        // Deleting the parent removes its subtasks.
        block_on(svc.delete(&me, &parent.id)).unwrap();
        assert!(block_on(svc.list(&me, Filter::All)).unwrap().is_empty());
    }

    #[test]
    fn status_changes_track_completion() {
        let f = F::new();
        let me = owner("u1");
        let t = block_on(f.at(1).create(&me, new("Task"))).unwrap();
        let done = block_on(f.at(5).update(
            &me,
            &t.id,
            TaskPatch {
                status: Some(Status::Done),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!((done.completed_at, done.updated_at), (Some(5), 5));
        assert_eq!(block_on(f.at(5).list(&me, Filter::Open)).unwrap().len(), 0);
        assert_eq!(
            block_on(f.at(5).list(&me, Filter::Closed)).unwrap().len(),
            1
        );

        // Re-saving the same status keeps the original completion time.
        let same = block_on(f.at(9).update(
            &me,
            &t.id,
            TaskPatch {
                status: Some(Status::Done),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(same.completed_at, Some(5));

        let reopened = block_on(f.at(7).update(
            &me,
            &t.id,
            TaskPatch {
                status: Some(Status::InProgress),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(reopened.completed_at, None);
    }

    #[test]
    fn patch_updates_and_clears_fields() {
        let f = F::new();
        let me = owner("u1");
        let t = block_on(f.at(1).create(
            &me,
            NewTask {
                due_date: Some("2026-01-01".into()),
                ..new("Old")
            },
        ))
        .unwrap();
        let patch: TaskPatch = serde_json::from_str(
            r#"{"title":"New","notes":"line\nline","priority":"high","due_date":null}"#,
        )
        .unwrap();
        let u = block_on(f.at(2).update(&me, &t.id, patch)).unwrap();
        assert_eq!(
            (u.title.as_str(), u.notes.as_str(), u.priority, u.due_date),
            ("New", "line\nline", Priority::High, None)
        );

        let untouched = block_on(f.at(3).update(&me, &t.id, TaskPatch::default())).unwrap();
        assert_eq!(untouched.title, "New");
        assert_eq!(
            block_on(f.at(3).update(&me, "nope", TaskPatch::default())).unwrap_err(),
            TaskError::NotFound
        );
    }

    #[test]
    fn wire_format() {
        let f = F::new();
        let t = block_on(f.at(1).create(&owner("u1"), new("T"))).unwrap();
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["status"], "todo");
        assert_eq!(json["priority"], "none");
        assert!(json.get("owner_id").is_none());
        assert_eq!(Status::parse("in_progress"), Some(Status::InProgress));
        assert_eq!(Priority::parse("high").map(Priority::as_str), Some("high"));
        assert_eq!(Filter::parse("open"), Some(Filter::Open));
    }

    #[test]
    fn client_ids_make_offline_creates_idempotent() {
        let f = F::new();
        let me = owner("u1");
        let id = "c".repeat(32);
        let t = block_on(f.at(1).create(
            &me,
            NewTask {
                id: Some(id.clone()),
                ..new("Offline")
            },
        ))
        .unwrap();
        assert_eq!(t.id, id);
        assert_eq!(
            block_on(f.at(2).create(
                &me,
                NewTask {
                    id: Some(id.clone()),
                    ..new("Replay")
                }
            ))
            .unwrap_err(),
            TaskError::Exists
        );
        assert!(matches!(
            block_on(f.at(2).create(
                &me,
                NewTask {
                    id: Some("nope".into()),
                    ..new("Bad")
                }
            )),
            Err(TaskError::Invalid("id", _))
        ));
        assert_eq!(f.repo.0.borrow().len(), 1);
    }
}
