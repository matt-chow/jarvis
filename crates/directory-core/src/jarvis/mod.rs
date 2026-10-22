//! JARVIS command layer (DECISIONS D-016).
//!
//! Text becomes typed `Command`s through an `IntentProvider` (today the
//! deterministic parser; later optionally an AI provider). Commands are
//! resolved into a `Plan` that the user previews. Read-only plans run at once;
//! anything that writes waits for approval and runs exactly once. Every
//! command executes through the owning module's service as the caller.

pub mod parser;
pub mod presets;

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::inbox::{Capture, InboxError, InboxRepository, InboxService};
use crate::journal::{JournalError, JournalRepository, JournalService};
use crate::links::{EntityKind, EntityRef};
use crate::notes::{NewNote, NoteError, NoteRepository, NoteService};
use crate::tasks::{
    self, Filter, NewTask, Status, TaskError, TaskPatch, TaskRepository, TaskService,
};
use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_INPUT: usize = 1_000;
/// How long a pending plan can be approved.
pub const PLAN_TTL_MS: i64 = 10 * 60 * 1000;
pub const HISTORY_LIMIT: u32 = 50;
const LIST_PREVIEW: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    CreateTask {
        title: String,
        due_date: Option<String>,
    },
    /// `task_id`/`title` are filled in when the plan is resolved.
    CompleteTask {
        query: String,
        task_id: Option<String>,
        title: Option<String>,
    },
    CreateNote {
        body: String,
    },
    AppendJournal {
        date: String,
        text: String,
    },
    Capture {
        text: String,
    },
    ListTasks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Read,
    Write,
    Destructive,
}

fn quote(s: &str, max: usize) -> String {
    let first = s.lines().next().unwrap_or("");
    let short: String = first.chars().take(max).collect();
    if short.chars().count() < s.chars().count() {
        format!("“{short}…”")
    } else {
        format!("“{short}”")
    }
}

impl Command {
    pub fn risk(&self) -> Risk {
        match self {
            Self::ListTasks => Risk::Read,
            _ => Risk::Write,
        }
    }

    /// One-line human description for previews and history.
    pub fn summary(&self) -> String {
        match self {
            Self::CreateTask {
                title,
                due_date: Some(d),
            } => format!("Create task {} due {d}", quote(title, 80)),
            Self::CreateTask {
                title,
                due_date: None,
            } => format!("Create task {}", quote(title, 80)),
            Self::CompleteTask { title: Some(t), .. } => format!("Complete task {}", quote(t, 80)),
            Self::CompleteTask { query, .. } => {
                format!("Complete the task matching {}", quote(query, 80))
            }
            Self::CreateNote { body } => format!("Create note {}", quote(body, 60)),
            Self::AppendJournal { date, text } => {
                format!("Add to journal for {date}: {}", quote(text, 60))
            }
            Self::Capture { text } => format!("Capture to inbox: {}", quote(text, 60)),
            Self::ListTasks => "List open tasks".into(),
        }
    }
}

/// Turns text into commands. Implemented by the deterministic parser now; an
/// AI provider can implement it later without changing execution or approval.
pub trait IntentProvider {
    async fn interpret(&self, input: &str, today: &str) -> Result<Vec<Command>, JarvisError>;
}

pub struct DeterministicParser;

