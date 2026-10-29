//! JARVIS command layer (DECISIONS D-016).
//!
//! Text becomes typed `Command`s through an `IntentProvider` (today the
//! deterministic parser; later optionally an AI provider). Commands are
//! resolved into a `Plan` that the user previews. Read-only plans run at once;
//! anything that writes waits for approval and runs exactly once. Every
//! command executes through the owning module's service as the caller.

pub mod agent;
pub mod ai;
pub mod chat;
pub mod conversation;
mod expand;
pub mod parser;
pub mod presets;
pub mod schedule;
pub mod undo;
pub mod voice;

use crate::blocks::BlockRepository;
use crate::calendar::{CalendarService, EventRepository, Occurrence};
use crate::entropy::{is_id, new_id, Entropy};
use crate::habits::{HabitError, HabitRepository, HabitService, HabitView};
use crate::identity::Principal;
use crate::inbox::{Capture, InboxError, InboxRepository, InboxService};
use crate::journal::{JournalError, JournalRepository, JournalService};
use crate::links::{EntityKind, EntityRef, LinkError, LinkRepository, LinkService, NewLink};
use crate::notes::{NewNote, NoteError, NoteRepository, NoteService};
use crate::planner::{self, Constraints};
use crate::preferences::PreferencesRepository;
use crate::projects::{NewProject, ProjectError, ProjectRepository, ProjectService};
use crate::search::{self, SearchError, SearchSource};
use crate::tasks::{
    self, Filter, NewTask, Priority, Status, TaskError, TaskPatch, TaskRepository, TaskService,
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
        /// Chosen when the plan is made, so later steps can refer to the task
        /// and a replayed step can't create it twice.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        title: String,
        due_date: Option<String>,
        /// Project named by the user ("… in project Garden").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
        /// Filled in when the plan is resolved.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        priority: Option<Priority>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        estimate_minutes: Option<u32>,
    },
    /// Change an open task's deadline, priority, or estimate. `task_id`/`title`
    /// are filled in when the plan is resolved.
    UpdateTask {
        query: String,
        change: TaskChange,
        #[serde(default)]
        task_id: Option<String>,
        #[serde(default)]
        title: Option<String>,
    },
    /// The planner's top tasks for `date` (D-041), optionally in one project.
    Priorities {
        date: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<ProjectScope>,
    },
    /// What fits in `minutes` of free time, by the planner's ranking.
    Suggest {
        date: String,
        minutes: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<ProjectScope>,
    },
    CreateProject {
        name: String,
    },
    ListProjects,
    /// Events, tasks due, and habits still open on `date`.
    Agenda {
        date: String,
    },
    /// Link two items; each side is resolved by name when the plan is made.
    LinkItems {
        a: LinkTarget,
        b: LinkTarget,
    },
    /// `habit_id`/`name` are filled in when the plan is resolved.
    CheckHabit {
        query: String,
        date: String,
        habit_id: Option<String>,
        name: Option<String>,
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
    Find {
        query: String,
    },
    /// Find work sessions for an open task (D-048). Becomes `CreateBlock`
    /// steps when the plan is made; `task_id` may name a task an earlier step
    /// creates.
    ScheduleTask {
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
        /// Total minutes; default: what the estimate still needs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minutes: Option<u32>,
        /// First date to use (default: today).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
        /// Last date to use (default: the due date, else two weeks).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_minutes: Option<u32>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keep: Vec<schedule::Keep>,
    },
    /// Sessions for every open task due by `by` that still needs time.
    ScheduleDue {
        by: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keep: Vec<schedule::Keep>,
    },
    /// Re-place flexible sessions on `date` (between `start` and `end`)
    /// around fixed commitments. Becomes `MoveBlock` steps.
    Rearrange {
        date: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keep: Vec<schedule::Keep>,
    },
    CreateBlock {
        /// Chosen when the plan is made (see `CreateTask::id`).
        #[serde(default)]
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
        title: String,
        date: String,
        start_time: String,
        end_time: String,
        /// Titles it overlapped when proposed; anything new overlapping it
        /// when it runs stops the step.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        overlaps: Vec<String>,
    },
    /// Move a session; `expect` is its `updated_at` when proposed.
    MoveBlock {
        block_id: String,
        #[serde(default)]
        title: String,
        date: String,
        start_time: String,
        end_time: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<i64>,
        /// As for `CreateBlock`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        overlaps: Vec<String>,
    },
    DeleteBlock {
        block_id: String,
        #[serde(default)]
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<i64>,
    },
    CreateEvent {
        #[serde(default)]
        id: String,
        title: String,
        date: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_time: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_time: Option<String>,
        /// A fixed commitment (default) or flexible time.
        #[serde(default = "yes")]
        fixed: bool,
    },
    /// Move a single (non-repeating) event.
    MoveEvent {
        event_id: String,
        #[serde(default)]
        title: String,
        date: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_time: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end_time: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<i64>,
    },
    AddSubtask {
        #[serde(default)]
        id: String,
        parent_id: String,
        #[serde(default)]
        parent_title: String,
        title: String,
    },
    /// Open tasks due by `to` whose sessions don't cover their estimate.
    Unscheduled {
        from: String,
        to: String,
    },
    FreeTime {
        from: String,
        to: String,
        /// Minutes since midnight on `from` when asked (time already gone
        /// isn't free); filled in when the plan is made.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        now: Option<u32>,
    },
    /// What was left undone on `date`: open tasks due by then, and sessions
    /// not marked done.
    Missed {
        date: String,
    },
    /// Reverse the latest change (or `plan_id`'s). Becomes `Revert` steps.
    Undo {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_id: Option<String>,
    },
    /// One recorded reversal from an executed plan.
    Revert {
        plan_id: String,
        op: undo::UndoOp,
    },
    /// Save scheduling preferences the owner asked JARVIS to keep.
    Remember {
        prefs: crate::preferences::JarvisPrefs,
    },
}

const fn yes() -> bool {
    true
}

