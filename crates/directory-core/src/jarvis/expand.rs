//! Turning requests into concrete steps, and running the steps JARVIS 2.0
//! added (DECISIONS D-048).
//!
//! Scheduling requests ("find four hours for the essay", "rearrange my
//! afternoon", "undo that") become ordinary steps when the plan is made:
//! `CreateBlock`/`MoveBlock` with real times, or `Revert` steps read from the
//! execution records. The owner approves exactly those steps. Each one is
//! checked again when it runs: an item that changed since it was proposed,
//! or a new clash, stops the step instead of overwriting anything.

use super::*;
use crate::blocks::{BlockError, BlockPatch, BlockService, BlockStatus, NewBlock, TimeBlock};
use crate::calendar::{CalendarError, EventPatch, NewEvent, Repeat};
use crate::preferences::{self as prefs, PreferencesPatch, PreferencesService};
use schedule::{Busy, Keep, Rules};

/// Look-ahead for a task without a due date.
const DEFAULT_DAYS: i64 = 14;
/// Longest range "free time" answers for.
const MAX_FREE_DAYS: i64 = 31;

/// A task to find time for: an open task, or one an earlier step creates.
struct Target {
    id: String,
    title: String,
    estimate: Option<u32>,
    due: Option<String>,
}

fn invalid(field: &'static str) -> impl Fn(&'static str) -> JarvisError {
    move |why| JarvisError::Invalid(field, why)
}

fn checked_date(field: &'static str, d: &str) -> Result<String, JarvisError> {
    validate::date(d).map_err(invalid(field))
}

/// Validated `start..end` minutes for a session or timed event.
fn checked_times(start: &str, end: &str) -> Result<(u32, u32), JarvisError> {
    let s = validate::clock(start, false).map_err(invalid("start_time"))?;
    let e = validate::clock(end, true).map_err(invalid("end_time"))?;
    if e <= s {
        return Err(JarvisError::Invalid("end_time", "must be after start_time"));
    }
    Ok((s, e))
}

fn checked_keep(keep: &[Keep]) -> Result<(), JarvisError> {
    keep.iter()
        .try_for_each(|k| k.minutes().map(|_| ()).map_err(invalid("keep")))
}

/// Refuse times that are already over.
fn not_past(clock: &Clock, date: &str, start: u32) -> Result<(), JarvisError> {
    if date < clock.today.as_str() || (date == clock.today && start < clock.minutes) {
        return Err(JarvisError::Unresolved(format!(
            "{} {} has already passed.",
            day_label(date),
            schedule::clock(start)
        )));
    }
    Ok(())
}

/// Busy time as it will be once `earlier` steps run: sessions they move or
/// remove leave their old place, new and moved sessions take their new one.
fn apply_earlier(busy: &mut Vec<Busy>, earlier: &[PlannedAction]) {
    let gone: Vec<&str> = earlier
        .iter()
        .filter_map(|a| match &a.command {
            Command::MoveBlock { block_id, .. } | Command::DeleteBlock { block_id, .. } => {
                Some(block_id.as_str())
            }
            _ => None,
        })
        .collect();
    busy.retain(|b| !b.block_id.as_deref().is_some_and(|id| gone.contains(&id)));
    for a in earlier {
        let (id, title, date, start, end) = match &a.command {
            Command::CreateBlock {
                id,
                title,
                date,
                start_time,
                end_time,
                ..
            } => (id, title, date, start_time, end_time),
            Command::MoveBlock {
                block_id,
                title,
                date,
                start_time,
                end_time,
                ..
            } => (block_id, title, date, start_time, end_time),
            _ => continue,
        };
        if let Ok((start, end)) = checked_times(start, end) {
            busy.push(Busy {
                date: date.clone(),
                start,
                end,
                title: title.clone(),
                fixed: false,
                block_id: Some(id.clone()),
            });
        }
    }
}

/// Titles of what `start..end` on `date` overlaps, as "“Class” 10:00–11:00".
fn overlap_titles(busy: &[Busy], date: &str, start: u32, end: u32, ignore: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = schedule::overlaps(busy, date, start, end, ignore)
        .into_iter()
        .map(|b| b.title.clone())
        .collect();
    out.dedup();
    out
}