impl IntentProvider for DeterministicParser {
    async fn interpret(&self, input: &str, today: &str) -> Result<Vec<Command>, JarvisError> {
        parser::parse(input, today).map_err(JarvisError::Parse)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedAction {
    pub command: Command,
    pub summary: String,
    pub risk: Risk,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    /// The item created or changed, for linking in the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<EntityRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Awaiting approval.
    Pending,
    /// Claimed for execution (guards against running twice).
    Running,
    Executed,
    /// Executed, but at least one action failed (later actions were skipped).
    Failed,
    Cancelled,
    /// Pending past its expiry; can no longer run.
    Expired,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Executed => "executed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "executed" => Self::Executed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "expired" => Self::Expired,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub input: String,
    pub actions: Vec<PlannedAction>,
    pub status: PlanStatus,
    pub requires_approval: bool,
    pub created_at: i64,
    pub expires_at: i64,
    pub results: Vec<ActionResult>,
}

impl Plan {
    /// Status as of `now` (pending plans past expiry read as expired).
    pub fn effective(mut self, now: i64) -> Self {
        if self.status == PlanStatus::Pending && self.expires_at <= now {
            self.status = PlanStatus::Expired;
        }
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanRequest {
    pub input: String,
    /// The user's local date (`YYYY-MM-DD`), for "today", "tomorrow", weekdays.
    pub today: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JarvisError {
    Parse(parser::ParseError),
    Invalid(&'static str, &'static str),
    /// A command couldn't be resolved (e.g. no or several matching tasks).
    Unresolved(String),
    NotFound,
    /// The plan is not pending (already run, cancelled, or expired).
    NotPending(PlanStatus),
    Backend(String),
}

pub trait JarvisRepository {
    async fn insert(&self, plan: &Plan) -> Result<(), String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Plan>, String>;
    /// Atomically move `pending` (and unexpired) → `running`; `false` otherwise.
    async fn claim(&self, owner_id: &str, id: &str, now: i64) -> Result<bool, String>;
    async fn finish(
        &self,
        owner_id: &str,
        id: &str,
        status: PlanStatus,
        results: &[ActionResult],
    ) -> Result<(), String>;
    /// Move `pending` → `cancelled`; `false` if it wasn't pending.
    async fn cancel(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    async fn history(&self, owner_id: &str, limit: u32) -> Result<Vec<Plan>, String>;
}

/// Module repositories commands execute through.
pub struct CommandTargets<'a, T, N, J, I> {
    pub tasks: &'a T,
    pub notes: &'a N,
    pub journal: &'a J,
    pub inbox: &'a I,
}

fn backend(e: String) -> JarvisError {
    JarvisError::Backend(e)
}

fn failure(message: String) -> ActionResult {
    ActionResult {
        ok: false,
        message,
        entity: None,
    }
}

fn task_failure(e: TaskError) -> ActionResult {
    failure(match e {
        TaskError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        TaskError::NotFound => "That task no longer exists.".into(),
        TaskError::Backend(_) => "The task service is unavailable.".into(),
    })
}

fn note_failure(e: NoteError) -> ActionResult {
    failure(match e {
        NoteError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        _ => "The notes service is unavailable.".into(),
    })
}

fn journal_failure(e: JournalError) -> ActionResult {
    failure(match e {
        JournalError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        JournalError::Conflict(_) => "The journal changed at the same time; try again.".into(),
        _ => "The journal service is unavailable.".into(),
    })
}

fn inbox_failure(e: InboxError) -> ActionResult {
    failure(match e {
        InboxError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        _ => "The inbox is unavailable.".into(),
    })
}

pub struct JarvisService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: JarvisRepository, E: Entropy + ?Sized> JarvisService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    /// Interpret and resolve `req` into a plan. Read-only plans execute now.
    pub async fn plan<P, T, N, J, I>(
        &self,
        p: &Principal,
        provider: &P,
        req: PlanRequest,
        targets: CommandTargets<'_, T, N, J, I>,
    ) -> Result<Plan, JarvisError>
    where
        P: IntentProvider,
        T: TaskRepository,
        N: NoteRepository,
        J: JournalRepository,
        I: InboxRepository,
    {
        let input = req.input.trim();
        if input.chars().count() > MAX_INPUT {
            return Err(JarvisError::Invalid("input", "too long"));
        }
        let today = validate::date(&req.today).map_err(|w| JarvisError::Invalid("today", w))?;
        let commands = provider.interpret(input, &today).await?;
        let mut actions = Vec::with_capacity(commands.len());
        for command in commands {
            let command = self.resolve(p, command, targets.tasks).await?;
            actions.push(PlannedAction {
                summary: command.summary(),
                risk: command.risk(),
                command,
            });
        }
        let requires_approval = actions.iter().any(|a| a.risk > Risk::Read);
        let mut plan = Plan {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            input: input.to_owned(),
            actions,
            status: PlanStatus::Pending,
            requires_approval,
            created_at: self.now,
            expires_at: self.now + PLAN_TTL_MS,
            results: vec![],
        };
        if !requires_approval {
            let (status, results) = self.run(p, &plan.actions, &targets).await;
            plan.status = status;
            plan.results = results;
        }
        self.repo.insert(&plan).await.map_err(backend)?;
        Ok(plan)
    }

    /// Fill in references the user described by name.
    async fn resolve<T: TaskRepository>(
        &self,
        p: &Principal,
        command: Command,
        tasks: &T,
    ) -> Result<Command, JarvisError> {
        let Command::CompleteTask { query, .. } = command else {
            return Ok(command);
        };
        let open = TaskService::new(tasks, self.entropy, self.now)
            .list(p, Filter::Open)
            .await
            .map_err(|e| match e {
                TaskError::Backend(b) => JarvisError::Backend(b),
                _ => JarvisError::Unresolved("Couldn't read your tasks.".into()),
            })?;
        let needle = query.to_lowercase();
        let exact: Vec<_> = open
            .iter()
            .filter(|t| t.title.to_lowercase() == needle)
            .collect();
        let matches: Vec<_> = if exact.len() == 1 {
            exact
        } else {
            open.iter()
                .filter(|t| t.title.to_lowercase().contains(&needle))
                .collect()
        };
        match matches.as_slice() {
            [] => Err(JarvisError::Unresolved(format!(
                "No open task matches “{query}”."
            ))),
            [t] => Ok(Command::CompleteTask {
                query,
                task_id: Some(t.id.clone()),
                title: Some(t.title.clone()),
            }),
            many => {
                let names: Vec<String> = many
                    .iter()
                    .take(5)
                    .map(|t| format!("“{}”", t.title))
                    .collect();
                Err(JarvisError::Unresolved(format!(
                    "“{query}” matches {} tasks: {}. Be more specific.",
                    many.len(),
                    names.join(", ")
                )))
            }
        }
    }

    /// Approve and run a pending plan (exactly once).
    pub async fn execute<T, N, J, I>(
        &self,
        p: &Principal,
        id: &str,
        targets: CommandTargets<'_, T, N, J, I>,
    ) -> Result<Plan, JarvisError>
    where
        T: TaskRepository,
        N: NoteRepository,
        J: JournalRepository,
        I: InboxRepository,
    {
        let plan = self.load(p, id).await?;
        if plan.status != PlanStatus::Pending {
            return Err(JarvisError::NotPending(plan.status));
        }
        if !self
            .repo
            .claim(&p.user_id, id, self.now)
            .await
            .map_err(backend)?
        {
            // Raced with another approval, or just expired.
            let current = self.load(p, id).await?;
            return Err(JarvisError::NotPending(current.status));
        }
        let (status, results) = self.run(p, &plan.actions, &targets).await;
        self.repo
            .finish(&p.user_id, id, status, &results)
            .await
            .map_err(backend)?;
        Ok(Plan {
            status,
            results,
            ..plan
        })
    }

    pub async fn cancel(&self, p: &Principal, id: &str) -> Result<Plan, JarvisError> {
        let plan = self.load(p, id).await?;
        if plan.status != PlanStatus::Pending
            || !self.repo.cancel(&p.user_id, id).await.map_err(backend)?
        {
            let current = self.load(p, id).await?;
            return Err(JarvisError::NotPending(current.status));
        }
        Ok(Plan {
            status: PlanStatus::Cancelled,
            ..plan
        })
    }

    pub async fn history(&self, p: &Principal) -> Result<Vec<Plan>, JarvisError> {
        let plans = self
            .repo
            .history(&p.user_id, HISTORY_LIMIT)
            .await
            .map_err(backend)?;
        Ok(plans.into_iter().map(|pl| pl.effective(self.now)).collect())
    }

    async fn load(&self, p: &Principal, id: &str) -> Result<Plan, JarvisError> {
        if !is_id(id) {
            return Err(JarvisError::NotFound);
        }
        let plan = self
            .repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(JarvisError::NotFound)?;
        Ok(plan.effective(self.now))
    }

    /// Run actions in order; stop at the first failure.
    async fn run<T, N, J, I>(
        &self,
        p: &Principal,
        actions: &[PlannedAction],
        t: &CommandTargets<'_, T, N, J, I>,
    ) -> (PlanStatus, Vec<ActionResult>)
    where
        T: TaskRepository,
        N: NoteRepository,
        J: JournalRepository,
        I: InboxRepository,
    {
        let mut results = Vec::with_capacity(actions.len());
        for action in actions {
            let result = self.run_one(p, &action.command, t).await;
            let ok = result.ok;
            results.push(result);
            if !ok {
                return (PlanStatus::Failed, results);
            }
        }
        (PlanStatus::Executed, results)
    }

    async fn run_one<T, N, J, I>(
        &self,
        p: &Principal,
        command: &Command,
        t: &CommandTargets<'_, T, N, J, I>,
    ) -> ActionResult
    where
        T: TaskRepository,
        N: NoteRepository,
        J: JournalRepository,
        I: InboxRepository,
    {
        let tasks = TaskService::new(t.tasks, self.entropy, self.now);
        let entity = |kind, id: &str| {
            Some(EntityRef {
                kind,
                id: id.to_owned(),
            })
        };
        match command {
            Command::CreateTask { title, due_date } => {
                let input = NewTask {
                    title: title.clone(),
                    notes: None,
                    priority: None,
                    due_date: due_date.clone(),
                    parent_id: None,
                    project_id: None,
                };
                match tasks.create(p, input).await {
                    Ok(task) => ActionResult {
                        ok: true,
                        message: format!("Created task “{}”.", task.title),
                        entity: entity(EntityKind::Task, &task.id),
                    },
                    Err(e) => task_failure(e),
                }
            }
            Command::CompleteTask {
                task_id: Some(id), ..
            } => {
                let patch = TaskPatch {
                    status: Some(Status::Done),
                    ..Default::default()
                };
                match tasks.update(p, id, patch).await {
                    Ok(task) => ActionResult {
                        ok: true,
                        message: format!("Completed “{}”.", task.title),
                        entity: entity(EntityKind::Task, &task.id),
                    },
                    Err(e) => task_failure(e),
                }
            }
            Command::CompleteTask {
                task_id: None,
                query,
                ..
            } => failure(format!("No task was resolved for “{query}”.")),
            Command::CreateNote { body } => {
                let input = NewNote {
                    body: Some(body.clone()),
                    ..Default::default()
                };
                match NoteService::new(t.notes, self.entropy, self.now)
                    .create(p, input)
                    .await
                {
                    Ok(note) => ActionResult {
                        ok: true,
                        message: "Created a note.".into(),
                        entity: entity(EntityKind::Note, &note.id),
                    },
                    Err(e) => note_failure(e),
                }
            }
            Command::AppendJournal { date, text } => match JournalService::new(t.journal, self.now)
                .append(p, date, text)
                .await
            {
                Ok(e) => ActionResult {
                    ok: true,
                    message: format!("Added to the journal for {}.", e.date),
                    entity: entity(EntityKind::Journal, &e.date),
                },
                Err(e) => journal_failure(e),
            },
            Command::Capture { text } => match InboxService::new(t.inbox, self.entropy, self.now)
                .capture(p, Capture { text: text.clone() })
                .await
            {
                Ok(_) => ActionResult {
                    ok: true,
                    message: "Captured to your inbox.".into(),
                    entity: None,
                },
                Err(e) => inbox_failure(e),
            },
            Command::ListTasks => match tasks.list(p, Filter::Open).await {
                Ok(open) => {
                    let mut top: Vec<&tasks::Task> =
                        open.iter().filter(|t| t.parent_id.is_none()).collect();
                    top.sort_by(|a, b| {
                        a.due_date
                            .is_none()
                            .cmp(&b.due_date.is_none())
                            .then(a.due_date.cmp(&b.due_date))
                    });
                    let message = match top.len() {
                        0 => "Nothing open. Nice.".into(),
                        n => {
                            let names: Vec<String> = top
                                .iter()
                                .take(LIST_PREVIEW)
                                .map(|t| match &t.due_date {
                                    Some(d) => format!("{} (due {d})", t.title),
                                    None => t.title.clone(),
                                })
                                .collect();
                            let more = if n > LIST_PREVIEW {
                                format!(", and {} more", n - LIST_PREVIEW)
                            } else {
                                String::new()
                            };
                            format!("{n} open: {}{more}.", names.join("; "))
                        }
                    };
                    ActionResult {
                        ok: true,
                        message,
                        entity: None,
                    }
                }
                Err(e) => task_failure(e),
            },
        }
    }
}

#[cfg(test)]
mod tests;