/// A planning request limited to one project: the name the user said, and
/// the active project it resolved to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectScope {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ProjectScope {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            id: None,
            name: None,
        }
    }

    fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.query)
    }
}

/// What an `UpdateTask` changes. `due_date: Some(None)` clears the deadline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskChange {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "validate::double_option"
    )]
    pub due_date: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate_minutes: Option<u32>,
}

impl TaskChange {
    pub fn is_empty(&self) -> bool {
        self.due_date.is_none() && self.priority.is_none() && self.estimate_minutes.is_none()
    }

    /// "due 2026-10-23, high priority, 30 min"
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        match &self.due_date {
            Some(Some(d)) => parts.push(format!("due {d}")),
            Some(None) => parts.push("no due date".into()),
            None => {}
        }
        if let Some(p) = self.priority {
            parts.push(format!("{} priority", p.as_str()));
        }
        if let Some(m) = self.estimate_minutes {
            parts.push(format!("estimate {}", planner::duration(m)));
        }
        parts.join(", ")
    }
}

/// One side of a `link` command: what the user typed, and what it resolved to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkTarget {
    pub kind: EntityKind,
    /// A title to match, or a date for journal days.
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl LinkTarget {
    pub fn new(kind: EntityKind, query: impl Into<String>) -> Self {
        Self {
            kind,
            query: query.into(),
            id: None,
            title: None,
        }
    }

    fn label(&self) -> String {
        format!(
            "{} {}",
            self.kind.as_str(),
            quote(self.title.as_deref().unwrap_or(&self.query), 60)
        )
    }
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
            Self::ListTasks
            | Self::ListProjects
            | Self::Agenda { .. }
            | Self::Find { .. }
            | Self::Priorities { .. }
            | Self::Suggest { .. }
            | Self::Unscheduled { .. }
            | Self::FreeTime { .. }
            | Self::Missed { .. } => Risk::Read,
            Self::DeleteBlock { .. } => Risk::Destructive,
            Self::Revert { op, .. } if op.deletes() => Risk::Destructive,
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
                priority,
                estimate_minutes,
                ..
            } => {
                let mut s = format!("Create task {}", quote(title, 80));
                if let Some(p) = project {
                    s.push_str(&format!(" in project {}", quote(p, 60)));
                }
                if let Some(d) = due_date {
                    s.push_str(&format!(" due {d}"));
                }
                if let Some(p) = priority {
                    s.push_str(&format!(", {} priority", p.as_str()));
                }
                if let Some(m) = estimate_minutes {
                    s.push_str(&format!(", {}", planner::duration(*m)));
                }
                s
            }
            Self::UpdateTask {
                query,
                change,
                title,
                ..
            } => format!(
                "Update task {}: {}",
                quote(title.as_deref().unwrap_or(query), 80),
                change.describe()
            ),
            Self::Priorities { date, project } => match project {
                Some(pr) => format!(
                    "Show top priorities in {} for {date}",
                    quote(pr.label(), 60)
                ),
                None => format!("Show top priorities for {date}"),
            },
            Self::Suggest {
                minutes, project, ..
            } => match project {
                Some(pr) => format!(
                    "Suggest work in {} for {}",
                    quote(pr.label(), 60),
                    planner::duration(*minutes)
                ),
                None => format!("Suggest work for {}", planner::duration(*minutes)),
            },
            Self::CreateProject { name } => format!("Create project {}", quote(name, 80)),
            Self::ListProjects => "List active projects".into(),
            Self::Agenda { date } => format!("Show the agenda for {date}"),
            Self::LinkItems { a, b } => format!("Link {} with {}", a.label(), b.label()),
            Self::CheckHabit {
                name: Some(n),
                date,
                ..
            } => format!("Check in {} for {date}", quote(n, 80)),
            Self::CheckHabit { query, date, .. } => {
                format!(
                    "Check in the habit matching {} for {date}",
                    quote(query, 80)
                )
            }
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
            Self::ScheduleTask { query, minutes, .. } => match minutes {
                Some(m) => format!("Find {} for {}", planner::duration(*m), quote(query, 80)),
                None => format!("Find time for {}", quote(query, 80)),
            },
            Self::ScheduleDue { by, .. } => {
                format!("Find time for open tasks due by {}", day_label(by))
            }
            Self::Rearrange { date, .. } => {
                format!("Rearrange flexible sessions on {}", day_label(date))
            }
            Self::CreateBlock {
                title,
                date,
                start_time,
                end_time,
                ..
            } => format!(
                "Schedule {} {} {start_time}–{end_time}",
                quote(title, 80),
                day_label(date)
            ),
            Self::MoveBlock {
                title,
                date,
                start_time,
                end_time,
                ..
            } => format!(
                "Move {} to {} {start_time}–{end_time}",
                quote(title, 80),
                day_label(date)
            ),
            Self::DeleteBlock { title, .. } => format!("Remove the session {}", quote(title, 80)),
            Self::CreateEvent {
                title,
                date,
                start_time,
                end_time,
                fixed,
                ..
            } => format!(
                "Add {} {} on {}{}",
                if *fixed { "event" } else { "flexible event" },
                quote(title, 80),
                day_label(date),
                times_label(start_time.as_deref(), end_time.as_deref())
            ),
            Self::MoveEvent {
                title,
                date,
                start_time,
                end_time,
                ..
            } => format!(
                "Move the event {} to {}{}",
                quote(title, 80),
                day_label(date),
                times_label(start_time.as_deref(), end_time.as_deref())
            ),
            Self::AddSubtask {
                title,
                parent_title,
                ..
            } => format!(
                "Add subtask {} to {}",
                quote(title, 80),
                quote(parent_title, 60)
            ),
            Self::Unscheduled { to, .. } => {
                format!("Show tasks that still need time by {}", day_label(to))
            }
            Self::FreeTime { from, to, .. } if from == to => {
                format!("Show free time on {}", day_label(from))
            }
            Self::FreeTime { from, to, .. } => format!(
                "Show free time from {} to {}",
                day_label(from),
                day_label(to)
            ),
            Self::Missed { date } => format!("Show what was left undone on {}", day_label(date)),
            Self::Undo { .. } => "Undo the last change".into(),
            Self::Revert { op, .. } => op.summary(),
            Self::Remember { prefs } => format!("Remember: {}", undo::describe_prefs(prefs)),
        }
    }
}

