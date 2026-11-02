//! Reminder and category commands (D-049). Like every command they resolve
//! names to the caller's own items when the plan is made and run through
//! the Reminders and Categories services as the caller, so ownership,
//! links, and bounds are checked there again.

use super::*;
use crate::categories::{CategoryError, CategoryService, NewCategory};
use crate::reminders::{
    self, zone::local_at, NewReminder, Reminder, ReminderError, ReminderPatch, ReminderService,
    Snooze,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderView {
    /// Every open reminder, soonest first.
    Open,
    Overdue,
    /// Due today, and overdue.
    Today,
    /// Due in the next seven days.
    Upcoming,
    /// Open and completed reminders whose title or notes match.
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryTarget {
    Task,
    Reminder,
}

impl CategoryTarget {
    pub fn noun(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Reminder => "reminder",
        }
    }
}

/// Fields a reminder edit sets (`None` leaves a field alone).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    /// `Some(None)` makes it an all-day reminder.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "validate::double_option"
    )]
    pub due_time: Option<Option<String>>,
    /// "in 2 hours": turned into a date and time when the plan is made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_minutes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flagged: Option<bool>,
    /// A list named by the user, resolved to `list_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_id: Option<String>,
}

impl ReminderChange {
    pub fn is_empty(&self) -> bool {
        self.due_date.is_none()
            && self.due_time.is_none()
            && self.after_minutes.is_none()
            && self.priority.is_none()
            && self.flagged.is_none()
            && self.list.is_none()
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        match (&self.due_date, &self.due_time) {
            (Some(d), Some(Some(t))) => parts.push(format!("move to {} {t}", day_label(d))),
            (Some(d), _) => parts.push(format!("move to {}", day_label(d))),
            (None, Some(Some(t))) => parts.push(format!("move to {t}")),
            (None, Some(None)) => parts.push("make it all-day".into()),
            (None, None) => {}
        }
        if let Some(m) = self.after_minutes {
            parts.push(format!("move to {} from now", planner::duration(m)));
        }
        if let Some(p) = self.priority {
            parts.push(format!("{} priority", p.as_str()));
        }
        match self.flagged {
            Some(true) => parts.push("flag it".into()),
            Some(false) => parts.push("unflag it".into()),
            None => {}
        }
        if let Some(l) = &self.list {
            parts.push(format!("move to the list {}", quote(l, 40)));
        }
        parts.join(", ")
    }
}

/// Most reminders one "remind me about my deadlines" proposes.
const MAX_DEADLINE_REMINDERS: usize = 10;
/// How far ahead "in N minutes" may reach.
const MAX_AFTER_MINUTES: u32 = 7 * 24 * 60;

fn reminder_failure(e: ReminderError) -> ActionResult {
    failure(match e {
        ReminderError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        ReminderError::Exists => "That reminder was already created.".into(),
        ReminderError::NotFound => "That reminder no longer exists.".into(),
        ReminderError::Conflict(..) => "The reminder changed meanwhile; nothing was saved.".into(),
        ReminderError::Stale(_, why) => format!("Not done: {why}."),
        ReminderError::Backend(_) => "Reminders are unavailable right now.".into(),
    })
}

fn category_failure(e: CategoryError) -> ActionResult {
    failure(match e {
        CategoryError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        CategoryError::Exists => "That category already exists.".into(),
        CategoryError::NotFound => "That category or task no longer exists.".into(),
        CategoryError::Backend(_) => "Categories are unavailable right now.".into(),
    })
}

fn unavailable(what: &str) -> JarvisError {
    JarvisError::Unresolved(format!("Couldn't read your {what}."))
}