fn overlap_note(titles: &[String]) -> Option<String> {
    match titles {
        [] => None,
        [one] => Some(format!("Overlaps “{one}”.")),
        many => Some(format!(
            "Overlaps {}.",
            many.iter()
                .map(|t| format!("“{t}”"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Minutes of non-skipped sessions already planned for `task_id`.
fn allocated(blocks: &[TimeBlock], task_id: &str) -> u32 {
    blocks
        .iter()
        .filter(|b| b.task_id.as_deref() == Some(task_id) && b.status != BlockStatus::Skipped)
        .map(TimeBlock::minutes)
        .sum()
}

/// Every step's dependencies on earlier steps: the task, project, parent, or
/// session an earlier step creates.
pub(super) fn link_steps(actions: &mut [PlannedAction]) {
    for i in 0..actions.len() {
        let (before, rest) = actions.split_at_mut(i);
        let action = &mut rest[0];
        let needs: Vec<&str> = match &action.command {
            Command::CreateBlock {
                task_id: Some(t), ..
            } => vec![t],
            Command::AddSubtask { parent_id, .. } => vec![parent_id],
            Command::MoveBlock { block_id, .. } | Command::DeleteBlock { block_id, .. } => {
                vec![block_id]
            }
            Command::CreateTask {
                project: Some(name),
                project_id: None,
                ..
            } => vec![name],
            _ => vec![],
        };
        let mut after: Vec<usize> = before
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                let made: Option<&str> = match &a.command {
                    Command::CreateTask { id, .. } => id.as_deref(),
                    Command::CreateBlock { id, .. }
                    | Command::CreateEvent { id, .. }
                    | Command::AddSubtask { id, .. } => Some(id),
                    Command::CreateProject { name } => Some(name.trim()),
                    _ => None,
                };
                made.is_some_and(|m| needs.iter().any(|n| n.eq_ignore_ascii_case(m)))
            })
            .map(|(j, _)| j)
            .collect();
        after.dedup();
        action.after = after;
    }
}

/// Steps left out at approval must exist, leave something to run, and not be
/// needed by a step that stays.
pub(super) fn check_skips(actions: &[PlannedAction], skip: &[usize]) -> Result<(), JarvisError> {
    if skip.iter().any(|i| *i >= actions.len()) {
        return Err(JarvisError::Invalid("skip", "no such step"));
    }
    if (0..actions.len()).all(|i| skip.contains(&i)) {
        return Err(JarvisError::Invalid(
            "skip",
            "every step is left out; cancel the plan instead",
        ));
    }
    let kept_needs_skipped = actions
        .iter()
        .enumerate()
        .filter(|(i, _)| !skip.contains(i))
        .any(|(_, a)| a.after.iter().any(|d| skip.contains(d)));
    if kept_needs_skipped {
        return Err(JarvisError::Invalid(
            "skip",
            "a step you kept needs a step you left out",
        ));
    }
    Ok(())
}

/// Sessions a plan creates, moves, or removes: they don't count as new
/// clashes for each other when the plan runs.
pub(super) fn touched_blocks(actions: &[PlannedAction]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match &a.command {
            Command::CreateBlock { id, .. } => Some(id.clone()),
            Command::MoveBlock { block_id, .. } | Command::DeleteBlock { block_id, .. } => {
                Some(block_id.clone())
            }
            _ => None,
        })
        .collect()
}

fn block_failure(e: BlockError) -> ActionResult {
    failure(match e {
        BlockError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        BlockError::NotFound => "That session no longer exists.".into(),
        BlockError::Exists => "That session already exists.".into(),
        BlockError::Backend(_) => "The planner is unavailable.".into(),
    })
}

fn event_failure(e: CalendarError) -> ActionResult {
    failure(match e {
        CalendarError::Invalid(f, w) => format!("Invalid {f}: {w}."),
        CalendarError::NotFound => "That event no longer exists.".into(),
        CalendarError::Exists => "That event already exists.".into(),
        CalendarError::Backend(_) => "The calendar is unavailable.".into(),
    })
}

fn ok(message: String) -> ActionResult {
    ActionResult {
        ok: true,
        message,
        ..Default::default()
    }
}

fn task_ref(id: &str) -> Option<EntityRef> {
    Some(EntityRef {
        kind: EntityKind::Task,
        id: id.to_owned(),
    })
}

fn item(kind: ItemKind, id: &str, date: &str) -> Option<Item> {
    Some(Item {
        kind,
        id: id.to_owned(),
        date: date.to_owned(),
    })
}

fn changed_since(title: &str) -> ActionResult {
    failure(format!(
        "“{title}” changed after this was proposed, so it was left as it is."
    ))
}

impl<'a, R: JarvisRepository, E: Entropy + ?Sized> JarvisService<'a, R, E> {
    async fn rules<C: CommandTargets>(&self, p: &Principal, t: &C) -> Result<Rules, JarvisError> {
        let prefs = PreferencesService::new(t.preferences(), self.now)
            .get(p)
            .await
            .map_err(|_| JarvisError::Unresolved("Couldn't read your preferences.".into()))?;
        Ok(Rules::from_prefs(&prefs))
    }

    /// Busy time from `from` to `to`, and every session (for allocations).
    async fn busy_between<C: CommandTargets>(
        &self,
        p: &Principal,
        t: &C,
        from: &str,
        to: &str,
    ) -> Result<(Vec<Busy>, Vec<TimeBlock>), JarvisError> {
        let occurrences = CalendarService::new(t.events(), self.entropy, self.now)
            .agenda(p, from, to)
            .await
            .map_err(|_| JarvisError::Unresolved("Couldn't read your calendar.".into()))?;
        let blocks = BlockService::new(t.blocks(), self.entropy, self.now)
            .list(p)
            .await
            .map_err(|_| JarvisError::Unresolved("Couldn't read your planner.".into()))?;
        Ok((schedule::busy(&occurrences, &blocks, from, to), blocks))
    }

    /// The task a scheduling request means: by id (checked to be the
    /// caller's), a task an earlier step creates, or an open task by name.
    async fn schedule_target<C: CommandTargets>(
        &self,
        p: &Principal,
        query: &str,
        task_id: Option<&str>,
        earlier: &[PlannedAction],
        t: &C,
    ) -> Result<Target, JarvisError> {
        let created: Vec<Target> = earlier
            .iter()
            .filter_map(|a| match &a.command {
                Command::CreateTask {
                    id: Some(id),
                    title,
                    due_date,
                    estimate_minutes,
                    ..
                } => Some(Target {
                    id: id.clone(),
                    title: title.clone(),
                    estimate: *estimate_minutes,
                    due: due_date.clone(),
                }),
                _ => None,
            })
            .collect();
        let query = query.trim();
        let found = match task_id {
            Some(id) => created.into_iter().find(|c| c.id == id),
            None if query.is_empty() && created.len() == 1 => created.into_iter().next(),
            None => created
                .into_iter()
                .find(|c| c.title.eq_ignore_ascii_case(query)),
        };
        if let Some(found) = found {
            return Ok(found);
        }
        let task = match task_id {
            Some(id) => {
                let task = TaskService::new(t.tasks(), self.entropy, self.now)
                    .get(p, id)
                    .await
                    .map_err(|_| JarvisError::Unresolved("That task no longer exists.".into()))?;
                if !task.status.is_open() {
                    return Err(JarvisError::Unresolved(format!(
                        "“{}” is already {}.",
                        task.title,
                        task.status.as_str()
                    )));
                }
                task
            }
            None if query.is_empty() => {
                return Err(JarvisError::Unresolved(
                    "Which task should I find time for?".into(),
                ))
            }
            None => self.resolve_open_task(p, query, t).await?,
        };
        Ok(Target {
            id: task.id,
            title: task.title,
            estimate: task.estimate_minutes,
            due: task.due_date,
        })
    }

    /// Minutes to find for `target`, and what was assumed. `None` when it
    /// already has the time it needs (with the reason).
    fn need_for(
        rules: &Rules,
        target: &Target,
        minutes: Option<u32>,
        blocks: &[TimeBlock],
    ) -> Result<(u32, Vec<String>), String> {
        if let Some(m) = minutes {
            return Ok((m, vec![]));
        }
        let have = allocated(blocks, &target.id);
        match target.estimate {
            Some(e) if have >= e => Err(format!(
                "“{}” already has {} scheduled, which covers its {} estimate.",
                target.title,
                planner::duration(have),
                planner::duration(e)
            )),
            Some(e) if have > 0 => Ok((
                e - have,
                vec![format!(
                    "{} of its {} estimate is already scheduled.",
                    planner::duration(have),
                    planner::duration(e)
                )],
            )),
            Some(e) => Ok((e, vec![])),
            None if have > 0 => Err(format!(
                "“{}” already has {} scheduled and no estimate; say how much more it needs.",
                target.title,
                planner::duration(have)
            )),
            None => Ok((
                rules.default_task,
                vec![format!(
                    "No estimate, so I assumed {}.",
                    planner::duration(rules.default_task)
                )],
            )),
        }
    }

    /// `CreateBlock` steps for the sessions found, with notes on the first.
    fn session_steps(
        &self,
        target: &Target,
        found: &schedule::Found,
        need: u32,
        to: &str,
        mut notes: Vec<String>,
        busy: &mut Vec<Busy>,
    ) -> Vec<PlannedAction> {
        if found.short > 0 {
            notes.push(format!(
                "Only found {} of {} by {}.",
                planner::duration(need - found.short),
                planner::duration(need),
                day_label(to)
            ));
        }
        let mut steps = Vec::with_capacity(found.sessions.len());
        for s in &found.sessions {
            let id = new_id(self.entropy);
            busy.push(Busy {
                date: s.date.clone(),
                start: s.start,
                end: s.end,
                title: target.title.clone(),
                fixed: false,
                block_id: Some(id.clone()),
            });
            steps.push(PlannedAction::new(Command::CreateBlock {
                id,
                task_id: Some(target.id.clone()),
                title: target.title.clone(),
                date: s.date.clone(),
                start_time: schedule::clock(s.start),
                end_time: schedule::clock(s.end),
                overlaps: vec![],
            }));
        }
        if let Some(first) = steps.first_mut() {
            first.notes = notes;
        }
        steps
    }

    /// Resolve one command into the steps the owner will approve.
    pub(super) async fn expand<C: CommandTargets>(
        &self,
        p: &Principal,
        command: Command,
        earlier: &[PlannedAction],
        clock: &Clock,
        t: &C,
    ) -> Result<Vec<PlannedAction>, JarvisError> {
        let one = |c: Command| Ok(vec![PlannedAction::new(c)]);
        match command {
            Command::CreateTask {
                id: Some(ref id), ..
            } if !is_id(id) => Err(JarvisError::Invalid("id", "expected 32 hex characters")),
            Command::CreateTask { .. } => {
                let mut command = self.resolve(p, command, earlier, t).await?;
                if let Command::CreateTask { id: id @ None, .. } = &mut command {
                    *id = Some(new_id(self.entropy));
                }
                one(command)
            }
            Command::ScheduleTask {
                query,
                task_id,
                minutes,
                from,
                by,
                session_minutes,
                keep,
            } => {
                checked_keep(&keep)?;
                if minutes.is_some_and(|m| m == 0 || m > tasks::MAX_ESTIMATE * 4) {
                    return Err(JarvisError::Invalid("minutes", "must be 1 to 5,760"));
                }
                if session_minutes.is_some_and(|m| !(schedule::MIN_SESSION..=480).contains(&m)) {
                    return Err(JarvisError::Invalid("session_minutes", "must be 15 to 480"));
                }
                let target = self
                    .schedule_target(p, &query, task_id.as_deref(), earlier, t)
                    .await?;
                let rules = self.rules(p, t).await?;
                let from = match from {
                    Some(d) => checked_date("from", &d)?,
                    None => clock.today.clone(),
                };
                let from = from.max(clock.today.clone());
                let by = match by.or_else(|| target.due.clone()) {
                    Some(d) => checked_date("by", &d)?,
                    None => validate::add_days(&from, DEFAULT_DAYS - 1),
                };
                if by < from {
                    return Err(JarvisError::Unresolved(format!(
                        "“{}” is due {}, which has passed. Say when to schedule it by.",
                        target.title,
                        day_label(&by)
                    )));
                }
                let to = by.min(validate::add_days(&from, schedule::MAX_DAYS - 1));
                let (mut busy, blocks) = self.busy_between(p, t, &from, &to).await?;
                apply_earlier(&mut busy, earlier);
                let (need, notes) = Self::need_for(&rules, &target, minutes, &blocks)
                    .map_err(JarvisError::Unresolved)?;
                let found = schedule::find_sessions(
                    &rules,
                    need,
                    session_minutes.unwrap_or(rules.session),
                    &from,
                    &to,
                    &clock.now(),
                    &busy,
                    &keep,
                );
                if found.sessions.is_empty() {
                    return Err(JarvisError::Unresolved(format!(
                        "I couldn't find free time for “{}” between {} and {} within your working hours ({}).",
                        target.title,
                        day_label(&from),
                        day_label(&to),
                        rules.hours_label()
                    )));
                }
                Ok(self.session_steps(&target, &found, need, &to, notes, &mut busy))
            }
            Command::ScheduleDue { by, keep } => {
                self.expand_due(p, &by, &keep, earlier, clock, t).await
            }
            Command::Rearrange {
                date,
                start,
                end,
                keep,
            } => {
                self.expand_rearrange(p, &date, start, end, &keep, earlier, clock, t)
                    .await
            }
            Command::Undo { plan_id } => self.expand_undo(p, plan_id.as_deref()).await,
            // Reversals only come from execution records, never from input.
            Command::Revert { .. } => Err(JarvisError::Invalid(
                "command",
                "undo steps come from your history",
            )),
            Command::CreateBlock {
                id,
                task_id,
                title,
                date,
                start_time,
                end_time,
                ..
            } => {
                let date = checked_date("date", &date)?;
                let (s, e) = checked_times(&start_time, &end_time)?;
                not_past(clock, &date, s)?;
                let id = match id {
                    id if id.is_empty() => new_id(self.entropy),
                    id if is_id(&id) => id,
                    _ => return Err(JarvisError::Invalid("id", "expected 32 hex characters")),
                };
                let mut title = validate::line(&title, crate::blocks::MAX_TITLE)
                    .map_err(invalid("title"))
                    .unwrap_or_default();
                if let Some(task) = &task_id {
                    let target = self.schedule_target(p, "", Some(task), earlier, t).await?;
                    if title.is_empty() {
                        title = target.title;
                    }
                } else if title.is_empty() {
                    return Err(JarvisError::Invalid("title", "required"));
                }
                let (mut busy, _) = self.busy_between(p, t, &date, &date).await?;
                apply_earlier(&mut busy, earlier);
                let overlaps = overlap_titles(&busy, &date, s, e, &[]);
                let mut action = PlannedAction::new(Command::CreateBlock {
                    id,
                    task_id,
                    title,
                    date,
                    start_time: schedule::clock(s),
                    end_time: schedule::clock(e),
                    overlaps: overlaps.clone(),
                });
                action.notes.extend(overlap_note(&overlaps));
                Ok(vec![action])
            }
            Command::MoveBlock {
                block_id,
                date,
                start_time,
                end_time,
                ..
            } => {
                let date = checked_date("date", &date)?;
                let (s, e) = checked_times(&start_time, &end_time)?;
                not_past(clock, &date, s)?;
                let block = BlockService::new(t.blocks(), self.entropy, self.now)
                    .get(p, &block_id)
                    .await
                    .map_err(|_| {
                        JarvisError::Unresolved("That session no longer exists.".into())
                    })?;
                let (mut busy, _) = self.busy_between(p, t, &date, &date).await?;
                apply_earlier(&mut busy, earlier);
                let overlaps = overlap_titles(&busy, &date, s, e, &[block.id.as_str()]);
                let mut action = PlannedAction::new(Command::MoveBlock {
                    block_id: block.id.clone(),
                    title: block.title.clone(),
                    date,
                    start_time: schedule::clock(s),
                    end_time: schedule::clock(e),
                    expect: Some(block.updated_at),
                    overlaps: overlaps.clone(),
                });
                if block.fixed {
                    action
                        .notes
                        .push(format!("“{}” is marked fixed.", block.title));
                }
                action.notes.extend(overlap_note(&overlaps));
                Ok(vec![action])
            }
            Command::DeleteBlock { block_id, .. } => {
                let block = BlockService::new(t.blocks(), self.entropy, self.now)
                    .get(p, &block_id)
                    .await
                    .map_err(|_| {
                        JarvisError::Unresolved("That session no longer exists.".into())
                    })?;
                let mut action = PlannedAction::new(Command::DeleteBlock {
                    block_id: block.id,
                    title: block.title,
                    expect: Some(block.updated_at),
                });
                action.summary = format!(
                    "{} ({} {}–{})",
                    action.summary,
                    day_label(&block.date),
                    block.start_time,
                    block.end_time
                );
                Ok(vec![action])
            }
            Command::CreateEvent {
                title,
                date,
                start_time,
                end_time,
                fixed,
                ..
            } => {
                let title = validate::line(&title, 200).map_err(invalid("title"))?;
                let date = checked_date("date", &date)?;
                let (start_time, end_time) = match (start_time, end_time) {
                    (Some(s), Some(e)) => {
                        let (s, e) = checked_times(&s, &e)?;
                        (Some(schedule::clock(s)), Some(schedule::clock(e)))
                    }
                    (Some(s), None) => {
                        let s = validate::clock(&s, false).map_err(invalid("start_time"))?;
                        (Some(schedule::clock(s)), None)
                    }
                    (None, Some(_)) => {
                        return Err(JarvisError::Invalid("end_time", "needs a start_time"))
                    }
                    (None, None) => (None, None),
                };
                if date < clock.today {
                    return Err(JarvisError::Unresolved(format!(
                        "{} has already passed.",
                        day_label(&date)
                    )));
                }
                let mut action = PlannedAction::new(Command::CreateEvent {
                    id: new_id(self.entropy),
                    title,
                    date: date.clone(),
                    start_time: start_time.clone(),
                    end_time: end_time.clone(),
                    fixed,
                });
                if let (Some(s), Some(e)) = (&start_time, &end_time) {
                    let (s, e) = checked_times(s, e)?;
                    let (mut busy, _) = self.busy_between(p, t, &date, &date).await?;
                    apply_earlier(&mut busy, earlier);
                    action
                        .notes
                        .extend(overlap_note(&overlap_titles(&busy, &date, s, e, &[])));
                }
                Ok(vec![action])
            }
            Command::MoveEvent {
                event_id,
                date,
                start_time,
                end_time,
                ..
            } => {
                let date = checked_date("date", &date)?;
                let event = CalendarService::new(t.events(), self.entropy, self.now)
                    .get(p, &event_id)
                    .await
                    .map_err(|_| JarvisError::Unresolved("That event no longer exists.".into()))?;
                if event.repeat != Repeat::None {
                    return Err(JarvisError::Unresolved(format!(
                        "“{}” repeats; change it in the Calendar.",
                        event.title
                    )));
                }
                // A new start keeps the length unless an end is given.
                let (start_time, end_time) = match (start_time, end_time) {
                    (Some(s), Some(e)) => {
                        let (s, e) = checked_times(&s, &e)?;
                        (Some(schedule::clock(s)), Some(schedule::clock(e)))
                    }
                    (Some(s), None) => {
                        let s = validate::clock(&s, false).map_err(invalid("start_time"))?;
                        let length = match (&event.start_time, &event.end_time) {
                            (Some(a), Some(b)) => checked_times(a, b).ok().map(|(a, b)| b - a),
                            _ => None,
                        };
                        match length {
                            Some(l) if s + l <= 24 * 60 => {
                                (Some(schedule::clock(s)), Some(schedule::clock(s + l)))
                            }
                            Some(_) => {
                                return Err(JarvisError::Invalid(
                                    "start_time",
                                    "the event would run past midnight",
                                ))
                            }
                            None => (Some(schedule::clock(s)), None),
                        }
                    }
                    (None, Some(_)) => {
                        return Err(JarvisError::Invalid("end_time", "needs a start_time"))
                    }
                    (None, None) => (None, None),
                };
                if date < clock.today {
                    return Err(JarvisError::Unresolved(format!(
                        "{} has already passed.",
                        day_label(&date)
                    )));
                }
                Ok(vec![PlannedAction::new(Command::MoveEvent {
                    event_id: event.id,
                    title: event.title,
                    date,
                    start_time,
                    end_time,
                    expect: Some(event.updated_at),
                })])
            }
            Command::AddSubtask {
                parent_id, title, ..
            } => {
                let title = validate::line(&title, tasks::MAX_TITLE).map_err(invalid("title"))?;
                let created = earlier.iter().find_map(|a| match &a.command {
                    Command::CreateTask {
                        id: Some(id),
                        title,
                        ..
                    } if *id == parent_id => Some(title.clone()),
                    _ => None,
                });
                let parent_title = match created {
                    Some(t) => t,
                    None => {
                        let parent = TaskService::new(t.tasks(), self.entropy, self.now)
                            .get(p, &parent_id)
                            .await
                            .map_err(|_| {
                                JarvisError::Unresolved("That task no longer exists.".into())
                            })?;
                        if parent.parent_id.is_some() {
                            return Err(JarvisError::Unresolved(format!(
                                "“{}” is a subtask; subtasks can't have their own.",
                                parent.title
                            )));
                        }
                        if !parent.status.is_open() {
                            return Err(JarvisError::Unresolved(format!(
                                "“{}” is already {}.",
                                parent.title,
                                parent.status.as_str()
                            )));
                        }
                        parent.title
                    }
                };
                one(Command::AddSubtask {
                    id: new_id(self.entropy),
                    parent_id,
                    parent_title,
                    title,
                })
            }
            Command::Remember { prefs: wanted } => {
                let current = PreferencesService::new(t.preferences(), self.now)
                    .get(p)
                    .await
                    .map_err(|_| JarvisError::Unresolved("Couldn't read your preferences.".into()))?
                    .jarvis
                    .unwrap_or_default();
                // Only what was asked for changes; the rest stays.
                let merged = crate::preferences::JarvisPrefs {
                    avoid_after: wanted.avoid_after.or(current.avoid_after),
                    break_minutes: if wanted.break_minutes > 0 {
                        wanted.break_minutes
                    } else {
                        current.break_minutes
                    },
                    default_task_minutes: wanted
                        .default_task_minutes
                        .or(current.default_task_minutes),
                    max_daily_minutes: wanted.max_daily_minutes.or(current.max_daily_minutes),
                    earliest_first: wanted.earliest_first || current.earliest_first,
                };
                let merged = prefs::check_jarvis(merged).map_err(|e| match e {
                    prefs::PreferencesError::Invalid(f, w) => JarvisError::Invalid(f, w),
                    prefs::PreferencesError::Backend(b) => JarvisError::Backend(b),
                })?;
                one(Command::Remember { prefs: merged })
            }
            Command::Unscheduled { from, to } => {
                let (from, to) = (checked_date("from", &from)?, checked_date("to", &to)?);
                if to < from {
                    return Err(JarvisError::Invalid("to", "must not be before from"));
                }
                one(Command::Unscheduled { from, to })
            }
            Command::FreeTime { from, to, .. } => {
                let (from, to) = (checked_date("from", &from)?, checked_date("to", &to)?);
                let days = validate::days_from_civil(&to) - validate::days_from_civil(&from);
                if !(0..MAX_FREE_DAYS).contains(&days) {
                    return Err(JarvisError::Invalid("to", "range must be 1 to 31 days"));
                }
                let now = (from <= clock.today).then_some(clock.minutes);
                let from = from.max(clock.today.clone());
                if from > to {
                    return Err(JarvisError::Unresolved(format!(
                        "{} has already passed.",
                        day_label(&to)
                    )));
                }
                one(Command::FreeTime { from, to, now })
            }
            Command::Missed { date } => one(Command::Missed {
                date: checked_date("date", &date)?,
            }),
            other => one(self.resolve(p, other, earlier, t).await?),
        }
    }

    /// Sessions for every open task due by `by` that still needs time,
    /// soonest due first, sharing one view of busy time.
    async fn expand_due<C: CommandTargets>(
        &self,
        p: &Principal,
        by: &str,
        keep: &[Keep],
        earlier: &[PlannedAction],
        clock: &Clock,
        t: &C,
    ) -> Result<Vec<PlannedAction>, JarvisError> {
        checked_keep(keep)?;
        let by = checked_date("by", by)?;
        if by < clock.today {
            return Err(JarvisError::Unresolved(format!(
                "{} has already passed.",
                day_label(&by)
            )));
        }
        let rules = self.rules(p, t).await?;
        let from = clock.today.clone();
        let to = by
            .clone()
            .min(validate::add_days(&from, schedule::MAX_DAYS - 1));
        let open = TaskService::new(t.tasks(), self.entropy, self.now)
            .list(p, Filter::Open)
            .await
            .map_err(|_| JarvisError::Unresolved("Couldn't read your tasks.".into()))?;
        let mut due: Vec<&tasks::Task> = open
            .iter()
            .filter(|x| x.parent_id.is_none() && x.due_date.as_ref().is_some_and(|d| *d <= by))
            .collect();
        due.sort_by(|a, b| {
            a.due_date
                .cmp(&b.due_date)
                .then(b.priority.cmp(&a.priority))
        });
        let (mut busy, blocks) = self.busy_between(p, t, &from, &to).await?;
        apply_earlier(&mut busy, earlier);
        let room = MAX_PLAN_STEPS.saturating_sub(earlier.len());
        let mut steps: Vec<PlannedAction> = Vec::new();
        let mut needing = 0;
        let mut left_out: Vec<String> = Vec::new();
        let mut no_room: Vec<String> = Vec::new();
        for task in due {
            let target = Target {
                id: task.id.clone(),
                title: task.title.clone(),
                estimate: task.estimate_minutes,
                due: task.due_date.clone(),
            };
            let Ok((need, mut notes)) = Self::need_for(&rules, &target, None, &blocks) else {
                continue;
            };
            needing += 1;
            let due_date = target.due.clone().unwrap_or_else(|| by.clone());
            // Overdue work goes in as soon as possible, within the range.
            let until = if due_date < from {
                to.clone()
            } else {
                due_date.clone().min(to.clone())
            };
            if due_date < from {
                notes.push(format!("Overdue since {}.", day_label(&due_date)));
            }
            let found = schedule::find_sessions(
                &rules,
                need,
                rules.session,
                &from,
                &until,
                &clock.now(),
                &busy,
                keep,
            );
            if found.sessions.is_empty() {
                left_out.push(target.title.clone());
                continue;
            }
            if steps.len() + found.sessions.len() > room {
                no_room.push(target.title.clone());
                continue;
            }
            steps.extend(self.session_steps(&target, &found, need, &until, notes, &mut busy));
        }
        if needing == 0 {
            return Err(JarvisError::Unresolved(format!(
                "Nothing open that's due by {} needs more time.",
                day_label(&by)
            )));
        }
        let Some(last) = steps.last_mut() else {
            return Err(JarvisError::Unresolved(format!(
                "I couldn't find free time before {} within your working hours ({}).",
                day_label(&by),
                rules.hours_label()
            )));
        };
        if !left_out.is_empty() {
            last.notes
                .push(format!("No free time found for: {}.", left_out.join(", ")));
        }
        if !no_room.is_empty() {
            last.notes.push(format!(
                "Too many sessions for one plan; ask again afterwards for: {}.",
                no_room.join(", ")
            ));
        }
        Ok(steps)
    }

    /// Move flexible sessions on `date` that clash (with commitments, each
    /// other, kept-free time, or the end of the day) into free time. Sessions
    /// that don't clash stay put; fixed items never move.
    #[allow(clippy::too_many_arguments)]
    async fn expand_rearrange<C: CommandTargets>(
        &self,
        p: &Principal,
        date: &str,
        start: Option<String>,
        end: Option<String>,
        keep: &[Keep],
        earlier: &[PlannedAction],
        clock: &Clock,
        t: &C,
    ) -> Result<Vec<PlannedAction>, JarvisError> {
        checked_keep(keep)?;
        let date = checked_date("date", date)?;
        if date < clock.today {
            return Err(JarvisError::Unresolved(format!(
                "{} has already passed.",
                day_label(&date)
            )));
        }
        let rules = self.rules(p, t).await?;
        let range = (
            match &start {
                Some(s) => validate::clock(s, false).map_err(invalid("start"))?,
                None => rules.day_start,
            },
            match &end {
                Some(e) => validate::clock(e, true).map_err(invalid("end"))?,
                None => rules.day_end,
            },
        );
        if range.1 <= range.0 {
            return Err(JarvisError::Invalid("end", "must be after start"));
        }
        let (mut busy, blocks) = self.busy_between(p, t, &date, &date).await?;
        apply_earlier(&mut busy, earlier);
        let touched = touched_blocks(earlier);
        let now = if date == clock.today {
            clock.minutes
        } else {
            0
        };
        let movable: Vec<&TimeBlock> = blocks
            .iter()
            .filter(|b| {
                b.date == date
                    && !b.fixed
                    && b.status == BlockStatus::Planned
                    && !touched.contains(&b.id)
                    && checked_times(&b.start_time, &b.end_time)
                        .is_ok_and(|(s, e)| s >= range.0.max(now) && s < range.1 && e > s)
            })
            .collect();
        if movable.is_empty() {
            return Err(JarvisError::Unresolved(format!(
                "There are no flexible sessions to rearrange on {} between {} and {}.",
                day_label(&date),
                schedule::clock(range.0),
                schedule::clock(range.1)
            )));
        }
        let ids: Vec<&str> = movable.iter().map(|b| b.id.as_str()).collect();
        let others: Vec<Busy> = busy
            .iter()
            .filter(|b| !b.block_id.as_deref().is_some_and(|id| ids.contains(&id)))
            .cloned()
            .collect();
        let kept: Vec<(u32, u32)> = keep
            .iter()
            .filter(|k| k.date.as_deref().is_none_or(|d| d == date))
            .filter_map(|k| k.minutes().ok())
            .collect();
        // Sessions that fit where they are stay, and count as busy.
        let mut staying = others.clone();
        let mut clashing: Vec<schedule::Movable> = Vec::new();
        let mut sorted = movable.clone();
        sorted.sort_by(|a, b| a.start_time.cmp(&b.start_time));
        for b in sorted {
            let Ok((s, e)) = checked_times(&b.start_time, &b.end_time) else {
                continue;
            };
            let clash = e > range.1
                || e > rules.day_end
                || !schedule::overlaps(&staying, &date, s, e, &[]).is_empty()
                || kept.iter().any(|(ks, ke)| *ks < e && s < *ke);
            if clash {
                clashing.push(schedule::Movable {
                    id: b.id.clone(),
                    start: s,
                    end: e,
                });
            } else {
                staying.push(Busy {
                    date: date.clone(),
                    start: s,
                    end: e,
                    title: b.title.clone(),
                    fixed: false,
                    block_id: Some(b.id.clone()),
                });
            }
        }
        if clashing.is_empty() {
            return Err(JarvisError::Unresolved(format!(
                "Your flexible sessions on {} don't clash with anything, so nothing needs to move.",
                day_label(&date)
            )));
        }
        let (moves, unplaced) = schedule::rearrange(
            &rules,
            &date,
            range,
            &clock.now(),
            &staying,
            &clashing,
            keep,
        );
        let title_of = |id: &str| {
            blocks
                .iter()
                .find(|b| b.id == id)
                .map(|b| b.title.clone())
                .unwrap_or_default()
        };
        let mut steps: Vec<PlannedAction> = moves
            .iter()
            .filter_map(|(id, s, e)| {
                let b = blocks.iter().find(|b| &b.id == id)?;
                Some(PlannedAction::new(Command::MoveBlock {
                    block_id: id.clone(),
                    title: b.title.clone(),
                    date: date.clone(),
                    start_time: schedule::clock(*s),
                    end_time: schedule::clock(*e),
                    expect: Some(b.updated_at),
                    overlaps: vec![],
                }))
            })
            .collect();
        let stuck: Vec<String> = unplaced
            .iter()
            .map(|id| format!("“{}”", title_of(id)))
            .collect();
        match steps.first_mut() {
            Some(first) if !stuck.is_empty() => first.notes.push(format!(
                "No room left for {}; left where it is.",
                stuck.join(", ")
            )),
            Some(_) => {}
            None => {
                return Err(JarvisError::Unresolved(format!(
                    "There's no free time on {} to move {} to.",
                    day_label(&date),
                    stuck.join(", ")
                )))
            }
        }
        Ok(steps)
    }

    /// `Revert` steps for the latest change (or `plan_id`), newest first.
    async fn expand_undo(
        &self,
        p: &Principal,
        plan_id: Option<&str>,
    ) -> Result<Vec<PlannedAction>, JarvisError> {
        let is_undo = |pl: &Plan| {
            pl.actions
                .iter()
                .any(|a| matches!(a.command, Command::Revert { .. }))
        };
        let plan = match plan_id {
            Some(id) => self.load(p, id).await?,
            None => self
                .repo
                .history(&p.user_id, HISTORY_LIMIT)
                .await
                .map_err(backend)?
                .into_iter()
                .find(|pl| {
                    pl.requires_approval
                        && matches!(pl.status, PlanStatus::Executed | PlanStatus::Failed)
                        && pl.reversed_by.is_none()
                        && !is_undo(pl)
                })
                .ok_or_else(|| JarvisError::Unresolved("There's no change to undo.".into()))?,
        };
        if plan.reversed_by.is_some() {
            return Err(JarvisError::Unresolved(format!(
                "“{}” was already undone.",
                plan.input
            )));
        }
        if !matches!(plan.status, PlanStatus::Executed | PlanStatus::Failed) || is_undo(&plan) {
            return Err(JarvisError::Unresolved(
                "Only changes that ran can be undone.".into(),
            ));
        }
        let mut steps: Vec<PlannedAction> = plan
            .results
            .iter()
            .rev()
            .filter_map(|r| r.undo.clone())
            .map(|op| {
                PlannedAction::new(Command::Revert {
                    plan_id: plan.id.clone(),
                    op,
                })
            })
            .collect();
        let kept: Vec<String> = plan
            .actions
            .iter()
            .zip(&plan.results)
            .filter(|(a, r)| r.ok && !r.skipped && r.undo.is_none() && a.risk > Risk::Read)
            .map(|(a, _)| a.summary.clone())
            .collect();
        let Some(first) = steps.first_mut() else {
            return Err(JarvisError::Unresolved(format!(
                "The last change (“{}”) can't be undone automatically.",
                plan.input
            )));
        };
        first.notes.push(format!("Undoes “{}”.", plan.input));
        if !kept.is_empty() {
            first.notes.push(format!(
                "Can't be undone automatically: {}.",
                kept.join("; ")
            ));
        }
        if steps.len() > MAX_PLAN_STEPS {
            steps.truncate(MAX_PLAN_STEPS);
        }
        Ok(steps)
    }

    /// Run one approved step.
    pub(super) async fn run_step<C: CommandTargets>(
        &self,
        p: &Principal,
        command: &Command,
        touched: &[&str],
        t: &C,
    ) -> ActionResult {
        let blocks = BlockService::new(t.blocks(), self.entropy, self.now);
        let calendar = CalendarService::new(t.events(), self.entropy, self.now);
        match command {
            Command::CreateBlock {
                id,
                task_id,
                title,
                date,
                start_time,
                end_time,
                overlaps,
            } => {
                if let Some(r) = self
                    .new_clash(p, t, date, start_time, end_time, overlaps, touched)
                    .await
                {
                    return r;
                }
                let input = NewBlock {
                    id: Some(id.clone()),
                    task_id: task_id.clone(),
                    title: Some(title.clone()),
                    date: date.clone(),
                    start_time: start_time.clone(),
                    end_time: end_time.clone(),
                    fixed: Some(false),
                    ..Default::default()
                };
                match blocks.create(p, input).await {
                    Ok(b) => ActionResult {
                        ok: true,
                        message: format!(
                            "Scheduled “{}” {} {}–{}.",
                            b.title,
                            day_label(&b.date),
                            b.start_time,
                            b.end_time
                        ),
                        entity: b.task_id.as_deref().and_then(task_ref),
                        item: item(ItemKind::Block, &b.id, &b.date),
                        undo: Some(undo::UndoOp::DeleteBlock {
                            id: b.id.clone(),
                            title: b.title.clone(),
                            updated_at: b.updated_at,
                        }),
                        ..Default::default()
                    },
                    Err(BlockError::Exists) => ActionResult {
                        item: item(ItemKind::Block, id, date),
                        ..ok(format!("“{title}” was already scheduled."))
                    },
                    Err(e) => block_failure(e),
                }
            }
            Command::MoveBlock {
                block_id,
                title,
                date,
                start_time,
                end_time,
                expect,
                overlaps,
            } => {
                let before = match blocks.get(p, block_id).await {
                    Ok(b) => b,
                    Err(e) => return block_failure(e),
                };
                if expect.is_some_and(|x| x != before.updated_at) {
                    return changed_since(title);
                }
                if let Some(r) = self
                    .new_clash(p, t, date, start_time, end_time, overlaps, touched)
                    .await
                {
                    return r;
                }
                let patch = BlockPatch {
                    date: Some(date.clone()),
                    start_time: Some(start_time.clone()),
                    end_time: Some(end_time.clone()),
                    ..Default::default()
                };
                match blocks.update(p, block_id, patch).await {
                    Ok(b) => ActionResult {
                        ok: true,
                        message: format!(
                            "Moved “{}” to {} {}–{}.",
                            b.title,
                            day_label(&b.date),
                            b.start_time,
                            b.end_time
                        ),
                        entity: b.task_id.as_deref().and_then(task_ref),
                        item: item(ItemKind::Block, &b.id, &b.date),
                        undo: Some(undo::UndoOp::RestoreBlock {
                            block: before,
                            updated_at: Some(b.updated_at),
                        }),
                        ..Default::default()
                    },
                    Err(e) => block_failure(e),
                }
            }
            Command::DeleteBlock {
                block_id,
                title,
                expect,
            } => {
                let before = match blocks.get(p, block_id).await {
                    Ok(b) => b,
                    Err(BlockError::NotFound) => {
                        return ok(format!("“{title}” was already removed."))
                    }
                    Err(e) => return block_failure(e),
                };
                if expect.is_some_and(|x| x != before.updated_at) {
                    return changed_since(title);
                }
                match blocks.delete(p, block_id).await {
                    Ok(()) => ActionResult {
                        undo: Some(undo::UndoOp::RestoreBlock {
                            block: before,
                            updated_at: None,
                        }),
                        ..ok(format!("Removed the session “{title}”."))
                    },
                    Err(BlockError::NotFound) => ok(format!("“{title}” was already removed.")),
                    Err(e) => block_failure(e),
                }
            }
            Command::CreateEvent {
                id,
                title,
                date,
                start_time,
                end_time,
                fixed,
            } => {
                let input = NewEvent {
                    id: Some(id.clone()),
                    title: title.clone(),
                    start_date: date.clone(),
                    start_time: start_time.clone(),
                    end_time: end_time.clone(),
                    fixed: Some(*fixed),
                    ..Default::default()
                };
                match calendar.create(p, input).await {
                    Ok(e) => ActionResult {
                        item: item(ItemKind::Event, &e.id, &e.start_date),
                        undo: Some(undo::UndoOp::DeleteEvent {
                            id: e.id.clone(),
                            title: e.title.clone(),
                            updated_at: e.updated_at,
                        }),
                        ..ok(format!(
                            "Added “{}” on {}{}.",
                            e.title,
                            day_label(&e.start_date),
                            times_label(e.start_time.as_deref(), e.end_time.as_deref())
                        ))
                    },
                    Err(CalendarError::Exists) => ActionResult {
                        item: item(ItemKind::Event, id, date),
                        ..ok(format!("“{title}” was already added."))
                    },
                    Err(e) => event_failure(e),
                }
            }
            Command::MoveEvent {
                event_id,
                title,
                date,
                start_time,
                end_time,
                expect,
            } => {
                let before = match calendar.get(p, event_id).await {
                    Ok(e) => e,
                    Err(e) => return event_failure(e),
                };
                if expect.is_some_and(|x| x != before.updated_at) {
                    return changed_since(title);
                }
                let patch = EventPatch {
                    start_date: Some(date.clone()),
                    start_time: start_time.clone().map(Some),
                    end_time: start_time.as_ref().map(|_| end_time.clone()),
                    ..Default::default()
                };
                match calendar.update(p, event_id, patch).await {
                    Ok(e) => ActionResult {
                        item: item(ItemKind::Event, &e.id, &e.start_date),
                        undo: Some(undo::UndoOp::RestoreEvent {
                            id: before.id.clone(),
                            title: before.title.clone(),
                            start_date: before.start_date.clone(),
                            end_date: before.end_date.clone(),
                            start_time: before.start_time.clone(),
                            end_time: before.end_time.clone(),
                            updated_at: e.updated_at,
                        }),
                        ..ok(format!(
                            "Moved “{}” to {}{}.",
                            e.title,
                            day_label(&e.start_date),
                            times_label(e.start_time.as_deref(), e.end_time.as_deref())
                        ))
                    },
                    Err(e) => event_failure(e),
                }
            }
            Command::AddSubtask {
                id,
                parent_id,
                title,
                ..
            } => {
                let input = NewTask {
                    id: Some(id.clone()),
                    title: title.clone(),
                    parent_id: Some(parent_id.clone()),
                    ..Default::default()
                };
                match TaskService::new(t.tasks(), self.entropy, self.now)
                    .create(p, input)
                    .await
                {
                    Ok(task) => ActionResult {
                        entity: task_ref(&task.id),
                        undo: Some(undo::UndoOp::DeleteTask {
                            id: task.id.clone(),
                            title: task.title.clone(),
                            updated_at: task.updated_at,
                        }),
                        ..ok(format!("Added subtask “{}”.", task.title))
                    },
                    Err(TaskError::Exists) => ActionResult {
                        entity: task_ref(id),
                        ..ok(format!("“{title}” was already added."))
                    },
                    Err(e) => task_failure(e),
                }
            }
            Command::Remember { prefs: wanted } => {
                let service = PreferencesService::new(t.preferences(), self.now);
                let before = match service.get(p).await {
                    Ok(x) => x.jarvis,
                    Err(_) => return failure("Preferences are unavailable.".into()),
                };
                let patch = PreferencesPatch {
                    jarvis: Some(Some(wanted.clone())),
                    ..Default::default()
                };
                match service.update(p, patch).await {
                    Ok(saved) => ActionResult {
                        undo: Some(undo::UndoOp::RestorePrefs {
                            before,
                            after: saved.jarvis.clone(),
                        }),
                        ..ok(format!(
                            "Saved: {}. Change it any time in Settings.",
                            undo::describe_prefs(wanted)
                        ))
                    },
                    Err(prefs::PreferencesError::Invalid(f, w)) => {
                        failure(format!("Invalid {f}: {w}."))
                    }
                    Err(_) => failure("Preferences are unavailable.".into()),
                }
            }
            Command::Revert { op, .. } => self.revert(p, op, t).await,
            Command::Unscheduled { from, to } => self.unscheduled(p, from, to, t).await,
            Command::FreeTime { from, to, now } => self.free_time(p, from, to, *now, t).await,
            Command::Missed { date } => self.missed(p, date, t).await,
            Command::ScheduleTask { .. }
            | Command::ScheduleDue { .. }
            | Command::Rearrange { .. }
            | Command::Undo { .. } => failure("This step wasn't prepared; ask again.".into()),
            other => self.run_one(p, other, t).await,
        }
    }

    /// A failure if `start..end` on `date` now overlaps something that it
    /// didn't when proposed (other than sessions this plan touches).
    #[allow(clippy::too_many_arguments)]
    async fn new_clash<C: CommandTargets>(
        &self,
        p: &Principal,
        t: &C,
        date: &str,
        start: &str,
        end: &str,
        accepted: &[String],
        touched: &[&str],
    ) -> Option<ActionResult> {
        let (s, e) = checked_times(start, end).ok()?;
        let busy = match self.busy_between(p, t, date, date).await {
            Ok((busy, _)) => busy,
            Err(_) => return Some(failure("The calendar is unavailable.".into())),
        };
        let fresh: Vec<String> = overlap_titles(&busy, date, s, e, touched)
            .into_iter()
            .filter(|x| !accepted.contains(x))
            .collect();
        (!fresh.is_empty()).then(|| {
            failure(format!(
                "Not done: {} {start}–{end} now overlaps {}.",
                day_label(date),
                fresh
                    .iter()
                    .map(|x| format!("“{x}”"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
    }

    /// Apply one recorded reversal, unless the item changed since.
    async fn revert<C: CommandTargets>(
        &self,
        p: &Principal,
        op: &undo::UndoOp,
        t: &C,
    ) -> ActionResult {
        use undo::UndoOp;
        let tasks = TaskService::new(t.tasks(), self.entropy, self.now);
        let blocks = BlockService::new(t.blocks(), self.entropy, self.now);
        let calendar = CalendarService::new(t.events(), self.entropy, self.now);
        match op {
            UndoOp::DeleteTask {
                id,
                title,
                updated_at,
            } => match tasks.get(p, id).await {
                Err(TaskError::NotFound) => ok(format!("“{title}” was already deleted.")),
                Err(e) => task_failure(e),
                Ok(x) if x.updated_at != *updated_at => changed_since(title),
                Ok(_) => match tasks.delete(p, id).await {
                    Ok(()) | Err(TaskError::NotFound) => ok(format!("Deleted “{title}”.")),
                    Err(e) => task_failure(e),
                },
            },
            UndoOp::RestoreTask {
                id,
                title,
                status,
                priority,
                due_date,
                estimate_minutes,
                updated_at,
            } => match tasks.get(p, id).await {
                Err(e) => task_failure(e),
                Ok(x) if x.updated_at != *updated_at => changed_since(title),
                Ok(_) => {
                    let patch = TaskPatch {
                        status: Some(*status),
                        priority: Some(*priority),
                        due_date: Some(due_date.clone()),
                        estimate_minutes: Some(*estimate_minutes),
                        ..Default::default()
                    };
                    match tasks.update(p, id, patch).await {
                        Ok(x) => ActionResult {
                            entity: task_ref(&x.id),
                            ..ok(format!("Restored “{}”.", x.title))
                        },
                        Err(e) => task_failure(e),
                    }
                }
            },
            UndoOp::DeleteBlock {
                id,
                title,
                updated_at,
            } => match blocks.get(p, id).await {
                Err(BlockError::NotFound) => ok(format!("“{title}” was already removed.")),
                Err(e) => block_failure(e),
                Ok(b) if b.updated_at != *updated_at => changed_since(title),
                Ok(_) => match blocks.delete(p, id).await {
                    Ok(()) | Err(BlockError::NotFound) => {
                        ok(format!("Removed the session “{title}”."))
                    }
                    Err(e) => block_failure(e),
                },
            },
            UndoOp::RestoreBlock {
                block,
                updated_at: Some(updated_at),
            } => match blocks.get(p, &block.id).await {
                Err(e) => block_failure(e),
                // An earlier, partly finished undo already moved it back.
                Ok(b)
                    if (&b.date, &b.start_time, &b.end_time)
                        == (&block.date, &block.start_time, &block.end_time) =>
                {
                    ok(format!("“{}” is already back in place.", block.title))
                }
                Ok(b) if b.updated_at != *updated_at => changed_since(&block.title),
                Ok(_) => {
                    let patch = BlockPatch {
                        date: Some(block.date.clone()),
                        start_time: Some(block.start_time.clone()),
                        end_time: Some(block.end_time.clone()),
                        ..Default::default()
                    };
                    match blocks.update(p, &block.id, patch).await {
                        Ok(b) => ActionResult {
                            item: item(ItemKind::Block, &b.id, &b.date),
                            ..ok(format!(
                                "Moved “{}” back to {} {}–{}.",
                                b.title,
                                day_label(&b.date),
                                b.start_time,
                                b.end_time
                            ))
                        },
                        Err(e) => block_failure(e),
                    }
                }
            },
            UndoOp::RestoreBlock {
                block,
                updated_at: None,
            } => {
                let input = NewBlock {
                    id: Some(block.id.clone()),
                    task_id: block.task_id.clone(),
                    title: Some(block.title.clone()),
                    date: block.date.clone(),
                    start_time: block.start_time.clone(),
                    end_time: block.end_time.clone(),
                    timezone: block.timezone.clone(),
                    fixed: Some(block.fixed),
                    notes: Some(block.notes.clone()),
                };
                match blocks.create(p, input).await {
                    Ok(_) | Err(BlockError::Exists) => ActionResult {
                        item: item(ItemKind::Block, &block.id, &block.date),
                        ..ok(format!("Put back the session “{}”.", block.title))
                    },
                    Err(e) => block_failure(e),
                }
            }
            UndoOp::DeleteEvent {
                id,
                title,
                updated_at,
            } => match calendar.get(p, id).await {
                Err(CalendarError::NotFound) => ok(format!("“{title}” was already deleted.")),
                Err(e) => event_failure(e),
                Ok(x) if x.updated_at != *updated_at => changed_since(title),
                Ok(_) => match calendar.delete(p, id).await {
                    Ok(()) | Err(CalendarError::NotFound) => ok(format!("Deleted “{title}”.")),
                    Err(e) => event_failure(e),
                },
            },
            UndoOp::RestoreEvent {
                id,
                title,
                start_date,
                end_date,
                start_time,
                end_time,
                updated_at,
            } => match calendar.get(p, id).await {
                Err(e) => event_failure(e),
                Ok(x) if x.updated_at != *updated_at => changed_since(title),
                Ok(_) => {
                    let patch = EventPatch {
                        start_date: Some(start_date.clone()),
                        end_date: Some(end_date.clone()),
                        start_time: Some(start_time.clone()),
                        end_time: Some(end_time.clone()),
                        ..Default::default()
                    };
                    match calendar.update(p, id, patch).await {
                        Ok(e) => ActionResult {
                            item: item(ItemKind::Event, &e.id, &e.start_date),
                            ..ok(format!(
                                "Moved “{}” back to {}.",
                                e.title,
                                day_label(&e.start_date)
                            ))
                        },
                        Err(e) => event_failure(e),
                    }
                }
            },
            UndoOp::RestorePrefs { before, after } => {
                let service = PreferencesService::new(t.preferences(), self.now);
                match service.get(p).await {
                    Err(_) => failure("Preferences are unavailable.".into()),
                    Ok(now) if now.jarvis != *after => failure(
                        "Your JARVIS preferences changed after that, so they were left as they are."
                            .into(),
                    ),
                    Ok(_) => {
                        let patch = PreferencesPatch {
                            jarvis: Some(before.clone()),
                            ..Default::default()
                        };
                        match service.update(p, patch).await {
                            Ok(_) => ok(match before {
                                Some(b) => format!(
                                    "Preferences restored: {}.",
                                    undo::describe_prefs(b)
                                ),
                                None => "JARVIS preferences are back to the defaults.".into(),
                            }),
                            Err(_) => failure("Preferences are unavailable.".into()),
                        }
                    }
                }
            }
        }
    }

    /// Open tasks due by `to` whose sessions don't cover their estimate (or
    /// that have no sessions and no estimate).
    async fn unscheduled<C: CommandTargets>(
        &self,
        p: &Principal,
        _from: &str,
        to: &str,
        t: &C,
    ) -> ActionResult {
        let open = match TaskService::new(t.tasks(), self.entropy, self.now)
            .list(p, Filter::Open)
            .await
        {
            Ok(x) => x,
            Err(e) => return task_failure(e),
        };
        let blocks = match BlockService::new(t.blocks(), self.entropy, self.now)
            .list(p)
            .await
        {
            Ok(b) => b,
            Err(e) => return block_failure(e),
        };
        let mut due: Vec<&tasks::Task> = open
            .iter()
            .filter(|x| x.parent_id.is_none() && x.due_date.as_deref().is_some_and(|d| d <= to))
            .collect();
        due.sort_by(|a, b| a.due_date.cmp(&b.due_date));
        let lines: Vec<(String, String)> = due
            .iter()
            .filter_map(|x| {
                let have = allocated(&blocks, &x.id);
                let due = day_label(x.due_date.as_deref().unwrap_or_default());
                let why = match x.estimate_minutes {
                    Some(e) if have >= e => return None,
                    Some(e) if have > 0 => format!(
                        "{} of {} still unscheduled",
                        planner::duration(e - have),
                        planner::duration(e)
                    ),
                    Some(e) => format!("{} estimate, nothing scheduled", planner::duration(e)),
                    None if have > 0 => return None,
                    None => "nothing scheduled".into(),
                };
                Some((x.id.clone(), format!("{} (due {due}, {why})", x.title)))
            })
            .collect();
        match lines.as_slice() {
            [] => ok(format!(
                "Everything open that's due by {} has time scheduled.",
                day_label(to)
            )),
            [(id, line)] => ActionResult {
                entity: task_ref(id),
                ..ok(format!(
                    "1 task due by {} still needs time: {line}.",
                    day_label(to)
                ))
            },
            many => ActionResult {
                entity: task_ref(&many[0].0),
                ..ok(format!(
                    "{} tasks due by {} still need time: {}.",
                    many.len(),
                    day_label(to),
                    many.iter()
                        .take(LIST_PREVIEW)
                        .map(|(_, l)| l.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                ))
            },
        }
    }

    async fn free_time<C: CommandTargets>(
        &self,
        p: &Principal,
        from: &str,
        to: &str,
        now: Option<u32>,
        t: &C,
    ) -> ActionResult {
        let rules = match self.rules(p, t).await {
            Ok(r) => r,
            Err(_) => return failure("Preferences are unavailable.".into()),
        };
        let busy = match self.busy_between(p, t, from, to).await {
            Ok((b, _)) => b,
            Err(_) => return failure("The calendar is unavailable.".into()),
        };
        let at = schedule::Now {
            date: from.to_owned(),
            minutes: now.unwrap_or(0),
        };
        let days = schedule::free_time(&rules, from, to, &at, &busy);
        if days.is_empty() {
            return ok(format!(
                "No working hours left between {} and {} ({}).",
                day_label(from),
                day_label(to),
                rules.hours_label()
            ));
        }
        let mut total = 0;
        let parts: Vec<String> = days
            .iter()
            .take(LIST_PREVIEW)
            .map(|(d, gaps)| {
                let minutes: u32 = gaps.iter().map(|(s, e)| e - s).sum();
                total += minutes;
                if gaps.is_empty() {
                    format!("{}: none", day_label(d))
                } else {
                    format!(
                        "{}: {} ({})",
                        day_label(d),
                        gaps.iter()
                            .map(|(s, e)| format!(
                                "{}–{}",
                                schedule::clock(*s),
                                schedule::clock(*e)
                            ))
                            .collect::<Vec<_>>()
                            .join(", "),
                        planner::duration(minutes)
                    )
                }
            })
            .collect();
        ok(format!(
            "Free within your hours ({}): {}. {} in all.",
            rules.hours_label(),
            parts.join(". "),
            planner::duration(total)
        ))
    }

    /// Open tasks due on or before `date`, and that day's sessions not done.
    async fn missed<C: CommandTargets>(&self, p: &Principal, date: &str, t: &C) -> ActionResult {
        let open = match TaskService::new(t.tasks(), self.entropy, self.now)
            .list(p, Filter::Open)
            .await
        {
            Ok(x) => x,
            Err(e) => return task_failure(e),
        };
        let blocks = match BlockService::new(t.blocks(), self.entropy, self.now)
            .list(p)
            .await
        {
            Ok(b) => b,
            Err(e) => return block_failure(e),
        };
        let mut late: Vec<&tasks::Task> = open
            .iter()
            .filter(|x| x.parent_id.is_none() && x.due_date.as_deref().is_some_and(|d| d <= date))
            .collect();
        late.sort_by(|a, b| a.due_date.cmp(&b.due_date));
        let open_ids: Vec<&str> = open.iter().map(|x| x.id.as_str()).collect();
        let sessions: Vec<String> = blocks
            .iter()
            .filter(|b| {
                b.date == date
                    && b.status == BlockStatus::Planned
                    && b.task_id.as_deref().is_none_or(|id| open_ids.contains(&id))
            })
            .map(|b| format!("{} {}–{}", b.title, b.start_time, b.end_time))
            .collect();
        let mut parts = Vec::new();
        if !late.is_empty() {
            parts.push(format!(
                "Still open: {}",
                late.iter()
                    .take(LIST_PREVIEW)
                    .map(|x| match x.due_date.as_deref() {
                        Some(d) if d == date => format!("{} (due that day)", x.title),
                        Some(d) => format!("{} (due {})", x.title, day_label(d)),
                        None => x.title.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !sessions.is_empty() {
            parts.push(format!("Sessions not marked done: {}", sessions.join("; ")));
        }
        if parts.is_empty() {
            return ok(format!("Nothing was left undone on {}.", day_label(date)));
        }
        ActionResult {
            entity: late.first().and_then(|x| task_ref(&x.id)),
            ..ok(format!("From {}: {}.", day_label(date), parts.join(". ")))
        }
    }
}