/// "Thu 2026-11-05"
pub fn day_label(date: &str) -> String {
    match validate::date(date) {
        Ok(d) => format!(
            "{} {d}",
            &WEEKDAY_NAMES[validate::weekday(&d) as usize][..3]
        ),
        Err(_) => date.to_owned(),
    }
}

/// " 14:00–15:00", " at 14:00", or "" (all day).
fn times_label(start: Option<&str>, end: Option<&str>) -> String {
    match (start, end) {
        (Some(s), Some(e)) => format!(" {s}–{e}"),
        (Some(s), None) => format!(" at {s}"),
        _ => String::new(),
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
    /// Things the owner should know before approving (overlaps, time that
    /// couldn't be found, assumptions such as a default length).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Indexes of earlier steps this one needs (e.g. the task it schedules).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<usize>,
}

impl PlannedAction {
    pub fn new(command: Command) -> Self {
        Self {
            summary: command.summary(),
            risk: command.risk(),
            command,
            notes: vec![],
            after: vec![],
        }
    }
}

/// A calendar item a result refers to (time blocks and events aren't
/// linkable entities), for linking in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub kind: ItemKind,
    pub id: String,
    pub date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Block,
    Event,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    /// The item created or changed, for linking in the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<EntityRef>,
    /// A completion or check-in that changed state (so it may fire automations,
    /// D-024). Not stored.
    #[serde(skip)]
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<Item>,
    /// How to reverse this step, recorded when it ran (D-048).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undo: Option<undo::UndoOp>,
    /// The owner left this step out when approving.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skipped: bool,
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
    /// The plan that undid this one, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reversed_by: Option<String>,
    /// The pending plan this one replaced (an edit before approval).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revises: Option<String>,
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
    /// The user's local time (`HH:MM`), so nothing is scheduled in the past.
    /// Without it the whole of today counts as ahead.
    #[serde(default)]
    pub time: Option<String>,
}

/// Most steps one plan may have (after expansion).
pub const MAX_PLAN_STEPS: usize = 20;

/// The caller's local date and time: "today" and "now" for every relative
/// date and for scheduling (wall clock, so DST never shifts anything).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clock {
    pub today: String,
    /// Minutes since local midnight.
    pub minutes: u32,
}

impl Clock {
    pub fn new(today: &str, time: Option<&str>) -> Result<Self, JarvisError> {
        let today = validate::date(today).map_err(|w| JarvisError::Invalid("today", w))?;
        let minutes = match time {
            Some(t) => validate::clock(t, false).map_err(|w| JarvisError::Invalid("time", w))?,
            None => 0,
        };
        Ok(Self { today, minutes })
    }

    pub fn from_request(req: &PlanRequest) -> Result<Self, JarvisError> {
        Self::new(&req.today, req.time.as_deref())
    }

    pub fn now(&self) -> schedule::Now {
        schedule::Now {
            date: self.today.clone(),
            minutes: self.minutes,
        }
    }
}

