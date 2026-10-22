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
use crate::projects::{NewProject, ProjectError, ProjectRepository, ProjectService};
use crate::search::{self, SearchError, SearchSource};
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
        /// Project named by the user ("… in project Garden").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
        /// Filled in when the plan is resolved.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<String>,
    },
    CreateProject {
        name: String,
    },
    ListProjects,
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
    Find {
        query: String,
    },
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
            Self::ListTasks | Self::ListProjects | Self::Find { .. } => Risk::Read,
            _ => Risk::Write,
        }
    }

    /// One-line human description for previews and history.
    pub fn summary(&self) -> String {
        match self {
            Self::CreateTask {
                title,
                due_date,
                project,
                ..
            } => {
                let mut s = format!("Create task {}", quote(title, 80));
                if let Some(p) = project {
                    s.push_str(&format!(" in project {}", quote(p, 60)));
                }
                if let Some(d) = due_date {
                    s.push_str(&format!(" due {d}"));
                }
                s
            }
            Self::CreateProject { name } => format!("Create project {}", quote(name, 80)),
            Self::ListProjects => "List active projects".into(),
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
            Self::Find { query } => format!("Search for {}", quote(query, 60)),
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
pub struct CommandTargets<'a, T, N, J, I, S, Pj> {
    pub tasks: &'a T,
    pub notes: &'a N,
    pub journal: &'a J,
    pub inbox: &'a I,
    pub search: &'a S,
    pub projects: &'a Pj,
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
        TaskError::NotFound | TaskError::Exists => "That task no longer exists.".into(),
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

fn project_failure(e: ProjectError) -> ActionResult {
    failure(match e {
        ProjectError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        _ => "The projects service is unavailable.".into(),
    })
}

/// Pick the one item whose name matches `query`: an exact (case-insensitive)
/// match wins, otherwise a unique substring match.
fn pick<'t, X>(
    items: &'t [X],
    name: impl Fn(&X) -> &str,
    query: &str,
) -> Result<&'t X, Vec<&'t X>> {
    let needle = query.to_lowercase();
    let exact: Vec<&X> = items
        .iter()
        .filter(|x| name(x).to_lowercase() == needle)
        .collect();
    let matches = if exact.len() == 1 {
        exact
    } else {
        items
            .iter()
            .filter(|x| name(x).to_lowercase().contains(&needle))
            .collect()
    };
    match matches.as_slice() {
        [one] => Ok(one),
        _ => Err(matches),
    }
}

/// The name of a project an earlier step creates, if it matches `query` exactly.
fn created_earlier(earlier: &[PlannedAction], query: &str) -> Option<String> {
    earlier.iter().find_map(|a| match &a.command {
        Command::CreateProject { name } if name.trim().eq_ignore_ascii_case(query.trim()) => {
            Some(name.trim().to_owned())
        }
        _ => None,
    })
}

fn ambiguity<X>(query: &str, what: &str, matches: &[&X], name: impl Fn(&X) -> &str) -> JarvisError {
    if matches.is_empty() {
        return JarvisError::Unresolved(format!("No {what} matches “{query}”."));
    }
    let names: Vec<String> = matches
        .iter()
        .take(5)
        .map(|x| format!("“{}”", name(x)))
        .collect();
    JarvisError::Unresolved(format!(
        "“{query}” matches {} {what}s: {}. Be more specific.",
        matches.len(),
        names.join(", ")
    ))
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
    pub async fn plan<P, T, N, J, I, S, Pj>(
        &self,
        p: &Principal,
        provider: &P,
        req: PlanRequest,
        targets: CommandTargets<'_, T, N, J, I, S, Pj>,
    ) -> Result<Plan, JarvisError>
    where
        P: IntentProvider,
        T: TaskRepository,
        N: NoteRepository,
        J: JournalRepository,
        I: InboxRepository,
        S: SearchSource,
        Pj: ProjectRepository,
    {
        let input = req.input.trim();
        if input.chars().count() > MAX_INPUT {
            return Err(JarvisError::Invalid("input", "too long"));
        }
        let today = validate::date(&req.today).map_err(|w| JarvisError::Invalid("today", w))?;
        let commands = provider.interpret(input, &today).await?;
        let mut actions = Vec::with_capacity(commands.len());
        for command in commands {
            let command = self
                .resolve(p, command, &actions, targets.tasks, targets.projects)
                .await?;
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
    async fn resolve<T: TaskRepository, Pj: ProjectRepository>(
        &self,
        p: &Principal,
        command: Command,
        earlier: &[PlannedAction],
        tasks: &T,
        projects: &Pj,
    ) -> Result<Command, JarvisError> {
        match command {
            Command::CompleteTask { query, .. } => {
                let open = TaskService::new(tasks, self.entropy, self.now)
                    .list(p, Filter::Open)
                    .await
                    .map_err(|e| match e {
                        TaskError::Backend(b) => JarvisError::Backend(b),
                        _ => JarvisError::Unresolved("Couldn't read your tasks.".into()),
                    })?;
                let t = pick(&open, |t| &t.title, &query)
                    .map_err(|m| ambiguity(&query, "open task", &m, |t| &t.title))?;
                Ok(Command::CompleteTask {
                    task_id: Some(t.id.clone()),
                    title: Some(t.title.clone()),
                    query,
                })
            }
            Command::CreateTask {
                title,
                due_date,
                project: Some(query),
                ..
            } => {
                let active = ProjectService::new(projects, self.entropy, self.now)
                    .list(p, false)
                    .await
                    .map_err(|e| match e {
                        ProjectError::Backend(b) => JarvisError::Backend(b),
                        _ => JarvisError::Unresolved("Couldn't read your projects.".into()),
                    })?;
                match pick(&active, |x| &x.name, &query) {
                    Ok(pr) => Ok(Command::CreateTask {
                        title,
                        due_date,
                        project: Some(pr.name.clone()),
                        project_id: Some(pr.id.clone()),
                    }),
                    // Created by an earlier step of this plan: look it up when it runs.
                    Err(m) if m.is_empty() && created_earlier(earlier, &query).is_some() => {
                        Ok(Command::CreateTask {
                            title,
                            due_date,
                            project: created_earlier(earlier, &query),
                            project_id: None,
                        })
                    }
                    Err(m) => Err(ambiguity(&query, "active project", &m, |x| &x.name)),
                }
            }
            other => Ok(other),
        }
    }

    /// Approve and run a pending plan (exactly once).
    pub async fn execute<T, N, J, I, S, Pj>(
        &self,
        p: &Principal,
        id: &str,
        targets: CommandTargets<'_, T, N, J, I, S, Pj>,
    ) -> Result<Plan, JarvisError>
    where
        T: TaskRepository,
        N: NoteRepository,
        J: JournalRepository,
        I: InboxRepository,
        S: SearchSource,
        Pj: ProjectRepository,
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
    async fn run<T, N, J, I, S, Pj>(
        &self,
        p: &Principal,
        actions: &[PlannedAction],
        t: &CommandTargets<'_, T, N, J, I, S, Pj>,
    ) -> (PlanStatus, Vec<ActionResult>)
    where
        T: TaskRepository,
        N: NoteRepository,
        J: JournalRepository,
        I: InboxRepository,
        S: SearchSource,
        Pj: ProjectRepository,
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

    async fn run_one<T, N, J, I, S, Pj>(
        &self,
        p: &Principal,
        command: &Command,
        t: &CommandTargets<'_, T, N, J, I, S, Pj>,
    ) -> ActionResult
    where
        T: TaskRepository,
        N: NoteRepository,
        J: JournalRepository,
        I: InboxRepository,
        S: SearchSource,
        Pj: ProjectRepository,
    {
        let tasks = TaskService::new(t.tasks, self.entropy, self.now);
        let entity = |kind, id: &str| {
            Some(EntityRef {
                kind,
                id: id.to_owned(),
            })
        };
        match command {
            Command::CreateTask {
                title,
                due_date,
                project,
                project_id,
            } => {
                // A project created earlier in this plan is looked up by its exact name now.
                let project_id = match (project, project_id) {
                    (Some(name), None) => {
                        let active = match ProjectService::new(t.projects, self.entropy, self.now)
                            .list(p, false)
                            .await
                        {
                            Ok(a) => a,
                            Err(e) => return project_failure(e),
                        };
                        match active.iter().find(|x| x.name.eq_ignore_ascii_case(name)) {
                            Some(x) => Some(x.id.clone()),
                            None => return failure(format!("Project “{name}” wasn't found.")),
                        }
                    }
                    (_, id) => id.clone(),
                };
                let input = NewTask {
                    id: None,
                    title: title.clone(),
                    notes: None,
                    priority: None,
                    due_date: due_date.clone(),
                    parent_id: None,
                    project_id: project_id.clone(),
                };
                match tasks.create(p, input).await {
                    Ok(task) => ActionResult {
                        ok: true,
                        message: match project {
                            Some(name) => format!("Created task “{}” in “{name}”.", task.title),
                            None => format!("Created task “{}”.", task.title),
                        },
                        entity: entity(EntityKind::Task, &task.id),
                    },
                    Err(e) => task_failure(e),
                }
            }
            Command::CreateProject { name } => {
                let input = NewProject {
                    id: None,
                    name: name.clone(),
                    description: None,
                };
                match ProjectService::new(t.projects, self.entropy, self.now)
                    .create(p, input)
                    .await
                {
                    Ok(pr) => ActionResult {
                        ok: true,
                        message: format!("Created project “{}”.", pr.name),
                        entity: entity(EntityKind::Project, &pr.id),
                    },
                    Err(e) => project_failure(e),
                }
            }
            Command::ListProjects => {
                let active = match ProjectService::new(t.projects, self.entropy, self.now)
                    .list(p, false)
                    .await
                {
                    Ok(a) => a,
                    Err(e) => return project_failure(e),
                };
                let all = match tasks.list(p, Filter::All).await {
                    Ok(all) => all,
                    Err(e) => return task_failure(e),
                };
                let message = if active.is_empty() {
                    "No active projects.".into()
                } else {
                    let names: Vec<String> = active
                        .iter()
                        .take(LIST_PREVIEW)
                        .map(|pr| {
                            // Same rule as the Projects screen: top-level, not cancelled.
                            let mine = all.iter().filter(|t| {
                                t.project_id.as_deref() == Some(pr.id.as_str())
                                    && t.parent_id.is_none()
                                    && t.status != Status::Cancelled
                            });
                            let (done, total) = mine.fold((0, 0), |(d, n), t| {
                                (d + usize::from(t.status == Status::Done), n + 1)
                            });
                            format!("{} ({done}/{total} done)", pr.name)
                        })
                        .collect();
                    let more = if active.len() > LIST_PREVIEW {
                        format!(", and {} more", active.len() - LIST_PREVIEW)
                    } else {
                        String::new()
                    };
                    format!("{} active: {}{more}.", active.len(), names.join("; "))
                };
                ActionResult {
                    ok: true,
                    message,
                    entity: None,
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
                .capture(
                    p,
                    Capture {
                        id: None,
                        text: text.clone(),
                    },
                )
                .await
            {
                Ok(_) => ActionResult {
                    ok: true,
                    message: "Captured to your inbox.".into(),
                    entity: None,
                },
                Err(e) => inbox_failure(e),
            },
            Command::Find { query } => match search::search(t.search, p, query).await {
                Ok(hits) if hits.is_empty() => ActionResult {
                    ok: true,
                    message: format!("Nothing matches “{query}”."),
                    entity: None,
                },
                Ok(hits) => {
                    let names: Vec<String> = hits
                        .iter()
                        .take(LIST_PREVIEW)
                        .map(|h| format!("{} ({})", h.title, h.kind.as_str()))
                        .collect();
                    let more = if hits.len() > LIST_PREVIEW {
                        format!(", and {} more", hits.len() - LIST_PREVIEW)
                    } else {
                        String::new()
                    };
                    let top = &hits[0];
                    ActionResult {
                        ok: true,
                        message: format!("{} found: {}{more}.", hits.len(), names.join("; ")),
                        entity: entity(top.kind, &top.id),
                    }
                }
                Err(SearchError::Invalid(why)) => failure(format!("Can't search for that: {why}.")),
                Err(SearchError::Backend(_)) => failure("Search is unavailable.".into()),
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
