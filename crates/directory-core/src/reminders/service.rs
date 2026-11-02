//! The reminder service (D-049): validation, owner scoping, field-level
//! merge, completion and snooze per occurrence, lists, and the scheduler step.

use super::model::{self, *};
use super::recurrence::Rule;
use super::schedule::{self, Completion, Fire, Scan};
use super::zone::{local_at, local_to_utc, ZoneClock, DAY_MS, MINUTE_MS};
use crate::categories::{self, MAX_PER_ITEM};
use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::notifications::{notice, Notice};
use crate::tasks::Priority;
use crate::validate;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Longest agenda range (`GET /reminders/agenda`).
pub const MAX_AGENDA_DAYS: i64 = 100;
pub const MAX_AGENDA_ITEMS: usize = 1_000;
pub const MAX_SNOOZE_MS: i64 = 365 * DAY_MS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReminderError {
    Invalid(&'static str, &'static str),
    /// A client-supplied id is already in use (a replayed offline create).
    Exists,
    NotFound,
    /// The edit overlaps a newer change to the same fields; nothing was saved.
    Conflict(Box<Reminder>, Vec<&'static str>),
    /// The action doesn't apply to the reminder as it now is (say, snoozing
    /// an occurrence that has passed).
    Stale(Box<Reminder>, &'static str),
    Backend(String),
}

fn backend(e: String) -> ReminderError {
    ReminderError::Backend(e)
}

fn bad((field, why): Bad) -> ReminderError {
    ReminderError::Invalid(field, why)
}

/// A resolved occurrence: `completed` or `missed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccurrenceRow {
    pub reminder_id: String,
    /// The occurrence date, or `-` for an undated reminder.
    pub occurrence: String,
    pub state: String,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OccurrenceChange {
    Set(OccurrenceRow),
    Remove(String),
}

/// Occurrence changes and history lines one action writes.
type Effects = (Vec<OccurrenceChange>, Vec<HistoryEntry>);

/// One line of a reminder's append-only history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub reminder_id: String,
    pub occurrence: String,
    /// `completed`, `uncompleted`, `snoozed`, or `missed`.
    pub action: String,
    pub at: i64,
    /// For a snooze, the instant it lasts until.
    pub until: Option<i64>,
}

pub trait ReminderRepository {
    async fn list(&self, owner_id: &str) -> Result<Vec<Reminder>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Reminder>, String>;
    async fn count(&self, owner_id: &str) -> Result<u32, String>;
    /// Insert the reminder and its categories.
    async fn insert(&self, r: &Reminder) -> Result<(), String>;
    /// Save the reminder, its categories, occurrence changes, history, and
    /// notices (ignoring any already logged) in one batch, only if the stored
    /// version is still `expected`. `false` means it changed meanwhile (or
    /// is gone), and nothing was written.
    async fn save(
        &self,
        r: &Reminder,
        expected: i64,
        changes: &[OccurrenceChange],
        history: &[HistoryEntry],
        notices: &[Notice],
    ) -> Result<bool, String>;
    /// Remove the reminder with its categories, occurrences, history, and notification log.
    async fn purge(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    /// Resolved occurrences dated `from..=to`, of one reminder or all of the owner's.
    async fn occurrences(
        &self,
        owner_id: &str,
        reminder_id: Option<&str>,
        from: &str,
        to: &str,
    ) -> Result<Vec<OccurrenceRow>, String>;
    /// Newest first.
    async fn history(
        &self,
        owner_id: &str,
        id: &str,
        limit: u32,
    ) -> Result<Vec<HistoryEntry>, String>;
    async fn category_ids(&self, owner_id: &str) -> Result<Vec<String>, String>;
    async fn task_exists(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    async fn project_exists(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    /// The owner's reminders linked to a task.
    async fn linked_to_task(&self, owner_id: &str, task_id: &str) -> Result<Vec<Reminder>, String>;
    async fn lists(&self, owner_id: &str) -> Result<Vec<ReminderList>, String>;
    async fn insert_list(&self, l: &ReminderList) -> Result<(), String>;
    async fn update_list(&self, l: &ReminderList) -> Result<bool, String>;
    /// Delete the list; its reminders stay, with no list.
    async fn delete_list(&self, owner_id: &str, id: &str, now: i64) -> Result<bool, String>;
    /// Reminders of every owner whose `wake_at` has come (scheduler only).
    async fn due_for_scan(&self, now: i64, limit: u32) -> Result<Vec<Reminder>, String>;
}

fn none() -> Priority {
    Priority::None
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewReminder {
    /// Client-chosen id (offline create); must be a fresh 32-hex id.
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub due_time: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub list_id: Option<String>,
    #[serde(default)]
    pub category_ids: Vec<String>,
    #[serde(default = "none")]
    pub priority: Priority,
    #[serde(default)]
    pub flagged: bool,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub complete_with_task: bool,
    /// Omitted: the default alert (at the due time, or 09:00). `[]`: none.
    #[serde(default)]
    pub alerts: Option<Vec<Alert>>,
    #[serde(default)]
    pub recurrence: Option<Rule>,
    #[serde(default)]
    pub advance: Advance,
    #[serde(default)]
    pub missed: Missed,
    #[serde(default)]
    pub checklist: Vec<ChecklistItem>,
}

impl NewReminder {
    pub fn titled(title: &str) -> Self {
        serde_json::from_value(serde_json::json!({ "title": title })).expect("minimal reminder")
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReminderPatch {
    /// The version the client edited; enables field-level merge.
    #[serde(default)]
    pub base_version: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub links: Option<Vec<String>>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub due_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub due_time: Option<Option<String>>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub list_id: Option<Option<String>>,
    #[serde(default)]
    pub category_ids: Option<Vec<String>>,
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde(default)]
    pub flagged: Option<bool>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub project_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub task_id: Option<Option<String>>,
    #[serde(default)]
    pub complete_with_task: Option<bool>,
    #[serde(default)]
    pub alerts: Option<Vec<Alert>>,
    #[serde(default, deserialize_with = "validate::double_option")]
    pub recurrence: Option<Option<Rule>>,
    #[serde(default)]
    pub advance: Option<Advance>,
    #[serde(default)]
    pub missed: Option<Missed>,
    #[serde(default)]
    pub checklist: Option<Vec<ChecklistItem>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Snooze {
    #[serde(default)]
    pub minutes: Option<u32>,
    #[serde(default)]
    pub until: Option<i64>,
    /// 09:00 tomorrow, local to the reminder.
    #[serde(default)]
    pub tomorrow: bool,
    /// The occurrence the client meant (a notification's); must be current.
    #[serde(default)]
    pub occurrence: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OccurrenceRef {
    /// An occurrence date; default the current one.
    #[serde(default)]
    pub occurrence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgendaItem {
    pub reminder_id: String,
    pub occurrence_id: String,
    pub date: String,
    pub time: Option<String>,
    /// The occurrence's start (its due time, or local midnight).
    pub at: Option<i64>,
    pub title: String,
    /// `open`, `overdue`, `completed`, or `missed`.
    pub state: &'static str,
    pub recurring: bool,
    pub priority: Priority,
    pub flagged: bool,
}

/// What the scheduler did for one reminder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scanned {
    pub owner_id: String,
    pub reminder_id: String,
    pub scan: Scan,
}

/// The fields a patch can change, for merge bookkeeping.
const FIELDS: [&str; 19] = [
    "title",
    "notes",
    "links",
    "due_date",
    "due_time",
    "timezone",
    "list_id",
    "category_ids",
    "priority",
    "flagged",
    "project_id",
    "task_id",
    "complete_with_task",
    "alerts",
    "recurrence",
    "advance",
    "missed",
    "checklist",
    "snoozed_until",
];

fn changed_fields(a: &Reminder, b: &Reminder) -> Vec<&'static str> {
    let rule = |r: &Reminder| {
        r.recurrence.clone().map(|mut x| {
            x.anchor.clear();
            x
        })
    };
    let same = [
        a.title == b.title,
        a.notes == b.notes,
        a.links == b.links,
        a.due_date == b.due_date,
        a.due_time == b.due_time,
        a.timezone == b.timezone,
        a.list_id == b.list_id,
        a.category_ids == b.category_ids,
        a.priority == b.priority,
        a.flagged == b.flagged,
        a.project_id == b.project_id,
        a.task_id == b.task_id,
        a.complete_with_task == b.complete_with_task,
        a.alerts == b.alerts,
        rule(a) == rule(b),
        a.advance == b.advance,
        a.missed == b.missed,
        a.checklist == b.checklist,
        a.snoozed_until == b.snoozed_until,
    ];
    FIELDS
        .iter()
        .zip(same)
        .filter(|(_, s)| !s)
        .map(|(f, _)| *f)
        .collect()
}

/// Bumps the version and stamps the changed fields.
fn stamp(before: &Reminder, next: &mut Reminder, now: i64) {
    next.version = before.version + 1;
    next.updated_at = now;
    for f in changed_fields(before, next) {
        next.field_versions.insert(f.to_owned(), next.version);
    }
}

pub struct ReminderService<'a, R, E: ?Sized, C> {
    repo: &'a R,
    entropy: &'a E,
    clock: &'a C,
    now: i64,
}

impl<'a, R: ReminderRepository, E: Entropy + ?Sized, C: ZoneClock> ReminderService<'a, R, E, C> {
    pub fn new(repo: &'a R, entropy: &'a E, clock: &'a C, now: i64) -> Self {
        Self {
            repo,
            entropy,
            clock,
            now,
        }
    }

    fn history(
        &self,
        r: &Reminder,
        occurrence: &str,
        action: &str,
        until: Option<i64>,
    ) -> HistoryEntry {
        HistoryEntry {
            id: new_id(self.entropy),
            reminder_id: r.id.clone(),
            occurrence: occurrence.to_owned(),
            action: action.to_owned(),
            at: self.now,
            until,
        }
    }

    fn row(&self, r: &Reminder, occurrence: &str, state: &str) -> OccurrenceChange {
        OccurrenceChange::Set(OccurrenceRow {
            reminder_id: r.id.clone(),
            occurrence: occurrence.to_owned(),
            state: state.to_owned(),
            at: self.now,
        })
    }

    fn zone(&self, raw: &str) -> Result<String, ReminderError> {
        let z = validate::timezone(raw).map_err(|w| ReminderError::Invalid("timezone", w))?;
        self.clock
            .offset_minutes(&z, self.now)
            .map(|_| z)
            .ok_or(ReminderError::Invalid("timezone", "unknown time zone"))
    }

    pub async fn list(&self, p: &Principal) -> Result<Vec<Reminder>, ReminderError> {
        self.repo.list(&p.user_id).await.map_err(backend)
    }

    pub async fn get(&self, p: &Principal, id: &str) -> Result<Reminder, ReminderError> {
        if !is_id(id) {
            return Err(ReminderError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(ReminderError::NotFound)
    }

    /// Check ids against what the owner has: links, list, categories.
    async fn check_refs(
        &self,
        p: &Principal,
        before: Option<&Reminder>,
        r: &mut Reminder,
    ) -> Result<(), ReminderError> {
        let unchanged = |f: fn(&Reminder) -> &Option<String>| before.is_some_and(|b| f(b) == f(r));
        if let Some(t) = r.task_id.clone().filter(|_| !unchanged(|x| &x.task_id)) {
            if !is_id(&t)
                || !self
                    .repo
                    .task_exists(&p.user_id, &t)
                    .await
                    .map_err(backend)?
            {
                return Err(ReminderError::Invalid("task_id", "unknown task"));
            }
        }
        if let Some(pr) = r
            .project_id
            .clone()
            .filter(|_| !unchanged(|x| &x.project_id))
        {
            if !is_id(&pr)
                || !self
                    .repo
                    .project_exists(&p.user_id, &pr)
                    .await
                    .map_err(backend)?
            {
                return Err(ReminderError::Invalid("project_id", "unknown project"));
            }
        }
        if let Some(l) = r.list_id.clone().filter(|_| !unchanged(|x| &x.list_id)) {
            let lists = self.repo.lists(&p.user_id).await.map_err(backend)?;
            if !lists.iter().any(|x| x.id == l) {
                return Err(ReminderError::Invalid("list_id", "unknown list"));
            }
        }
        if r.task_id.is_none() {
            r.complete_with_task = false;
        }
        if before.is_none_or(|b| b.category_ids != r.category_ids) {
            let owned = self.repo.category_ids(&p.user_id).await.map_err(backend)?;
            let mut seen = HashSet::new();
            let mut ids = Vec::new();
            for id in &r.category_ids {
                if !owned.contains(id) {
                    return Err(ReminderError::Invalid("category_ids", "unknown category"));
                }
                if seen.insert(id.clone()) {
                    ids.push(id.clone());
                }
            }
            if ids.len() > MAX_PER_ITEM {
                return Err(ReminderError::Invalid(
                    "category_ids",
                    "at most 12 categories per item",
                ));
            }
            r.category_ids = ids;
        }
        Ok(())
    }

    pub async fn create(
        &self,
        p: &Principal,
        input: NewReminder,
    ) -> Result<Reminder, ReminderError> {
        let id = match input.id {
            Some(id) if !is_id(&id) => {
                return Err(ReminderError::Invalid("id", "expected 32 hex characters"))
            }
            Some(id)
                if self
                    .repo
                    .get(&p.user_id, &id)
                    .await
                    .map_err(backend)?
                    .is_some() =>
            {
                return Err(ReminderError::Exists)
            }
            Some(id) => id,
            None => new_id(self.entropy),
        };
        if self.repo.count(&p.user_id).await.map_err(backend)? >= MAX_REMINDERS {
            return Err(ReminderError::Invalid(
                "reminders",
                "limit of 5000 reminders reached; delete some first",
            ));
        }
        let due_date = match input.due_date {
            Some(d) => Some(validate::date(&d).map_err(|w| ReminderError::Invalid("due_date", w))?),
            None => None,
        };
        let due_time = match input.due_time {
            Some(_) if due_date.is_none() => {
                return Err(ReminderError::Invalid("due_time", "a time needs a date"))
            }
            Some(t) => Some(model::time(&t).map_err(bad)?),
            None => None,
        };
        let alerts = match input.alerts {
            Some(a) => model::alerts(a).map_err(bad)?,
            None if due_date.is_some() => default_alerts(due_time.as_deref()),
            None => Vec::new(),
        };
        let mut r = Reminder {
            id,
            owner_id: p.user_id.clone(),
            title: model::title(&input.title).map_err(bad)?,
            notes: model::notes(&input.notes).map_err(bad)?,
            links: model::links(input.links).map_err(bad)?,
            recurrence: model::rule(input.recurrence, due_date.as_deref()).map_err(bad)?,
            due_date,
            due_time,
            timezone: self.zone(input.timezone.as_deref().unwrap_or("UTC"))?,
            list_id: input.list_id,
            category_ids: input.category_ids,
            priority: input.priority,
            flagged: input.flagged,
            project_id: input.project_id,
            task_id: input.task_id,
            complete_with_task: input.complete_with_task,
            alerts,
            advance: input.advance,
            missed: input.missed,
            checklist: model::checklist(input.checklist, || new_id(self.entropy)).map_err(bad)?,
            snoozed_until: None,
            completed_ahead: Vec::new(),
            backlog: 0,
            completed_at: None,
            deleted_at: None,
            created_at: self.now,
            updated_at: self.now,
            version: 1,
            field_versions: Default::default(),
            due_at: None,
            notified_through: self.now,
            wake_at: None,
        };
        self.check_refs(p, None, &mut r).await?;
        schedule::refresh(self.clock, &mut r, self.now);
        self.repo.insert(&r).await.map_err(backend)?;
        Ok(r)
    }

    /// Apply `f` to the current record and save it, retrying a few times if
    /// another write lands in between. `f` returns `None` for "no change".
    async fn mutate<F>(&self, p: &Principal, id: &str, mut f: F) -> Result<Reminder, ReminderError>
    where
        F: FnMut(&Self, &mut Reminder) -> Result<Option<Effects>, ReminderError>,
    {
        for _ in 0..3 {
            let before = self.get(p, id).await?;
            let mut next = before.clone();
            let Some((changes, history)) = f(self, &mut next)? else {
                return Ok(before);
            };
            stamp(&before, &mut next, self.now);
            schedule::refresh(self.clock, &mut next, self.now);
            if self
                .repo
                .save(&next, before.version, &changes, &history, &[])
                .await
                .map_err(backend)?
            {
                return Ok(next);
            }
        }
        Err(ReminderError::Backend(
            "the reminder kept changing; try again".into(),
        ))
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: ReminderPatch,
    ) -> Result<Reminder, ReminderError> {
        for _ in 0..3 {
            let before = self.get(p, id).await?;
            if before.deleted_at.is_some() {
                return Err(ReminderError::Invalid(
                    "reminder",
                    "restore it before editing",
                ));
            }
            let mut next = self.apply(&before, patch.clone())?;
            self.check_refs(p, Some(&before), &mut next).await?;
            let changed = changed_fields(&before, &next);
            if changed.is_empty() {
                return Ok(before);
            }
            if let Some(base) = patch.base_version.filter(|b| *b < before.version) {
                // Fields someone else changed after the client's copy.
                let clash: Vec<&'static str> = changed
                    .iter()
                    .copied()
                    .filter(|f| before.field_versions.get(*f).is_some_and(|v| *v > base))
                    .collect();
                if !clash.is_empty() {
                    return Err(ReminderError::Conflict(Box::new(before), clash));
                }
            }
            stamp(&before, &mut next, self.now);
            schedule::refresh(self.clock, &mut next, self.now);
            if self
                .repo
                .save(&next, before.version, &[], &[], &[])
                .await
                .map_err(backend)?
            {
                return Ok(next);
            }
        }
        Err(ReminderError::Backend(
            "the reminder kept changing; try again".into(),
        ))
    }

    fn apply(&self, before: &Reminder, patch: ReminderPatch) -> Result<Reminder, ReminderError> {
        let mut r = before.clone();
        if let Some(t) = patch.title {
            r.title = model::title(&t).map_err(bad)?;
        }
        if let Some(n) = patch.notes {
            r.notes = model::notes(&n).map_err(bad)?;
        }
        if let Some(l) = patch.links {
            r.links = model::links(l).map_err(bad)?;
        }
        if let Some(d) = patch.due_date {
            r.due_date = match d {
                Some(d) => {
                    Some(validate::date(&d).map_err(|w| ReminderError::Invalid("due_date", w))?)
                }
                None => None,
            };
        }
        if let Some(t) = patch.due_time {
            r.due_time = t.map(|t| model::time(&t)).transpose().map_err(bad)?;
        }
        if r.due_date.is_none() {
            r.due_time = None;
        }
        if let Some(z) = patch.timezone {
            r.timezone = self.zone(&z)?;
        }
        if let Some(l) = patch.list_id {
            r.list_id = l;
        }
        if let Some(c) = patch.category_ids {
            r.category_ids = c;
        }
        if let Some(x) = patch.priority {
            r.priority = x;
        }
        if let Some(x) = patch.flagged {
            r.flagged = x;
        }
        if let Some(x) = patch.project_id {
            r.project_id = x;
        }
        if let Some(x) = patch.task_id {
            r.task_id = x;
        }
        if let Some(x) = patch.complete_with_task {
            r.complete_with_task = x;
        }
        if let Some(a) = patch.alerts {
            r.alerts = model::alerts(a).map_err(bad)?;
        }
        if let Some(x) = patch.advance {
            r.advance = x;
        }
        if let Some(x) = patch.missed {
            r.missed = x;
        }
        if let Some(c) = patch.checklist {
            r.checklist = model::checklist(c, || new_id(self.entropy)).map_err(bad)?;
        }
        let rule_patch = patch.recurrence;
        let moved = r.due_date != before.due_date;
        if let Some(rule) = rule_patch.clone() {
            r.recurrence = rule;
        }
        if moved || rule_patch.is_some() || r.advance != before.advance {
            // The schedule changed: re-anchor the rule at the (new) date.
            r.recurrence = model::rule(r.recurrence.take(), r.due_date.as_deref()).map_err(bad)?;
            r.completed_ahead.clear();
        }
        if moved || r.due_time != before.due_time || r.timezone != before.timezone {
            r.snoozed_until = None;
        }
        if changed_fields(before, &r).iter().any(|f| {
            matches!(
                *f,
                "due_date" | "due_time" | "timezone" | "alerts" | "recurrence"
            )
        }) {
            // Don't send alerts whose time passed before this edit.
            r.notified_through = r.notified_through.max(self.now);
        }
        Ok(r)
    }

    /// Soft-delete; deleting a deleted reminder removes it for good.
    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), ReminderError> {
        let r = self.get(p, id).await?;
        if r.deleted_at.is_some() {
            if !self.repo.purge(&p.user_id, id).await.map_err(backend)? {
                return Err(ReminderError::NotFound);
            }
            return Ok(());
        }
        self.mutate(p, id, |s, r| {
            if r.deleted_at.is_some() {
                return Ok(None);
            }
            r.deleted_at = Some(s.now);
            Ok(Some((vec![], vec![])))
        })
        .await
        .map(|_| ())
    }

    pub async fn restore(&self, p: &Principal, id: &str) -> Result<Reminder, ReminderError> {
        self.mutate(p, id, |s, r| {
            if r.deleted_at.is_none() {
                return Ok(None);
            }
            r.deleted_at = None;
            r.notified_through = r.notified_through.max(s.now);
            Ok(Some((vec![], vec![])))
        })
        .await
    }

    /// Complete an occurrence (default: the current one). Idempotent per
    /// occurrence; completing one the reminder has moved past (say, offline
    /// while it was recorded missed) records it completed.
    pub async fn complete(
        &self,
        p: &Principal,
        id: &str,
        occurrence: Option<&str>,
    ) -> Result<Reminder, ReminderError> {
        if let Some(d) = occurrence {
            validate::date(d).map_err(|w| ReminderError::Invalid("occurrence", w))?;
        }
        let past = match occurrence {
            Some(d) => self
                .repo
                .occurrences(&p.user_id, Some(id), d, d)
                .await
                .map_err(backend)?
                .pop(),
            None => None,
        };
        self.mutate(p, id, |s, r| {
            if r.deleted_at.is_some() {
                return Err(ReminderError::NotFound);
            }
            let cursor = r.due_date.clone();
            if let (Some(d), Some(c)) = (occurrence, cursor.as_deref()) {
                if r.recurring() && d < c {
                    return match &past {
                        Some(row) if row.state == "completed" => Ok(None),
                        Some(_) => Ok(Some((
                            vec![s.row(r, d, "completed")],
                            vec![s.history(r, d, "completed", None)],
                        ))),
                        None => Err(ReminderError::Stale(
                            Box::new(r.clone()),
                            "that occurrence isn't this reminder's",
                        )),
                    };
                }
            }
            s.settle_into(r, occurrence, |s, r, missed| {
                match schedule::complete(s.clock, r, occurrence, s.now) {
                    Ok(Completion::Already) if missed.0.is_empty() => Ok(None),
                    Ok(Completion::Already) => Ok(Some(missed)),
                    Ok(Completion::Done { occurrence: d }) => {
                        let (mut rows, mut hist) = missed;
                        rows.push(s.row(r, &d, "completed"));
                        hist.push(s.history(r, &d, "completed", None));
                        Ok(Some((rows, hist)))
                    }
                    Err(why) => Err(ReminderError::Stale(Box::new(r.clone()), why)),
                }
            })
        })
        .await
    }

    /// Settle the missed policy first (so actions see the current
    /// occurrence), unless the client named an occurrence that settling would pass.
    fn settle_into<G>(
        &self,
        r: &mut Reminder,
        occurrence: Option<&str>,
        g: G,
    ) -> Result<Option<Effects>, ReminderError>
    where
        G: FnOnce(&Self, &mut Reminder, Effects) -> Result<Option<Effects>, ReminderError>,
    {
        let mut settled = r.clone();
        let missed = schedule::settle(self.clock, &mut settled, self.now);
        let keeps = occurrence.is_none_or(|d| settled.due_date.as_deref().is_some_and(|c| d >= c));
        let mut effects = (Vec::new(), Vec::new());
        if keeps && !missed.is_empty() {
            *r = settled;
            for d in &missed {
                effects.0.push(self.row(r, d, "missed"));
                effects.1.push(self.history(r, d, "missed", None));
            }
        }
        g(self, r, effects)
    }

    /// Undo a completion: a non-recurring reminder's, or a recurring one's
    /// latest (its date becomes current again), or one completed ahead.
    pub async fn uncomplete(
        &self,
        p: &Principal,
        id: &str,
        occurrence: Option<&str>,
    ) -> Result<Reminder, ReminderError> {
        let current = self.get(p, id).await?;
        let d = match (occurrence, current.due_date.as_deref()) {
            (Some(d), _) => {
                validate::date(d).map_err(|w| ReminderError::Invalid("occurrence", w))?
            }
            // A recurring reminder's open cursor: undo its latest completion.
            (None, Some(c)) if current.recurring() && current.completed_at.is_none() => self
                .repo
                .occurrences(&p.user_id, Some(id), "0000-01-01", c)
                .await
                .map_err(backend)?
                .into_iter()
                .rev()
                .find(|x| x.state == "completed" && x.occurrence.as_str() < c)
                .map_or_else(|| c.to_owned(), |x| x.occurrence),
            (None, c) => c.unwrap_or("-").to_owned(),
        };
        let since = match current.due_date.as_deref() {
            Some(c) if current.recurring() && d.as_str() < c => self
                .repo
                .occurrences(&p.user_id, Some(id), &d, c)
                .await
                .map_err(backend)?,
            _ => Vec::new(),
        };
        self.mutate(p, id, |s, r| {
            if r.deleted_at.is_some() {
                return Err(ReminderError::NotFound);
            }
            let undo = |s: &Self, r: &Reminder| {
                Ok(Some((
                    vec![OccurrenceChange::Remove(d.clone())],
                    vec![s.history(r, &d, "uncompleted", None)],
                )))
            };
            if !r.recurring() {
                if r.completed_at.take().is_none() {
                    return Ok(None);
                }
                return undo(s, r);
            }
            let cursor = r.due_date.clone().unwrap_or_default();
            if let Some(i) = r.completed_ahead.iter().position(|x| *x == d) {
                r.completed_ahead.remove(i);
                return undo(s, r);
            }
            if d == cursor {
                if r.completed_at.take().is_none() {
                    return Ok(None);
                }
                return undo(s, r);
            }
            // The latest completion: its row is the only one since.
            let latest = since
                .first()
                .is_some_and(|x| x.occurrence == d && x.state == "completed")
                && since.iter().skip(1).all(|x| x.occurrence >= cursor);
            if d > cursor || !latest || r.completed_at.is_some() {
                return Err(ReminderError::Stale(
                    Box::new(r.clone()),
                    "only the latest completion can be undone",
                ));
            }
            r.due_date = Some(d.clone());
            if let Some(rule) = r.recurrence.as_mut() {
                if r.advance == Advance::Completion {
                    rule.anchor = d.clone();
                }
            }
            r.snoozed_until = None;
            undo(s, r)
        })
        .await
    }

    pub async fn snooze(
        &self,
        p: &Principal,
        id: &str,
        input: Snooze,
    ) -> Result<Reminder, ReminderError> {
        let picked = [
            input.minutes.is_some(),
            input.until.is_some(),
            input.tomorrow,
        ]
        .iter()
        .filter(|x| **x)
        .count();
        if picked != 1 {
            return Err(ReminderError::Invalid(
                "snooze",
                "give one of minutes, until, or tomorrow",
            ));
        }
        self.mutate(p, id, |s, r| {
            if r.deleted_at.is_some() {
                return Err(ReminderError::NotFound);
            }
            s.settle_into(r, None, |s, r, effects| {
                let Some(cursor) = r.due_date.clone().filter(|_| r.completed_at.is_none()) else {
                    return Err(ReminderError::Invalid(
                        "snooze",
                        "only an open, dated reminder can be snoozed",
                    ));
                };
                if input
                    .occurrence
                    .as_ref()
                    .is_some_and(|o| *o != cursor && *o != r.occurrence_id())
                {
                    return Err(ReminderError::Stale(
                        Box::new(r.clone()),
                        "that occurrence has passed",
                    ));
                }
                let until = match (input.minutes, input.until) {
                    (Some(m), _) => s.now + i64::from(m) * MINUTE_MS,
                    (_, Some(u)) => u,
                    _ => {
                        let today = local_at(s.clock, &r.timezone, s.now)
                            .map(|(d, _)| d)
                            .unwrap_or(cursor.clone());
                        local_to_utc(
                            s.clock,
                            &r.timezone,
                            &validate::add_days(&today, 1),
                            DAY_ANCHOR_MINUTE,
                        )
                        .ok_or(ReminderError::Invalid("timezone", "unknown time zone"))?
                    }
                };
                if until <= s.now || until > s.now + MAX_SNOOZE_MS {
                    return Err(ReminderError::Invalid(
                        "snooze",
                        "pick a time in the next year",
                    ));
                }
                // Snoozing again to the same time is a replay: no change.
                if r.snoozed_until == Some(until) && effects.0.is_empty() {
                    return Ok(None);
                }
                r.snoozed_until = Some(until);
                let (rows, mut hist) = effects;
                hist.push(s.history(r, &cursor, "snoozed", Some(until)));
                Ok(Some((rows, hist)))
            })
        })
        .await
    }

    pub async fn history_of(
        &self,
        p: &Principal,
        id: &str,
    ) -> Result<Vec<HistoryEntry>, ReminderError> {
        self.get(p, id).await?;
        self.repo
            .history(&p.user_id, id, 200)
            .await
            .map_err(backend)
    }

    /// Dated reminders' occurrences in `from..=to` for the calendar: open
    /// ones from the rule, resolved ones from the history.
    pub async fn agenda(
        &self,
        p: &Principal,
        from: &str,
        to: &str,
    ) -> Result<Vec<AgendaItem>, ReminderError> {
        let from = validate::date(from).map_err(|w| ReminderError::Invalid("from", w))?;
        let to = validate::date(to).map_err(|w| ReminderError::Invalid("to", w))?;
        let span = validate::days_from_civil(&to) - validate::days_from_civil(&from);
        if !(0..MAX_AGENDA_DAYS).contains(&span) {
            return Err(ReminderError::Invalid(
                "to",
                "the range is at most 100 days",
            ));
        }
        let all = self.repo.list(&p.user_id).await.map_err(backend)?;
        let rows = self
            .repo
            .occurrences(&p.user_id, None, &from, &to)
            .await
            .map_err(backend)?;
        let mut out = Vec::new();
        let mut push = |r: &Reminder, date: &str, state: &'static str| {
            out.push(AgendaItem {
                reminder_id: r.id.clone(),
                occurrence_id: occurrence_id(&r.id, Some(date)),
                date: date.to_owned(),
                time: r.due_time.clone(),
                at: schedule::start_at(self.clock, r, date),
                title: r.title.clone(),
                state,
                recurring: r.recurring(),
                priority: r.priority,
                flagged: r.flagged,
            })
        };
        for r in all
            .iter()
            .filter(|r| r.deleted_at.is_none() && r.due_date.is_some())
        {
            let mut shown = HashSet::new();
            for row in rows.iter().filter(|x| x.reminder_id == r.id) {
                if shown.insert(row.occurrence.clone()) {
                    push(
                        r,
                        &row.occurrence,
                        if row.state == "completed" {
                            "completed"
                        } else {
                            "missed"
                        },
                    );
                }
            }
            for d in r
                .completed_ahead
                .iter()
                .filter(|d| from <= **d && **d <= to)
            {
                if shown.insert(d.clone()) {
                    push(r, d, "completed");
                }
            }
            for d in schedule::occurrences(r, &to, 500) {
                if d >= from && shown.insert(d.clone()) {
                    let overdue =
                        schedule::overdue_at(self.clock, r, &d).is_some_and(|t| t <= self.now);
                    push(r, &d, if overdue { "overdue" } else { "open" });
                }
            }
        }
        out.sort_by(|a, b| {
            (a.date.as_str(), a.at, a.title.as_str()).cmp(&(
                b.date.as_str(),
                b.at,
                b.title.as_str(),
            ))
        });
        out.truncate(MAX_AGENDA_ITEMS);
        Ok(out)
    }

    /// A linked task was completed: complete the current occurrence of each
    /// reminder that follows it. One-way: completing a reminder never
    /// completes a task, so there are no loops.
    pub async fn task_completed(&self, p: &Principal, task_id: &str) -> Result<u32, ReminderError> {
        let linked = self
            .repo
            .linked_to_task(&p.user_id, task_id)
            .await
            .map_err(backend)?;
        let mut n = 0;
        for r in linked
            .iter()
            .filter(|r| r.complete_with_task && r.completed_at.is_none() && r.deleted_at.is_none())
        {
            self.complete(p, &r.id, None).await?;
            n += 1;
        }
        Ok(n)
    }

    pub async fn lists(&self, p: &Principal) -> Result<Vec<ReminderList>, ReminderError> {
        self.repo.lists(&p.user_id).await.map_err(backend)
    }

    pub async fn create_list(
        &self,
        p: &Principal,
        input: NewList,
    ) -> Result<ReminderList, ReminderError> {
        let all = self.repo.lists(&p.user_id).await.map_err(backend)?;
        let id = match input.id {
            Some(id) if !is_id(&id) => {
                return Err(ReminderError::Invalid("id", "expected 32 hex characters"))
            }
            Some(id) if all.iter().any(|l| l.id == id) => return Err(ReminderError::Exists),
            Some(id) => id,
            None => new_id(self.entropy),
        };
        let name = list_name(&input.name).map_err(bad)?;
        if all
            .iter()
            .any(|l| l.name.to_lowercase() == name.to_lowercase())
        {
            return Err(ReminderError::Invalid("name", "already used"));
        }
        if all.len() as u32 >= MAX_LISTS {
            return Err(ReminderError::Invalid(
                "lists",
                "limit of 100 lists reached",
            ));
        }
        let color = categories::color(input.color.as_deref().unwrap_or(categories::DEFAULT_COLOR))
            .map_err(|_| ReminderError::Invalid("color", "expected #rrggbb"))?;
        let l = ReminderList {
            id,
            owner_id: p.user_id.clone(),
            name,
            color,
            created_at: self.now,
            updated_at: self.now,
        };
        self.repo.insert_list(&l).await.map_err(backend)?;
        Ok(l)
    }

    pub async fn update_list(
        &self,
        p: &Principal,
        id: &str,
        patch: ListPatch,
    ) -> Result<ReminderList, ReminderError> {
        let all = self.repo.lists(&p.user_id).await.map_err(backend)?;
        let mut l = all
            .iter()
            .find(|l| l.id == id)
            .cloned()
            .ok_or(ReminderError::NotFound)?;
        if let Some(n) = patch.name {
            let n = list_name(&n).map_err(bad)?;
            if all
                .iter()
                .any(|x| x.id != l.id && x.name.to_lowercase() == n.to_lowercase())
            {
                return Err(ReminderError::Invalid("name", "already used"));
            }
            l.name = n;
        }
        if let Some(c) = patch.color {
            l.color = categories::color(&c)
                .map_err(|_| ReminderError::Invalid("color", "expected #rrggbb"))?;
        }
        l.updated_at = self.now;
        if !self.repo.update_list(&l).await.map_err(backend)? {
            return Err(ReminderError::NotFound);
        }
        Ok(l)
    }

    pub async fn delete_list(&self, p: &Principal, id: &str) -> Result<(), ReminderError> {
        if is_id(id)
            && self
                .repo
                .delete_list(&p.user_id, id, self.now)
                .await
                .map_err(backend)?
        {
            Ok(())
        } else {
            Err(ReminderError::NotFound)
        }
    }
}

/// The scheduler step (cron): visit every reminder whose wake time has
/// come, each under its own owner, and save what changed. Returns the
/// alerts to send; the caller dedups them in the notification log.
pub async fn run_scheduler<R: ReminderRepository, E: Entropy + ?Sized, C: ZoneClock>(
    repo: &R,
    entropy: &E,
    clock: &C,
    now: i64,
    limit: u32,
) -> Result<Vec<Scanned>, String> {
    let mut out = Vec::new();
    for before in repo.due_for_scan(now, limit).await? {
        let mut next = before.clone();
        let scan = schedule::scan(clock, &mut next, now);
        stamp(&before, &mut next, now);
        let mut rows = Vec::new();
        let mut hist = Vec::new();
        for d in &scan.missed {
            let entry = |action: &str| HistoryEntry {
                id: new_id(entropy),
                reminder_id: next.id.clone(),
                occurrence: d.clone(),
                action: action.to_owned(),
                at: now,
                until: None,
            };
            rows.push(OccurrenceChange::Set(OccurrenceRow {
                reminder_id: next.id.clone(),
                occurrence: d.clone(),
                state: "missed".into(),
                at: now,
            }));
            hist.push(entry("missed"));
        }
        // The alert to send, and any it superseded, are logged with the scan
        // itself: a notice exists exactly when the high-water mark passed it.
        let notice = |f: &Fire, state: &str| Notice {
            id: new_id(entropy),
            owner_id: next.owner_id.clone(),
            reminder_id: next.id.clone(),
            occurrence: f.occurrence.clone(),
            slot: f.slot.clone(),
            due_at: f.at,
            state: state.to_owned(),
            created_at: now,
        };
        let mut notices: Vec<Notice> = scan
            .superseded
            .iter()
            .map(|f| notice(f, notice::SUPERSEDED))
            .collect();
        notices.extend(scan.send.iter().map(|f| notice(f, notice::PENDING)));
        // A lost race means someone edited it: the next minute's scan sees the new copy.
        if repo
            .save(&next, before.version, &rows, &hist, &notices)
            .await?
        {
            out.push(Scanned {
                owner_id: next.owner_id.clone(),
                reminder_id: next.id.clone(),
                scan,
            });
        }
    }
    Ok(out)
}

/// The text a notification shows, given a fire.
pub fn fire_label(r: &Reminder, fire: &Fire) -> String {
    match (&r.due_time, fire.occurrence.as_str()) {
        (Some(t), d) => format!("{d} {t}"),
        (None, d) => d.to_owned(),
    }
}