impl PlanRequest {
    pub fn new(input: impl Into<String>, today: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            today: today.into(),
            time: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JarvisError {
    Parse(parser::ParseError),
    Invalid(&'static str, &'static str),
    /// A command couldn't be resolved (e.g. no or several matching tasks).
    Unresolved(String),
    /// A name matched several items; the owner picks one (D-048).
    Ambiguous {
        message: String,
        choices: Vec<Choice>,
    },
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
    /// Record that plan `by` undid plan `id`.
    async fn mark_reversed(&self, owner_id: &str, id: &str, by: &str) -> Result<(), String>;
}

/// Module repositories commands execute through, provided by each host
/// (the Worker's D1 adapters, in-memory fakes in tests).
pub trait CommandTargets {
    type Tasks: TaskRepository;
    type Notes: NoteRepository;
    type Journal: JournalRepository;
    type Inbox: InboxRepository;
    type Search: SearchSource;
    type Projects: ProjectRepository;
    type Events: EventRepository;
    type Habits: HabitRepository;
    type Links: LinkRepository;
    type Blocks: BlockRepository;
    type Preferences: PreferencesRepository;
    fn tasks(&self) -> &Self::Tasks;
    fn notes(&self) -> &Self::Notes;
    fn journal(&self) -> &Self::Journal;
    fn inbox(&self) -> &Self::Inbox;
    fn search(&self) -> &Self::Search;
    fn projects(&self) -> &Self::Projects;
    fn events(&self) -> &Self::Events;
    fn habits(&self) -> &Self::Habits;
    fn links(&self) -> &Self::Links;
    fn blocks(&self) -> &Self::Blocks;
    fn preferences(&self) -> &Self::Preferences;
}

fn backend(e: String) -> JarvisError {
    JarvisError::Backend(e)
}

fn failure(message: String) -> ActionResult {
    ActionResult {
        ok: false,
        message,
        entity: None,
        changed: false,
        ..Default::default()
    }
}

fn task_failure(e: TaskError) -> ActionResult {
    failure(match e {
        TaskError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        TaskError::NotFound | TaskError::Exists => "That task no longer exists.".into(),
        TaskError::Backend(_) => "The task service is unavailable.".into(),
    })
}

/// How to put `before`'s fields back on `after` (the task once changed).
fn restore_task(before: &tasks::Task, after: &tasks::Task) -> undo::UndoOp {
    undo::UndoOp::RestoreTask {
        id: before.id.clone(),
        title: before.title.clone(),
        status: before.status,
        priority: before.priority,
        due_date: before.due_date.clone(),
        estimate_minutes: before.estimate_minutes,
        updated_at: after.updated_at,
    }
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

fn habit_failure(e: HabitError) -> ActionResult {
    failure(match e {
        HabitError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        HabitError::NotFound => "That habit no longer exists.".into(),
        _ => "The habits service is unavailable.".into(),
    })
}

const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// "Thursday 2026-10-22 — 09:30 Standup; all day Trip. Due: Pay rent. …"
fn agenda_message(
    date: &str,
    events: &[Occurrence],
    open: &[tasks::Task],
    habits: &[HabitView],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !events.is_empty() {
        let list: Vec<String> = events
            .iter()
            .take(LIST_PREVIEW)
            .map(|o| match &o.start_time {
                Some(t) => format!("{t} {}", o.title),
                None => format!("all day {}", o.title),
            })
            .collect();
        parts.push(format!("Events: {}", list.join("; ")));
    }
    let due: Vec<&str> = open
        .iter()
        .filter(|t| t.due_date.as_deref() == Some(date))
        .map(|t| t.title.as_str())
        .collect();
    if !due.is_empty() {
        parts.push(format!("Due: {}", due.join("; ")));
    }
    let overdue = open
        .iter()
        .filter(|t| t.due_date.as_deref().is_some_and(|d| d < date))
        .count();
    if overdue > 0 {
        parts.push(format!(
            "Overdue: {overdue} task{}",
            if overdue == 1 { "" } else { "s" }
        ));
    }
    let weekday = validate::weekday(date) as u8;
    let left: Vec<&str> = habits
        .iter()
        .filter(|h| {
            !h.habit.archived
                && h.habit.days.contains(&weekday)
                && !h.checkins.iter().any(|c| c == date)
        })
        .map(|h| h.habit.name.as_str())
        .collect();
    if !left.is_empty() {
        parts.push(format!("Habits left: {}", left.join(", ")));
    }
    let day = format!("{} {date}", WEEKDAY_NAMES[weekday as usize]);
    if parts.is_empty() {
        format!("{day}: nothing scheduled.")
    } else {
        format!("{day} — {}.", parts.join(". "))
    }
}

/// "Top 2 for Friday 2026-10-23: 1. Pay rent (overdue by 2 days, high
/// priority). 2. Read." or, with a time budget, "For 45 min: …".
fn plan_message(p: &planner::Proposal, project: Option<&str>) -> String {
    let day = format!(
        "{} {}",
        WEEKDAY_NAMES[validate::weekday(&p.date) as usize],
        p.date
    );
    let within = project.map(|n| format!(" in “{n}”")).unwrap_or_default();
    if p.picks.is_empty() {
        return match (p.minutes, p.too_long) {
            (Some(m), n) if n > 0 => format!(
                "Nothing open{within} fits in {}; the shortest takes longer.",
                planner::duration(m)
            ),
            _ if p.blocked > 0 => "Only tasks waiting on their subtasks are open.".into(),
            _ if project.is_some() => format!("Nothing open{within}."),
            _ => "Nothing open. Nice.".into(),
        };
    }
    let items: Vec<String> = p
        .picks
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mut why: Vec<String> = s.factors.iter().map(|f| f.reason.clone()).collect();
            if p.minutes.is_some() {
                let length = planner::duration(s.minutes);
                why.insert(
                    0,
                    if s.estimated {
                        length
                    } else {
                        format!("about {length}, no estimate")
                    },
                );
            }
            if why.is_empty() {
                format!("{}. {}", i + 1, s.title)
            } else {
                format!("{}. {} ({})", i + 1, s.title, why.join(", "))
            }
        })
        .collect();
    match p.minutes {
        Some(m) => format!(
            "For {}{within} on {day}: {}. That's {} in all.",
            planner::duration(m),
            items.join(". "),
            planner::duration(p.total_minutes)
        ),
        None => format!(
            "Top {}{within} for {day}: {}.",
            p.picks.len(),
            items.join(". ")
        ),
    }
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

/// One of several items a name matched, offered back to the owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub kind: String,
    pub id: String,
    pub title: String,
}

/// Most choices offered for an ambiguous name.
pub const MAX_CHOICES: usize = 5;

fn ambiguity<X>(
    query: &str,
    what: &str,
    matches: &[&X],
    name: impl Fn(&X) -> &str,
    id: impl Fn(&X) -> &str,
) -> JarvisError {
    if matches.is_empty() {
        return JarvisError::Unresolved(format!("No {what} matches “{query}”."));
    }
    let names: Vec<String> = matches
        .iter()
        .take(MAX_CHOICES)
        .map(|x| format!("“{}”", name(x)))
        .collect();
    let kind = what.rsplit(' ').next().unwrap_or(what).to_owned();
    JarvisError::Ambiguous {
        message: format!(
            "“{query}” matches {} {what}s: {}. Be more specific.",
            matches.len(),
            names.join(", ")
        ),
        choices: matches
            .iter()
            .take(MAX_CHOICES)
            .map(|x| Choice {
                kind: kind.clone(),
                id: id(x).to_owned(),
                title: name(x).to_owned(),
            })
            .collect(),
    }
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
    pub async fn plan<P, C>(
        &self,
        p: &Principal,
        provider: &P,
        req: PlanRequest,
        targets: &C,
    ) -> Result<Plan, JarvisError>
    where
        P: IntentProvider,
        C: CommandTargets,
    {
        let input = req.input.trim();
        if input.chars().count() > MAX_INPUT {
            return Err(JarvisError::Invalid("input", "too long"));
        }
        let clock = Clock::from_request(&req)?;
        let commands = provider.interpret(input, &clock.today).await?;
        self.plan_commands(p, input, commands, &clock, None, targets)
            .await
    }

    /// Resolve and expand `commands` into a stored plan (read-only plans run
    /// at once). `revises` is a pending plan this one replaces.
    pub async fn plan_commands<C: CommandTargets>(
        &self,
        p: &Principal,
        input: &str,
        commands: Vec<Command>,
        clock: &Clock,
        revises: Option<String>,
        targets: &C,
    ) -> Result<Plan, JarvisError> {
        let mut actions: Vec<PlannedAction> = Vec::with_capacity(commands.len());
        for command in commands {
            let expanded = self.expand(p, command, &actions, clock, targets).await?;
            actions.extend(expanded);
            if actions.len() > MAX_PLAN_STEPS {
                return Err(JarvisError::Invalid(
                    "plan",
                    "too many steps; ask for less at once",
                ));
            }
        }
        if actions.is_empty() {
            return Err(JarvisError::Unresolved("There's nothing to do.".into()));
        }
        expand::link_steps(&mut actions);
        self.store_plan(p, input, actions, revises, targets).await
    }

    /// Store a resolved plan (running it at once when it only reads).
    async fn store_plan<C: CommandTargets>(
        &self,
        p: &Principal,
        input: &str,
        actions: Vec<PlannedAction>,
        revises: Option<String>,
        targets: &C,
    ) -> Result<Plan, JarvisError> {
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
            reversed_by: None,
            revises,
        };
        if !requires_approval {
            let (status, results) = self.run(p, &plan.id, &plan.actions, &[], targets).await;
            plan.status = status;
            plan.results = results;
        }
        self.repo.insert(&plan).await.map_err(backend)?;
        Ok(plan)
    }