fn hhmm(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// "in 90 minutes" from the caller's clock, as a local date and time.
fn after(clock: &Clock, minutes: u32) -> Result<(String, String), JarvisError> {
    if minutes == 0 || minutes > MAX_AFTER_MINUTES {
        return Err(JarvisError::Invalid(
            "minutes",
            "must be 1 minute to 7 days",
        ));
    }
    let total = clock.minutes + minutes;
    Ok((
        validate::add_days(&clock.today, i64::from(total / 1440)),
        hhmm(total % 1440),
    ))
}

/// Open and dated reminders: is this one overdue for the owner's `today`?
pub fn is_overdue(r: &Reminder, now: i64, today: &str) -> bool {
    if r.completed_at.is_some() || r.deleted_at.is_some() {
        return false;
    }
    match (&r.due_date, &r.due_time, r.due_at) {
        (None, ..) => false,
        (Some(_), Some(_), Some(at)) => at <= now,
        (Some(d), ..) => d.as_str() < today,
    }
}

fn when_label(r: &Reminder) -> String {
    match (&r.due_date, &r.due_time) {
        (Some(d), Some(t)) => format!("{} {t}", day_label(d)),
        (Some(d), None) => day_label(d),
        _ => "no date".into(),
    }
}

impl<'a, R: JarvisRepository, E: Entropy + ?Sized> JarvisService<'a, R, E> {
    fn reminders<'s, C: CommandTargets>(
        &'s self,
        t: &'s C,
    ) -> ReminderService<'s, C::Reminders, E, C::Zones> {
        ReminderService::new(t.reminders(), self.entropy, t.zones(), self.now)
    }

    /// The open reminder `id` names (checked to be the caller's), else the
    /// one whose title matches `query`.
    async fn reminder_ref<C: CommandTargets>(
        &self,
        p: &Principal,
        query: &str,
        id: Option<&str>,
        t: &C,
    ) -> Result<Reminder, JarvisError> {
        let service = self.reminders(t);
        if let Some(id) = id {
            let r = service.get(p, id).await.map_err(|e| match e {
                ReminderError::Backend(b) => JarvisError::Backend(b),
                _ => JarvisError::Unresolved("That reminder no longer exists.".into()),
            })?;
            if r.deleted_at.is_some() || r.completed_at.is_some() {
                return Err(JarvisError::Unresolved(format!(
                    "“{}” isn't open any more.",
                    r.title
                )));
            }
            return Ok(r);
        }
        let open: Vec<Reminder> = service
            .list(p)
            .await
            .map_err(|_| unavailable("reminders"))?
            .into_iter()
            .filter(|r| r.deleted_at.is_none() && r.completed_at.is_none())
            .collect();
        pick(&open, |r| &r.title, query)
            .cloned()
            .map_err(|m| ambiguity(query, "reminder", &m, |r| &r.title, |r| &r.id))
    }

    /// Resolve and check a reminder or category command into plan steps.
    pub(super) async fn expand_reminder<C: CommandTargets>(
        &self,
        p: &Principal,
        command: Command,
        clock: &Clock,
        t: &C,
    ) -> Result<Vec<PlannedAction>, JarvisError> {
        match command {
            Command::CreateReminder { .. } => {
                let (command, notes) = self.resolve_create(p, command, clock, t).await?;
                let mut a = PlannedAction::new(command);
                a.notes = notes;
                Ok(vec![a])
            }
            Command::CompleteReminder {
                query, reminder_id, ..
            } => {
                let r = self
                    .reminder_ref(p, &query, reminder_id.as_deref(), t)
                    .await?;
                let mut a = PlannedAction::new(Command::CompleteReminder {
                    query,
                    reminder_id: Some(r.id.clone()),
                    title: Some(r.title.clone()),
                });
                if r.recurrence.is_some() {
                    a.notes
                        .push("It repeats: this completes the current occurrence only.".into());
                }
                Ok(vec![a])
            }
            Command::SnoozeReminder {
                query,
                minutes,
                tomorrow,
                reminder_id,
                ..
            } => {
                if minutes.is_some() == tomorrow {
                    return Err(JarvisError::Invalid("snooze", "give minutes or tomorrow"));
                }
                if minutes
                    .is_some_and(|m| m == 0 || i64::from(m) * 60_000 > reminders::MAX_SNOOZE_MS)
                {
                    return Err(JarvisError::Invalid(
                        "minutes",
                        "pick a time in the next year",
                    ));
                }
                let r = self
                    .reminder_ref(p, &query, reminder_id.as_deref(), t)
                    .await?;
                if r.due_date.is_none() {
                    return Err(JarvisError::Unresolved(format!(
                        "“{}” has no date, so there's nothing to snooze. Give it a date instead.",
                        r.title
                    )));
                }
                Ok(vec![PlannedAction::new(Command::SnoozeReminder {
                    query,
                    minutes,
                    tomorrow,
                    reminder_id: Some(r.id.clone()),
                    title: Some(r.title),
                })])
            }
            Command::UpdateReminder {
                query,
                mut change,
                reminder_id,
                ..
            } => {
                if change.is_empty() {
                    return Err(JarvisError::Invalid("change", "nothing to change"));
                }
                let r = self
                    .reminder_ref(p, &query, reminder_id.as_deref(), t)
                    .await?;
                if let Some(m) = change.after_minutes.take() {
                    let (d, time) = after(clock, m)?;
                    change.due_date = Some(d);
                    change.due_time = Some(Some(time));
                }
                if let Some(d) = &change.due_date {
                    validate::date(d).map_err(|w| JarvisError::Invalid("due_date", w))?;
                } else if let Some(Some(time)) = &change.due_time {
                    // A time alone keeps the date (today if there was none).
                    let _ = validate::clock(time, false)
                        .map_err(|w| JarvisError::Invalid("due_time", w))?;
                    change.due_date = Some(r.due_date.clone().unwrap_or(clock.today.clone()));
                }
                if let Some(name) = &change.list {
                    let lists = self
                        .reminders(t)
                        .lists(p)
                        .await
                        .map_err(|_| unavailable("reminder lists"))?;
                    let l = pick(&lists, |l| &l.name, name)
                        .map_err(|m| ambiguity(name, "list", &m, |l| &l.name, |l| &l.id))?;
                    change.list = Some(l.name.clone());
                    change.list_id = Some(l.id.clone());
                }
                Ok(vec![PlannedAction::new(Command::UpdateReminder {
                    query,
                    change,
                    reminder_id: Some(r.id.clone()),
                    title: Some(r.title),
                })])
            }
            Command::SetCategories {
                target,
                query,
                add,
                remove,
                id,
                ..
            } => {
                if add.len() + remove.len() > crate::categories::MAX_PER_ITEM {
                    return Err(JarvisError::Invalid(
                        "categories",
                        "at most 12 categories per item",
                    ));
                }
                for n in add.iter().chain(&remove) {
                    validate::line(n, crate::categories::MAX_NAME)
                        .map_err(|w| JarvisError::Invalid("category", w))?;
                }
                let (id, title) = match target {
                    CategoryTarget::Task => {
                        let x = self.task_ref(p, &query, id.as_deref(), t).await?;
                        (x.id, x.title)
                    }
                    CategoryTarget::Reminder => {
                        let x = self.reminder_ref(p, &query, id.as_deref(), t).await?;
                        (x.id, x.title)
                    }
                };
                let have = CategoryService::new(t.categories(), self.entropy, self.now)
                    .list(p)
                    .await
                    .map_err(|_| unavailable("categories"))?
                    .categories;
                let known = |n: &String| have.iter().any(|c| c.name.eq_ignore_ascii_case(n));
                if let Some(n) = remove.iter().find(|n| !known(n)) {
                    return Err(JarvisError::Unresolved(format!(
                        "You have no category called “{n}”."
                    )));
                }
                let fresh: Vec<&String> = add.iter().filter(|n| !known(n)).collect();
                let mut a = PlannedAction::new(Command::SetCategories {
                    target,
                    query,
                    add: add.clone(),
                    remove,
                    id: Some(id),
                    title: Some(title),
                });
                if !fresh.is_empty() {
                    a.notes.push(format!(
                        "New {}: {}.",
                        if fresh.len() == 1 {
                            "category"
                        } else {
                            "categories"
                        },
                        fresh
                            .iter()
                            .map(|n| quote(n, 40))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                Ok(vec![a])
            }
            Command::CreateCategory { name, .. } => {
                let name = validate::line(&name, crate::categories::MAX_NAME)
                    .map_err(|w| JarvisError::Invalid("name", w))?;
                let have = CategoryService::new(t.categories(), self.entropy, self.now)
                    .list(p)
                    .await
                    .map_err(|_| unavailable("categories"))?
                    .categories;
                if let Some(c) = have.iter().find(|c| c.name.eq_ignore_ascii_case(&name)) {
                    return Err(JarvisError::Unresolved(format!(
                        "You already have the category “{}”.",
                        c.name
                    )));
                }
                Ok(vec![PlannedAction::new(Command::CreateCategory {
                    name,
                    id: Some(new_id(self.entropy)),
                })])
            }
            Command::RemindDeadlines { by } => self.expand_deadlines(p, &by, clock, t).await,
            other => Ok(vec![PlannedAction::new(other)]),
        }
    }

    async fn resolve_create<C: CommandTargets>(
        &self,
        p: &Principal,
        command: Command,
        clock: &Clock,
        t: &C,
    ) -> Result<(Command, Vec<String>), JarvisError> {
        let Command::CreateReminder {
            id,
            title,
            mut due_date,
            mut due_time,
            after_minutes,
            repeat,
            task,
            task_id,
            project,
            project_id,
            priority,
            with_task,
            ..
        } = command
        else {
            unreachable!("only CreateReminder");
        };
        if id.as_deref().is_some_and(|i| !is_id(i)) {
            return Err(JarvisError::Invalid("id", "expected 32 hex characters"));
        }
        let title = reminders::model::title(&title).map_err(|(f, w)| JarvisError::Invalid(f, w))?;
        let mut notes = Vec::new();
        if let Some(m) = after_minutes {
            let (d, time) = after(clock, m)?;
            due_date = Some(d);
            due_time = Some(time);
        }
        if let Some(time) = &due_time {
            let at =
                validate::clock(time, false).map_err(|w| JarvisError::Invalid("due_time", w))?;
            if due_date.is_none() {
                // "at 8 pm": today, unless that has passed.
                due_date = Some(if at > clock.minutes {
                    clock.today.clone()
                } else {
                    validate::add_days(&clock.today, 1)
                });
            }
        }
        if let Some(d) = &due_date {
            let d = validate::date(d).map_err(|w| JarvisError::Invalid("due_date", w))?;
            let past_time = d == clock.today
                && due_time
                    .as_deref()
                    .and_then(|x| validate::clock(x, false).ok())
                    .is_some_and(|m| m <= clock.minutes);
            if repeat.is_none() && (d < clock.today || past_time) {
                return Err(JarvisError::Unresolved(format!(
                    "{} has passed. Say a later time for “{title}”.",
                    match &due_time {
                        Some(x) => format!("{} {x}", day_label(&d)),
                        None => day_label(&d),
                    }
                )));
            }
        } else {
            notes.push("No date, so it won't notify. Add one to be alerted.".into());
        }
        let project_id = match (project_id, project) {
            (Some(id), _) => {
                let x = ProjectService::new(t.projects(), self.entropy, self.now)
                    .get(p, &id)
                    .await
                    .map_err(|_| {
                        JarvisError::Unresolved("That project no longer exists.".into())
                    })?;
                Some(x.id)
            }
            (None, Some(q)) if q == "this project" => {
                return Err(JarvisError::Unresolved(
                    "Which project? Open it first, or say its name.".into(),
                ))
            }
            (None, Some(q)) => {
                let active = ProjectService::new(t.projects(), self.entropy, self.now)
                    .list(p, false)
                    .await
                    .map_err(|_| unavailable("projects"))?;
                let x = pick(&active, |x| &x.name, &q)
                    .map_err(|m| ambiguity(&q, "active project", &m, |x| &x.name, |x| &x.id))?;
                Some(x.id.clone())
            }
            (None, None) => None,
        };
        let task_id = match (task_id, task) {
            (None, Some(q)) if q == "this task" => {
                return Err(JarvisError::Unresolved(
                    "Which task? Open it first, or say its title.".into(),
                ))
            }
            (id, Some(q)) => Some(self.task_ref(p, &q, id.as_deref(), t).await?.id),
            (Some(id), None) => Some(self.task_ref(p, "", Some(&id), t).await?.id),
            (None, None) => None,
        };
        let timezone = match &clock.zone {
            Some(z) => z.clone(),
            None => {
                if due_time.is_some() {
                    notes.push(
                        "This device didn't send its time zone, so the time is in UTC.".into(),
                    );
                }
                "UTC".into()
            }
        };
        Ok((
            Command::CreateReminder {
                id: Some(id.unwrap_or_else(|| new_id(self.entropy))),
                title,
                due_date,
                due_time,
                after_minutes: None,
                repeat,
                task: None,
                task_id,
                project: None,
                project_id,
                priority,
                timezone: Some(timezone),
                with_task,
            },
            notes,
        ))
    }

    /// Reminders for open tasks due by `by` that no open reminder covers
    /// (one each, completing with the task). Proposed, never automatic.
    async fn expand_deadlines<C: CommandTargets>(
        &self,
        p: &Principal,
        by: &str,
        clock: &Clock,
        t: &C,
    ) -> Result<Vec<PlannedAction>, JarvisError> {
        let by = validate::date(by).map_err(|w| JarvisError::Invalid("by", w))?;
        let open = TaskService::new(t.tasks(), self.entropy, self.now)
            .list(p, Filter::Open)
            .await
            .map_err(|_| unavailable("tasks"))?;
        let covered: Vec<String> = self
            .reminders(t)
            .list(p)
            .await
            .map_err(|_| unavailable("reminders"))?
            .into_iter()
            .filter(|r| r.deleted_at.is_none() && r.completed_at.is_none())
            .filter_map(|r| r.task_id)
            .collect();
        let mut due: Vec<&tasks::Task> = open
            .iter()
            .filter(|x| x.due_date.as_deref().is_some_and(|d| d <= by.as_str()))
            .filter(|x| !covered.contains(&x.id))
            .collect();
        due.sort_by(|a, b| a.due_date.cmp(&b.due_date));
        if due.is_empty() {
            return Err(JarvisError::Unresolved(format!(
                "No open task due by {} is without a reminder.",
                day_label(&by)
            )));
        }
        let more = due.len().saturating_sub(MAX_DEADLINE_REMINDERS);
        let mut out = Vec::new();
        for x in due.into_iter().take(MAX_DEADLINE_REMINDERS) {
            let date = x
                .due_date
                .clone()
                .unwrap_or_default()
                .max(clock.today.clone());
            let mut a = PlannedAction::new(Command::CreateReminder {
                id: Some(new_id(self.entropy)),
                title: x.title.clone(),
                due_date: Some(date),
                due_time: None,
                after_minutes: None,
                repeat: None,
                task: None,
                task_id: Some(x.id.clone()),
                project: None,
                project_id: x.project_id.clone(),
                priority: Some(x.priority),
                timezone: Some(clock.zone.clone().unwrap_or_else(|| "UTC".into())),
                with_task: true,
            });
            if x.due_date
                .as_deref()
                .is_some_and(|d| d < clock.today.as_str())
            {
                a.notes
                    .push("The task is overdue, so the reminder is for today.".into());
            }
            out.push(a);
        }
        if more > 0 {
            if let Some(last) = out.last_mut() {
                last.notes
                    .push(format!("{more} more tasks are due; ask again after these."));
            }
        }
        Ok(out)
    }

    /// Run one reminder or category step.
    pub(super) async fn run_reminder<C: CommandTargets>(
        &self,
        p: &Principal,
        command: &Command,
        t: &C,
    ) -> ActionResult {
        let service = self.reminders(t);
        let item = |r: &Reminder| {
            Some(Item {
                kind: ItemKind::Reminder,
                id: r.id.clone(),
                date: r.due_date.clone().unwrap_or_default(),
            })
        };
        match command {
            Command::CreateReminder {
                id: Some(id),
                title,
                due_date,
                due_time,
                repeat,
                task_id,
                project_id,
                priority,
                timezone,
                with_task,
                ..
            } => {
                let mut input = NewReminder::titled(title);
                input.id = Some(id.clone());
                input.due_date = due_date.clone();
                input.due_time = due_time.clone();
                input.timezone = timezone.clone();
                input.recurrence = repeat.clone();
                input.task_id = task_id.clone();
                input.project_id = project_id.clone();
                input.complete_with_task = *with_task && task_id.is_some();
                if let Some(pr) = priority {
                    input.priority = *pr;
                }
                match service.create(p, input).await {
                    Ok(r) => ActionResult {
                        ok: true,
                        message: match (&r.due_date, &r.recurrence) {
                            (Some(_), Some(_)) => {
                                format!(
                                    "Reminder “{}” set, repeating from {}.",
                                    r.title,
                                    when_label(&r)
                                )
                            }
                            (Some(_), None) => {
                                format!("Reminder “{}” set for {}.", r.title, when_label(&r))
                            }
                            _ => format!("Reminder “{}” saved (no date).", r.title),
                        },
                        item: item(&r),
                        undo: Some(undo::UndoOp::DeleteReminder {
                            id: r.id.clone(),
                            title: r.title.clone(),
                            updated_at: r.updated_at,
                        }),
                        ..Default::default()
                    },
                    Err(e) => reminder_failure(e),
                }
            }
            Command::CompleteReminder {
                reminder_id: Some(id),
                ..
            } => {
                let before = service.get(p, id).await.ok();
                match service.complete(p, id, None).await {
                    Ok(r) => {
                        let message = match (&r.completed_at, &r.due_date) {
                            (None, Some(_)) => format!(
                                "Completed this time; “{}” is next {}.",
                                r.title,
                                when_label(&r)
                            ),
                            _ => format!("Completed “{}”.", r.title),
                        };
                        ActionResult {
                            ok: true,
                            message,
                            item: item(&r),
                            undo: before.filter(|b| b.completed_at.is_none()).map(|b| {
                                undo::UndoOp::UncompleteReminder {
                                    id: r.id.clone(),
                                    title: r.title.clone(),
                                    occurrence: b.due_date,
                                    updated_at: r.updated_at,
                                }
                            }),
                            ..Default::default()
                        }
                    }
                    Err(e) => reminder_failure(e),
                }
            }
            Command::SnoozeReminder {
                reminder_id: Some(id),
                minutes,
                tomorrow,
                ..
            } => {
                let input = Snooze {
                    minutes: *minutes,
                    tomorrow: *tomorrow,
                    ..Default::default()
                };
                match service.snooze(p, id, input).await {
                    Ok(r) => {
                        let until = r
                            .snoozed_until
                            .and_then(|u| local_at(t.zones(), &r.timezone, u))
                            .map(|(d, m)| format!(" until {} {}", day_label(&d), hhmm(m)))
                            .unwrap_or_default();
                        ActionResult {
                            ok: true,
                            message: format!("Snoozed “{}”{until}.", r.title),
                            item: item(&r),
                            ..Default::default()
                        }
                    }
                    Err(e) => reminder_failure(e),
                }
            }
            Command::UpdateReminder {
                reminder_id: Some(id),
                change,
                ..
            } => {
                let Ok(before) = service.get(p, id).await else {
                    return failure("That reminder no longer exists.".into());
                };
                let patch = ReminderPatch {
                    base_version: Some(before.version),
                    due_date: change.due_date.clone().map(Some),
                    due_time: change.due_time.clone(),
                    priority: change.priority,
                    flagged: change.flagged,
                    list_id: change.list_id.clone().map(Some),
                    ..Default::default()
                };
                match service.update(p, id, patch).await {
                    Ok(r) => ActionResult {
                        ok: true,
                        message: format!("Updated “{}”: {}.", r.title, when_label(&r)),
                        item: item(&r),
                        undo: Some(undo::UndoOp::RestoreReminder {
                            id: r.id.clone(),
                            title: r.title.clone(),
                            due_date: before.due_date,
                            due_time: before.due_time,
                            priority: before.priority,
                            flagged: before.flagged,
                            list_id: before.list_id,
                            updated_at: r.updated_at,
                        }),
                        ..Default::default()
                    },
                    Err(e) => reminder_failure(e),
                }
            }
            Command::ListReminders { view, query, today } => {
                self.list_reminders(p, *view, query.as_deref(), today, t)
                    .await
            }
            Command::SetCategories {
                target,
                add,
                remove,
                id: Some(id),
                title: Some(title),
                ..
            } => {
                self.set_categories(p, *target, id, title, add, remove, t)
                    .await
            }
            Command::CreateCategory { name, id } => {
                let input = NewCategory {
                    id: id.clone(),
                    name: name.clone(),
                    color: None,
                    icon: None,
                };
                match CategoryService::new(t.categories(), self.entropy, self.now)
                    .create(p, input)
                    .await
                {
                    Ok(c) => ActionResult {
                        ok: true,
                        message: format!("Created the category “{}”.", c.name),
                        ..Default::default()
                    },
                    Err(e) => category_failure(e),
                }
            }
            _ => failure("No reminder was resolved for that step.".into()),
        }
    }

    async fn list_reminders<C: CommandTargets>(
        &self,
        p: &Principal,
        view: ReminderView,
        query: Option<&str>,
        today: &str,
        t: &C,
    ) -> ActionResult {
        let all = match self.reminders(t).list(p).await {
            Ok(all) => all,
            Err(e) => return reminder_failure(e),
        };
        let week = validate::add_days(today, 7);
        let q = query.map(str::to_lowercase);
        let mut shown: Vec<&Reminder> = all
            .iter()
            .filter(|r| r.deleted_at.is_none())
            .filter(|r| match view {
                ReminderView::Search => q.as_deref().is_some_and(|q| {
                    r.title.to_lowercase().contains(q) || r.notes.to_lowercase().contains(q)
                }),
                _ if r.completed_at.is_some() => false,
                ReminderView::Open => true,
                ReminderView::Overdue => is_overdue(r, self.now, today),
                ReminderView::Today => {
                    is_overdue(r, self.now, today) || r.due_date.as_deref() == Some(today)
                }
                ReminderView::Upcoming => r
                    .due_date
                    .as_deref()
                    .is_some_and(|d| d <= week.as_str() && !is_overdue(r, self.now, today)),
            })
            .collect();
        shown.sort_by(|a, b| {
            (
                a.completed_at.is_some(),
                a.due_date.is_none(),
                &a.due_date,
                &a.due_time,
            )
                .cmp(&(
                    b.completed_at.is_some(),
                    b.due_date.is_none(),
                    &b.due_date,
                    &b.due_time,
                ))
        });
        let what = match view {
            ReminderView::Open => "open reminders".to_owned(),
            ReminderView::Overdue => "overdue reminders".to_owned(),
            ReminderView::Today => "reminders for today".to_owned(),
            ReminderView::Upcoming => "reminders in the next week".to_owned(),
            ReminderView::Search => {
                format!("reminders matching {}", quote(query.unwrap_or(""), 40))
            }
        };
        if shown.is_empty() {
            return ActionResult {
                ok: true,
                message: format!("No {what}."),
                ..Default::default()
            };
        }
        let names: Vec<String> = shown
            .iter()
            .take(LIST_PREVIEW)
            .map(|r| {
                let done = if r.completed_at.is_some() {
                    ", done"
                } else {
                    ""
                };
                let flag = if r.flagged { " ⚑" } else { "" };
                format!("“{}”{flag} ({}{done})", r.title, when_label(r))
            })
            .collect();
        let more = if shown.len() > LIST_PREVIEW {
            format!(" and {} more", shown.len() - LIST_PREVIEW)
        } else {
            String::new()
        };
        let mut message = format!("{} {what}: {}{more}.", shown.len(), names.join("; "));
        // A gentle nudge about the longest-neglected one, never a lecture.
        let stale = validate::add_days(today, -7);
        if let Some(old) = shown.iter().find(|r| {
            is_overdue(r, self.now, today)
                && r.due_date.as_deref().is_some_and(|d| d < stale.as_str())
        }) {
            message.push_str(&format!(
                " “{}” has waited since {}: complete it, snooze it, or give it a new date?",
                old.title,
                day_label(old.due_date.as_deref().unwrap_or(today))
            ));
        }
        ActionResult {
            ok: true,
            message,
            item: shown.first().map(|r| Item {
                kind: ItemKind::Reminder,
                id: r.id.clone(),
                date: r.due_date.clone().unwrap_or_default(),
            }),
            ..Default::default()
        }
    }

    /// Add and remove categories by name on a task or reminder, creating
    /// names that don't exist yet.
    #[allow(clippy::too_many_arguments)]
    async fn set_categories<C: CommandTargets>(
        &self,
        p: &Principal,
        target: CategoryTarget,
        id: &str,
        title: &str,
        add: &[String],
        remove: &[String],
        t: &C,
    ) -> ActionResult {
        let cats = CategoryService::new(t.categories(), self.entropy, self.now);
        let listing = match cats.list(p).await {
            Ok(l) => l,
            Err(e) => return category_failure(e),
        };
        let find = |n: &str| {
            listing
                .categories
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(n))
                .map(|c| c.id.clone())
        };
        let before: Vec<String> = match target {
            CategoryTarget::Task => listing
                .tasks
                .iter()
                .filter(|a| a.task_id == id)
                .map(|a| a.category_id.clone())
                .collect(),
            CategoryTarget::Reminder => match self.reminders(t).get(p, id).await {
                Ok(r) => r.category_ids,
                Err(e) => return reminder_failure(e),
            },
        };
        let mut ids = before.clone();
        let mut created = Vec::new();
        for n in add {
            let cid = match find(n) {
                Some(c) => c,
                None => {
                    let input = NewCategory {
                        id: None,
                        name: n.clone(),
                        color: None,
                        icon: None,
                    };
                    match cats.create(p, input).await {
                        Ok(c) => {
                            created.push(c.name.clone());
                            c.id
                        }
                        Err(e) => return category_failure(e),
                    }
                }
            };
            if !ids.contains(&cid) {
                ids.push(cid);
            }
        }
        let gone: Vec<String> = remove.iter().filter_map(|n| find(n)).collect();
        ids.retain(|c| !gone.contains(c));
        let saved = match target {
            CategoryTarget::Task => cats.set_task(p, id, &ids).await.map_err(category_failure),
            CategoryTarget::Reminder => {
                let patch = ReminderPatch {
                    category_ids: Some(ids.clone()),
                    ..Default::default()
                };
                self.reminders(t)
                    .update(p, id, patch)
                    .await
                    .map(|r| r.category_ids)
                    .map_err(reminder_failure)
            }
        };
        let after = match saved {
            Ok(a) => a,
            Err(r) => return r,
        };
        let mut message = match (add.is_empty(), remove.is_empty()) {
            (false, true) => format!("Added {} to “{title}”.", names(add)),
            (true, false) => format!("Removed {} from “{title}”.", names(remove)),
            _ => format!("Updated the categories on “{title}”."),
        };
        if !created.is_empty() {
            message.push_str(&format!(" New: {}.", names(&created)));
        }
        ActionResult {
            ok: true,
            message,
            entity: (target == CategoryTarget::Task).then(|| EntityRef {
                kind: EntityKind::Task,
                id: id.to_owned(),
            }),
            undo: Some(undo::UndoOp::RestoreCategories {
                target,
                id: id.to_owned(),
                title: title.to_owned(),
                before,
                after,
            }),
            ..Default::default()
        }
    }

    /// Reverse a reminder or category step, unless it changed since.
    pub(super) async fn revert_reminder<C: CommandTargets>(
        &self,
        p: &Principal,
        op: &undo::UndoOp,
        t: &C,
    ) -> ActionResult {
        use undo::UndoOp;
        let service = self.reminders(t);
        let ok = |message: String| ActionResult {
            ok: true,
            message,
            ..Default::default()
        };
        let changed = |title: &str| {
            failure(format!(
                "“{title}” changed after that, so it was left as it is."
            ))
        };
        match op {
            UndoOp::DeleteReminder {
                id,
                title,
                updated_at,
            } => match service.get(p, id).await {
                Err(ReminderError::NotFound) => ok(format!("“{title}” was already deleted.")),
                Err(e) => reminder_failure(e),
                Ok(r) if r.deleted_at.is_some() => ok(format!("“{title}” was already deleted.")),
                Ok(r) if r.updated_at != *updated_at => changed(title),
                Ok(_) => match service.delete(p, id).await {
                    Ok(()) => ok(format!("Deleted the reminder “{title}”.")),
                    Err(e) => reminder_failure(e),
                },
            },
            UndoOp::UncompleteReminder {
                id,
                title,
                occurrence,
                updated_at,
            } => match service.get(p, id).await {
                Err(e) => reminder_failure(e),
                Ok(r) if r.updated_at != *updated_at => changed(title),
                Ok(_) => match service.uncomplete(p, id, occurrence.as_deref()).await {
                    Ok(_) => ok(format!("“{title}” is open again.")),
                    Err(e) => reminder_failure(e),
                },
            },
            UndoOp::RestoreReminder {
                id,
                title,
                due_date,
                due_time,
                priority,
                flagged,
                list_id,
                updated_at,
            } => match service.get(p, id).await {
                Err(e) => reminder_failure(e),
                Ok(r) if r.updated_at != *updated_at => changed(title),
                Ok(r) => {
                    let patch = ReminderPatch {
                        base_version: Some(r.version),
                        due_date: Some(due_date.clone()),
                        due_time: Some(due_time.clone()),
                        priority: Some(*priority),
                        flagged: Some(*flagged),
                        list_id: Some(list_id.clone()),
                        ..Default::default()
                    };
                    match service.update(p, id, patch).await {
                        Ok(_) => ok(format!("Restored the reminder “{title}”.")),
                        Err(e) => reminder_failure(e),
                    }
                }
            },
            UndoOp::RestoreCategories {
                target,
                id,
                title,
                before,
                after,
            } => {
                let cats = CategoryService::new(t.categories(), self.entropy, self.now);
                let now: Result<Vec<String>, ActionResult> = match target {
                    CategoryTarget::Task => cats
                        .list(p)
                        .await
                        .map(|l| {
                            l.tasks
                                .into_iter()
                                .filter(|a| a.task_id == *id)
                                .map(|a| a.category_id)
                                .collect()
                        })
                        .map_err(category_failure),
                    CategoryTarget::Reminder => service
                        .get(p, id)
                        .await
                        .map(|r| r.category_ids)
                        .map_err(reminder_failure),
                };
                let same = |a: &[String], b: &[String]| {
                    a.len() == b.len() && a.iter().all(|x| b.contains(x))
                };
                match now {
                    Err(r) => r,
                    Ok(now) if !same(&now, after) => changed(title),
                    Ok(_) => {
                        // Categories deleted since are skipped, not recreated.
                        let have: Vec<String> = match cats.list(p).await {
                            Ok(l) => l.categories.into_iter().map(|c| c.id).collect(),
                            Err(e) => return category_failure(e),
                        };
                        let ids: Vec<String> = before
                            .iter()
                            .filter(|c| have.contains(c))
                            .cloned()
                            .collect();
                        let r = match target {
                            CategoryTarget::Task => cats
                                .set_task(p, id, &ids)
                                .await
                                .map(|_| ())
                                .map_err(category_failure),
                            CategoryTarget::Reminder => service
                                .update(
                                    p,
                                    id,
                                    ReminderPatch {
                                        category_ids: Some(ids),
                                        ..Default::default()
                                    },
                                )
                                .await
                                .map(|_| ())
                                .map_err(reminder_failure),
                        };
                        match r {
                            Ok(()) => ok(format!("Put back the categories on “{title}”.")),
                            Err(r) => r,
                        }
                    }
                }
            }
            _ => failure("Not a reminder change.".into()),
        }
    }
}

fn names(v: &[String]) -> String {
    v.iter()
        .map(|n| format!("“{n}”"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// "every day", "every 2 weeks on Mon, Wed", "every month".
pub fn describe_rule(r: &Rule) -> String {
    const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let unit = match r.freq {
        reminders::recurrence::Freq::Daily => "day",
        reminders::recurrence::Freq::Weekly => "week",
        reminders::recurrence::Freq::Monthly => "month",
        reminders::recurrence::Freq::Yearly => "year",
    };
    let mut s = if r.interval > 1 {
        format!("every {} {unit}s", r.interval)
    } else {
        format!("every {unit}")
    };
    if !r.weekdays.is_empty() {
        let days: Vec<&str> = r
            .weekdays
            .iter()
            .filter_map(|d| DAYS.get(*d as usize).copied())
            .collect();
        s.push_str(&format!(" on {}", days.join(", ")));
    }
    if let Some(u) = &r.until {
        s.push_str(&format!(" until {}", day_label(u)));
    }
    s
}