    /// Resolve one side of a link by title (journal days by date).
    async fn resolve_target<C: CommandTargets>(
        &self,
        p: &Principal,
        mut target: LinkTarget,
        t: &C,
    ) -> Result<LinkTarget, JarvisError> {
        let unavailable =
            |what: &str| JarvisError::Unresolved(format!("Couldn't read your {what}."));
        let query = target.query.clone();
        let (id, title) = match target.kind {
            EntityKind::Journal => (query.clone(), query.clone()),
            EntityKind::Task => {
                let all = TaskService::new(t.tasks(), self.entropy, self.now)
                    .list(p, Filter::All)
                    .await
                    .map_err(|_| unavailable("tasks"))?;
                let x = pick(&all, |x| &x.title, &query)
                    .map_err(|m| ambiguity(&query, "task", &m, |x| &x.title, |x| &x.id))?;
                (x.id.clone(), x.title.clone())
            }
            EntityKind::Note => {
                let all = NoteService::new(t.notes(), self.entropy, self.now)
                    .list(p)
                    .await
                    .map_err(|_| unavailable("notes"))?;
                let x = pick(&all, |x| &x.title, &query)
                    .map_err(|m| ambiguity(&query, "note", &m, |x| &x.title, |x| &x.id))?;
                (x.id.clone(), x.title.clone())
            }
            EntityKind::Project => {
                let all = ProjectService::new(t.projects(), self.entropy, self.now)
                    .list(p, true)
                    .await
                    .map_err(|_| unavailable("projects"))?;
                let x = pick(&all, |x| &x.name, &query)
                    .map_err(|m| ambiguity(&query, "project", &m, |x| &x.name, |x| &x.id))?;
                (x.id.clone(), x.name.clone())
            }
        };
        target.id = Some(id);
        target.title = Some(title);
        Ok(target)
    }

    /// Fill in references the user described by name.
    async fn resolve<C: CommandTargets>(
        &self,
        p: &Principal,
        command: Command,
        earlier: &[PlannedAction],
        t: &C,
    ) -> Result<Command, JarvisError> {
        match command {
            Command::LinkItems { a, b } => Ok(Command::LinkItems {
                a: self.resolve_target(p, a, t).await?,
                b: self.resolve_target(p, b, t).await?,
            }),
            Command::CheckHabit { query, date, .. } => {
                let habits = HabitService::new(t.habits(), self.entropy, self.now)
                    .list(p, &date, 1)
                    .await
                    .map_err(|e| match e {
                        HabitError::Backend(b) => JarvisError::Backend(b),
                        _ => JarvisError::Unresolved("Couldn't read your habits.".into()),
                    })?;
                let active: Vec<_> = habits.into_iter().filter(|h| !h.habit.archived).collect();
                let h = pick(&active, |h| &h.habit.name, &query).map_err(|m| {
                    ambiguity(&query, "habit", &m, |h| &h.habit.name, |h| &h.habit.id)
                })?;
                Ok(Command::CheckHabit {
                    habit_id: Some(h.habit.id.clone()),
                    name: Some(h.habit.name.clone()),
                    query,
                    date,
                })
            }
            Command::CompleteTask { query, task_id, .. } => {
                let t = self.task_ref(p, &query, task_id.as_deref(), t).await?;
                Ok(Command::CompleteTask {
                    task_id: Some(t.id),
                    title: Some(t.title),
                    query,
                })
            }
            Command::UpdateTask {
                query,
                change,
                task_id,
                ..
            } => {
                if change.is_empty() {
                    return Err(JarvisError::Invalid("change", "nothing to change"));
                }
                let t = self.task_ref(p, &query, task_id.as_deref(), t).await?;
                Ok(Command::UpdateTask {
                    task_id: Some(t.id),
                    title: Some(t.title),
                    query,
                    change,
                })
            }
            Command::CreateTask {
                id,
                title,
                due_date,
                project: Some(query),
                priority,
                estimate_minutes,
                project_id: _,
            } => {
                let active = ProjectService::new(t.projects(), self.entropy, self.now)
                    .list(p, false)
                    .await
                    .map_err(|e| match e {
                        ProjectError::Backend(b) => JarvisError::Backend(b),
                        _ => JarvisError::Unresolved("Couldn't read your projects.".into()),
                    })?;
                match pick(&active, |x| &x.name, &query) {
                    Ok(pr) => Ok(Command::CreateTask {
                        id,
                        title,
                        due_date,
                        project: Some(pr.name.clone()),
                        project_id: Some(pr.id.clone()),
                        priority,
                        estimate_minutes,
                    }),
                    // Created by an earlier step of this plan: look it up when it runs.
                    Err(m) if m.is_empty() && created_earlier(earlier, &query).is_some() => {
                        Ok(Command::CreateTask {
                            id,
                            title,
                            due_date,
                            project: created_earlier(earlier, &query),
                            project_id: None,
                            priority,
                            estimate_minutes,
                        })
                    }
                    Err(m) => Err(ambiguity(
                        &query,
                        "active project",
                        &m,
                        |x| &x.name,
                        |x| &x.id,
                    )),
                }
            }
            // Any id already present is ignored: the name is always resolved here.
            Command::Priorities {
                date,
                project: Some(scope),
            } => Ok(Command::Priorities {
                date,
                project: Some(self.resolve_scope(p, scope, t).await?),
            }),
            Command::Suggest {
                date,
                minutes,
                project: Some(scope),
            } => Ok(Command::Suggest {
                date,
                minutes,
                project: Some(self.resolve_scope(p, scope, t).await?),
            }),
            other => Ok(other),
        }
    }

    /// The one active project matching a planning request's project name.
    async fn resolve_scope<C: CommandTargets>(
        &self,
        p: &Principal,
        scope: ProjectScope,
        t: &C,
    ) -> Result<ProjectScope, JarvisError> {
        let active = ProjectService::new(t.projects(), self.entropy, self.now)
            .list(p, false)
            .await
            .map_err(|e| match e {
                ProjectError::Backend(b) => JarvisError::Backend(b),
                _ => JarvisError::Unresolved("Couldn't read your projects.".into()),
            })?;
        let pr = pick(&active, |x| &x.name, &scope.query)
            .map_err(|m| ambiguity(&scope.query, "active project", &m, |x| &x.name, |x| &x.id))?;
        Ok(ProjectScope {
            id: Some(pr.id.clone()),
            name: Some(pr.name.clone()),
            query: scope.query,
        })
    }

    /// The open task `task_id` names (a reference the conversation resolved;
    /// checked to be the caller's), else the one whose title matches `query`.
    async fn task_ref<C: CommandTargets>(
        &self,
        p: &Principal,
        query: &str,
        task_id: Option<&str>,
        t: &C,
    ) -> Result<tasks::Task, JarvisError> {
        let Some(id) = task_id else {
            return self.resolve_open_task(p, query, t).await;
        };
        let task = TaskService::new(t.tasks(), self.entropy, self.now)
            .get(p, id)
            .await
            .map_err(|e| match e {
                TaskError::Backend(b) => JarvisError::Backend(b),
                _ => JarvisError::Unresolved("That task no longer exists.".into()),
            })?;
        if !task.status.is_open() {
            return Err(JarvisError::Unresolved(format!(
                "“{}” is already {}.",
                task.title,
                task.status.as_str()
            )));
        }
        Ok(task)
    }

    /// The one open task whose title matches `query`.
    async fn resolve_open_task<C: CommandTargets>(
        &self,
        p: &Principal,
        query: &str,
        t: &C,
    ) -> Result<tasks::Task, JarvisError> {
        let open = TaskService::new(t.tasks(), self.entropy, self.now)
            .list(p, Filter::Open)
            .await
            .map_err(|e| match e {
                TaskError::Backend(b) => JarvisError::Backend(b),
                _ => JarvisError::Unresolved("Couldn't read your tasks.".into()),
            })?;
        pick(&open, |t| &t.title, query)
            .cloned()
            .map_err(|m| ambiguity(query, "open task", &m, |t| &t.title, |t| &t.id))
    }

    /// Approve and run a pending plan (exactly once).
    pub async fn execute<C>(
        &self,
        p: &Principal,
        id: &str,
        targets: &C,
    ) -> Result<Plan, JarvisError>
    where
        C: CommandTargets,
    {
        self.execute_with(p, id, &[], targets).await
    }

    /// Approve a pending plan without the steps at `skip` (indexes), and run
    /// it exactly once. A step another kept step needs can't be skipped.
    pub async fn execute_with<C>(
        &self,
        p: &Principal,
        id: &str,
        skip: &[usize],
        targets: &C,
    ) -> Result<Plan, JarvisError>
    where
        C: CommandTargets,
    {
        let plan = self.load(p, id).await?;
        if plan.status != PlanStatus::Pending {
            return Err(JarvisError::NotPending(plan.status));
        }
        expand::check_skips(&plan.actions, skip)?;
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
        let (status, results) = self.run(p, id, &plan.actions, skip, targets).await;
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

    /// Resolve and run commands the owner approved in advance (an automation
    /// rule, DECISIONS D-024). No plan is stored; the caller records the outcome.
    pub async fn run_unattended<C: CommandTargets>(
        &self,
        p: &Principal,
        commands: Vec<Command>,
        targets: &C,
    ) -> (PlanStatus, Vec<ActionResult>) {
        let mut actions = Vec::with_capacity(commands.len());
        for command in commands {
            match self.resolve(p, command, &actions, targets).await {
                Ok(command) => actions.push(PlannedAction::new(command)),
                Err(e) => {
                    let message = match e {
                        JarvisError::Unresolved(m) => m,
                        _ => "A step couldn't be prepared.".into(),
                    };
                    return (PlanStatus::Failed, vec![failure(message)]);
                }
            }
        }
        self.run(p, "", &actions, &[], targets).await
    }

    /// Run actions in order, leaving out `skip`; stop at the first failure.
    /// A plan whose reversals all succeed marks the plan it undid.
    async fn run<C>(
        &self,
        p: &Principal,
        plan_id: &str,
        actions: &[PlannedAction],
        skip: &[usize],
        t: &C,
    ) -> (PlanStatus, Vec<ActionResult>)
    where
        C: CommandTargets,
    {
        let touched = expand::touched_blocks(actions);
        let touched: Vec<&str> = touched.iter().map(String::as_str).collect();
        let mut results = Vec::with_capacity(actions.len());
        for (i, action) in actions.iter().enumerate() {
            if skip.contains(&i) {
                results.push(ActionResult {
                    ok: true,
                    message: "Left out.".into(),
                    skipped: true,
                    ..Default::default()
                });
                continue;
            }
            let result = self.run_step(p, &action.command, &touched, t).await;
            let ok = result.ok;
            results.push(result);
            if !ok {
                return (PlanStatus::Failed, results);
            }
        }
        let reverted: Vec<&str> = actions
            .iter()
            .filter_map(|a| match &a.command {
                Command::Revert { plan_id, .. } => Some(plan_id.as_str()),
                _ => None,
            })
            .collect();
        if let (Some(original), false, true) =
            (reverted.first(), plan_id.is_empty(), skip.is_empty())
        {
            // Best effort: the reversal itself already happened.
            let _ = self.repo.mark_reversed(&p.user_id, original, plan_id).await;
        }
        (PlanStatus::Executed, results)
    }

    /// Priorities (no time budget) or suggestions for `minutes`, as one
    /// spoken-friendly sentence with the reasons for each pick.
    async fn plan_work<C: CommandTargets>(
        &self,
        p: &Principal,
        t: &C,
        date: &str,
        minutes: Option<u32>,
        project: Option<&ProjectScope>,
    ) -> ActionResult {
        let project_id = match project {
            Some(ProjectScope { id: Some(id), .. }) => Some(id.clone()),
            Some(pr) => return failure(format!("No project was resolved for “{}”.", pr.query)),
            None => None,
        };
        let open = match TaskService::new(t.tasks(), self.entropy, self.now)
            .list(p, Filter::Open)
            .await
        {
            Ok(open) => open,
            Err(e) => return task_failure(e),
        };
        let c = Constraints {
            minutes,
            max_items: Some(if minutes.is_some() {
                LIST_PREVIEW
            } else {
                planner::TOP
            }),
            project_id,
        };
        let proposal = match planner::propose(&open, date, self.now, &c) {
            Ok(proposal) => proposal,
            Err(planner::PlanError::Invalid(f, w)) => return failure(format!("Invalid {f}: {w}.")),
        };
        ActionResult {
            ok: true,
            message: plan_message(&proposal, project.map(ProjectScope::label)),
            entity: proposal.picks.first().map(|s| EntityRef {
                kind: EntityKind::Task,
                id: s.task_id.clone(),
            }),
            changed: false,
            ..Default::default()
        }
    }

    async fn run_one<C>(&self, p: &Principal, command: &Command, t: &C) -> ActionResult
    where
        C: CommandTargets,
    {
        let tasks = TaskService::new(t.tasks(), self.entropy, self.now);
        let entity = |kind, id: &str| {
            Some(EntityRef {
                kind,
                id: id.to_owned(),
            })
        };
        match command {
            Command::CreateTask {
                id,
                title,
                due_date,
                project,
                project_id,
                priority,
                estimate_minutes,
            } => {
                // A project created earlier in this plan is looked up by its exact name now.
                let project_id = match (project, project_id) {
                    (Some(name), None) => {
                        let active = match ProjectService::new(t.projects(), self.entropy, self.now)
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
                    id: id.clone(),
                    title: title.clone(),
                    notes: None,
                    priority: *priority,
                    due_date: due_date.clone(),
                    parent_id: None,
                    project_id: project_id.clone(),
                    estimate_minutes: *estimate_minutes,
                    repeat: Default::default(),
                    repeat_until: None,
                };
                match tasks.create(p, input).await {
                    Ok(task) => ActionResult {
                        ok: true,
                        message: match project {
                            Some(name) => format!("Created task “{}” in “{name}”.", task.title),
                            None => format!("Created task “{}”.", task.title),
                        },
                        entity: entity(EntityKind::Task, &task.id),
                        undo: Some(undo::UndoOp::DeleteTask {
                            id: task.id.clone(),
                            title: task.title.clone(),
                            updated_at: task.updated_at,
                        }),
                        ..Default::default()
                    },
                    // A replayed step: the task this plan chose an id for exists.
                    Err(TaskError::Exists) => ActionResult {
                        ok: true,
                        message: format!("“{title}” was already created."),
                        entity: id.as_deref().and_then(|id| entity(EntityKind::Task, id)),
                        ..Default::default()
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
                match ProjectService::new(t.projects(), self.entropy, self.now)
                    .create(p, input)
                    .await
                {
                    Ok(pr) => ActionResult {
                        ok: true,
                        message: format!("Created project “{}”.", pr.name),
                        entity: entity(EntityKind::Project, &pr.id),
                        changed: false,
                        ..Default::default()
                    },
                    Err(e) => project_failure(e),
                }
            }
            Command::ListProjects => {
                let active = match ProjectService::new(t.projects(), self.entropy, self.now)
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
                    changed: false,
                    ..Default::default()
                }
            }
            Command::CompleteTask {
                task_id: Some(id), ..
            } => {
                let patch = TaskPatch {
                    status: Some(Status::Done),
                    ..Default::default()
                };
                let before = tasks.get(p, id).await.ok();
                let was_open = before.as_ref().is_some_and(|t| t.status.is_open());
                match tasks.update(p, id, patch).await {
                    Ok(task) => ActionResult {
                        ok: true,
                        message: format!("Completed “{}”.", task.title),
                        entity: entity(EntityKind::Task, &task.id),
                        changed: was_open,
                        // Completing a repeating task creates the next one;
                        // that isn't reversed automatically.
                        undo: before
                            .filter(|b| was_open && b.repeat == crate::calendar::Repeat::None)
                            .map(|b| restore_task(&b, &task)),
                        ..Default::default()
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
                match NoteService::new(t.notes(), self.entropy, self.now)
                    .create(p, input)
                    .await
                {
                    Ok(note) => ActionResult {
                        ok: true,
                        message: "Created a note.".into(),
                        entity: entity(EntityKind::Note, &note.id),
                        changed: false,
                        ..Default::default()
                    },
                    Err(e) => note_failure(e),
                }
            }
            Command::AppendJournal { date, text } => {
                match JournalService::new(t.journal(), self.now)
                    .append(p, date, text)
                    .await
                {
                    Ok(e) => ActionResult {
                        ok: true,
                        message: format!("Added to the journal for {}.", e.date),
                        entity: entity(EntityKind::Journal, &e.date),
                        changed: false,
                        ..Default::default()
                    },
                    Err(e) => journal_failure(e),
                }
            }
            Command::Capture { text } => match InboxService::new(t.inbox(), self.entropy, self.now)
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
                    changed: false,
                    ..Default::default()
                },
                Err(e) => inbox_failure(e),
            },
            Command::Find { query } => match search::search(t.search(), p, query).await {
                Ok(hits) if hits.is_empty() => ActionResult {
                    ok: true,
                    message: format!("Nothing matches “{query}”."),
                    entity: None,
                    changed: false,
                    ..Default::default()
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
                        // Events and habits aren't linkable entities.
                        entity: top.kind.entity().and_then(|k| entity(k, &top.id)),
                        changed: false,
                        ..Default::default()
                    }
                }
                Err(SearchError::Invalid(why)) => failure(format!("Can't search for that: {why}.")),
                Err(SearchError::Backend(_)) => failure("Search is unavailable.".into()),
            },
            Command::CheckHabit {
                habit_id: None,
                query,
                ..
            } => failure(format!("No habit was resolved for “{query}”.")),
            Command::CheckHabit {
                habit_id: Some(id),
                name,
                date,
                ..
            } => {
                let habits = HabitService::new(t.habits(), self.entropy, self.now);
                let changed = match habits.set_check(p, id, date, true).await {
                    Ok((_, changed)) => changed,
                    Err(e) => return habit_failure(e),
                };
                let name = name.as_deref().unwrap_or("the habit");
                if !changed {
                    return ActionResult {
                        ok: true,
                        message: format!("“{name}” was already checked in for {date}."),
                        entity: None,
                        changed: false,
                        ..Default::default()
                    };
                }
                // The streak as of the checked day.
                let streak = match habits.list(p, date, 1).await {
                    Ok(list) => list
                        .into_iter()
                        .find(|h| &h.habit.id == id)
                        .map(|h| h.streak),
                    Err(_) => None,
                };
                ActionResult {
                    ok: true,
                    message: match streak {
                        Some(n) if n > 1 => {
                            format!("Checked in “{name}” for {date} ({n}-day streak).")
                        }
                        _ => format!("Checked in “{name}” for {date}."),
                    },
                    entity: None,
                    changed: true,
                    ..Default::default()
                }
            }
            Command::LinkItems { a, b } => {
                let (Some(a_id), Some(b_id)) = (&a.id, &b.id) else {
                    return failure("The items to link weren't resolved.".into());
                };
                let input = NewLink {
                    from: EntityRef {
                        kind: a.kind,
                        id: a_id.clone(),
                    },
                    to: EntityRef {
                        kind: b.kind,
                        id: b_id.clone(),
                    },
                };
                match LinkService::new(t.links(), self.entropy, self.now)
                    .link(p, input)
                    .await
                {
                    Ok(_) => ActionResult {
                        ok: true,
                        message: format!("Linked {} with {}.", a.label(), b.label()),
                        entity: entity(a.kind, a_id),
                        changed: false,
                        ..Default::default()
                    },
                    Err(LinkError::Invalid(f, w)) => failure(format!("Can't link those: {f} {w}.")),
                    Err(LinkError::NotFound) => {
                        failure("One of those items no longer exists.".into())
                    }
                    Err(LinkError::Backend(_)) => failure("Links are unavailable.".into()),
                }
            }
            Command::Agenda { date } => {
                let events = match CalendarService::new(t.events(), self.entropy, self.now)
                    .agenda(p, date, date)
                    .await
                {
                    Ok(o) => o,
                    Err(_) => return failure("The calendar is unavailable.".into()),
                };
                let open = match tasks.list(p, Filter::Open).await {
                    Ok(open) => open,
                    Err(e) => return task_failure(e),
                };
                let habits = match HabitService::new(t.habits(), self.entropy, self.now)
                    .list(p, date, 1)
                    .await
                {
                    Ok(h) => h,
                    Err(e) => return habit_failure(e),
                };
                ActionResult {
                    ok: true,
                    message: agenda_message(date, &events, &open, &habits),
                    entity: None,
                    changed: false,
                    ..Default::default()
                }
            }
            Command::UpdateTask {
                task_id: Some(id),
                change,
                ..
            } => {
                let patch = TaskPatch {
                    due_date: change.due_date.clone(),
                    priority: change.priority,
                    estimate_minutes: change.estimate_minutes.map(Some),
                    ..Default::default()
                };
                let before = tasks.get(p, id).await.ok();
                match tasks.update(p, id, patch).await {
                    Ok(task) => ActionResult {
                        ok: true,
                        message: format!("Updated “{}”: {}.", task.title, change.describe()),
                        entity: entity(EntityKind::Task, &task.id),
                        undo: before.map(|b| restore_task(&b, &task)),
                        ..Default::default()
                    },
                    Err(e) => task_failure(e),
                }
            }
            Command::UpdateTask {
                task_id: None,
                query,
                ..
            } => failure(format!("No task was resolved for “{query}”.")),
            Command::Priorities { date, project } => {
                self.plan_work(p, t, date, None, project.as_ref()).await
            }
            Command::Suggest {
                date,
                minutes,
                project,
            } => {
                self.plan_work(p, t, date, Some(*minutes), project.as_ref())
                    .await
            }
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
                        changed: false,
                        ..Default::default()
                    }
                }
                Err(e) => task_failure(e),
            },
            // Scheduling, calendar, and undo steps run in `expand::run_step`.
            _ => failure("That step can't run on its own.".into()),
        }
    }
}

#[cfg(test)]
mod tests;
