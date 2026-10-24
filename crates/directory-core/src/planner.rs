//! Deterministic work planner (DECISIONS D-040).
//!
//! Scores open tasks by urgency, importance, momentum, and dependencies, and
//! proposes what to do today or within the time available. Every point comes
//! with a reason, so a proposal can be explained. It only proposes: nothing
//! here changes a task or a plan. AI may phrase the request (minutes, project),
//! but never does the arithmetic.

use crate::tasks::{Priority, Status, Task};
use crate::validate;
use serde::Serialize;

/// Assumed length of a task without an estimate.
pub const DEFAULT_ESTIMATE: u32 = 30;
/// Longest stretch one suggestion plans for.
pub const MAX_AVAILABLE: u32 = 16 * 60;
/// Default number of priorities.
pub const TOP: usize = 5;
pub const MAX_PICKS: usize = 20;
/// Undated tasks older than this get a small nudge.
const STALE_DAYS: i64 = 14;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Factor {
    pub points: i32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scored {
    pub task_id: String,
    pub title: String,
    pub score: i32,
    pub factors: Vec<Factor>,
    /// Minutes, the task's estimate or `DEFAULT_ESTIMATE`.
    pub minutes: u32,
    /// Whether `minutes` is the task's own estimate.
    pub estimated: bool,
    /// The due date that counted (the task's or its parent's).
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Constraints {
    /// Minutes available; `None` ranks without a time budget.
    pub minutes: Option<u32>,
    /// Only tasks in this project (subtasks follow their parent).
    pub project_id: Option<String>,
    pub max_items: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Proposal {
    pub date: String,
    pub minutes: Option<u32>,
    pub picks: Vec<Scored>,
    pub total_minutes: u32,
    /// Open tasks waiting on their own open subtasks (not offered directly).
    pub blocked: usize,
    /// Ranked tasks left out because they don't fit the time available.
    pub too_long: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    Invalid(&'static str, &'static str),
}

fn days_label(days: i64) -> String {
    match days {
        d if d < -1 => format!("overdue by {} days", -d),
        -1 => "overdue by 1 day".into(),
        0 => "due today".into(),
        1 => "due tomorrow".into(),
        d => format!("due in {d} days"),
    }
}

fn urgency(days: i64) -> i32 {
    match days {
        d if d < 0 => 50 + (-d).min(10) as i32 * 2,
        0 => 45,
        1 => 35,
        2..=3 => 25,
        4..=7 => 15,
        8..=14 => 5,
        _ => 0,
    }
}

fn importance(p: Priority) -> i32 {
    match p {
        Priority::High => 30,
        Priority::Medium => 20,
        Priority::Low => 10,
        Priority::None => 0,
    }
}

/// Score one actionable task. `parent` is its parent when it's a subtask.
pub fn score(task: &Task, parent: Option<&Task>, today: &str, now: i64) -> Scored {
    let mut factors = Vec::new();
    // A subtask inherits its parent's deadline and importance when they're stronger.
    let (due, due_from_parent) = match (&task.due_date, parent.and_then(|p| p.due_date.as_ref())) {
        (Some(own), Some(theirs)) if theirs < own => (Some(theirs.clone()), true),
        (Some(own), _) => (Some(own.clone()), false),
        (None, Some(theirs)) => (Some(theirs.clone()), true),
        (None, None) => (None, false),
    };
    let today_n = validate::days_from_civil(today);
    if let Some(d) = &due {
        let days = validate::days_from_civil(d) - today_n;
        let points = urgency(days);
        if points > 0 {
            let mut reason = days_label(days);
            if due_from_parent {
                reason.push_str(" (its parent task)");
            }
            factors.push(Factor { points, reason });
        }
    }
    let inherited = parent.map(|p| p.priority).filter(|p| *p > task.priority);
    let priority = inherited.unwrap_or(task.priority);
    if priority > Priority::None {
        let mut reason = format!("{} priority", priority.as_str());
        if inherited.is_some() {
            reason.push_str(" (its parent task)");
        }
        factors.push(Factor {
            points: importance(priority),
            reason,
        });
    }
    if task.status == Status::InProgress {
        factors.push(Factor {
            points: 10,
            reason: "already started".into(),
        });
    }
    if let Some(p) = parent {
        factors.push(Factor {
            points: 5,
            reason: format!("unblocks “{}”", p.title),
        });
    }
    let age = (now - task.created_at) / DAY_MS;
    if due.is_none() && age >= STALE_DAYS {
        factors.push(Factor {
            points: 3,
            reason: format!("waiting {age} days"),
        });
    }
    Scored {
        task_id: task.id.clone(),
        title: task.title.clone(),
        score: factors.iter().map(|f| f.points).sum(),
        factors,
        minutes: task.estimate_minutes.unwrap_or(DEFAULT_ESTIMATE),
        estimated: task.estimate_minutes.is_some(),
        due_date: due,
    }
}

/// Rank open, actionable tasks (most important first) and choose what fits.
///
/// A parent with open subtasks is blocked by them and isn't offered; its
/// subtasks are, carrying its deadline and priority.
pub fn propose(
    tasks: &[Task],
    today: &str,
    now: i64,
    c: &Constraints,
) -> Result<Proposal, PlanError> {
    let today = validate::date(today).map_err(|w| PlanError::Invalid("date", w))?;
    if let Some(m) = c.minutes {
        if m == 0 || m > MAX_AVAILABLE {
            return Err(PlanError::Invalid("minutes", "must be 1 to 960"));
        }
    }
    let max = c
        .max_items
        .unwrap_or(if c.minutes.is_some() { MAX_PICKS } else { TOP });
    if max == 0 || max > MAX_PICKS {
        return Err(PlanError::Invalid("limit", "must be 1 to 20"));
    }
    let open: Vec<&Task> = tasks.iter().filter(|t| t.status.is_open()).collect();
    let by_id = |id: &str| tasks.iter().find(|t| t.id == id);
    let has_open_children = |t: &Task| {
        open.iter()
            .any(|s| s.parent_id.as_deref() == Some(t.id.as_str()))
    };
    let mut blocked = 0;
    let mut ranked: Vec<Scored> = Vec::new();
    for t in &open {
        let parent = t.parent_id.as_deref().and_then(by_id);
        // A subtask of a closed parent is no longer part of any plan.
        if parent.is_some_and(|p| !p.status.is_open()) {
            continue;
        }
        let project = parent.map_or(&t.project_id, |p| &p.project_id);
        if c.project_id.is_some() && project != &c.project_id {
            continue;
        }
        if has_open_children(t) {
            blocked += 1;
            continue;
        }
        ranked.push(score(t, parent, &today, now));
    }
    ranked.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.due_date.is_none().cmp(&b.due_date.is_none()))
            .then(a.due_date.cmp(&b.due_date))
            .then(a.minutes.cmp(&b.minutes))
            .then(a.task_id.cmp(&b.task_id))
    });
    let mut picks = Vec::new();
    let mut too_long = 0;
    let mut left = c.minutes;
    for s in ranked {
        if picks.len() == max {
            break;
        }
        match left {
            Some(l) if s.minutes > l => too_long += 1,
            Some(l) => {
                left = Some(l - s.minutes);
                picks.push(s);
            }
            None => picks.push(s),
        }
    }
    Ok(Proposal {
        date: today,
        minutes: c.minutes,
        total_minutes: picks.iter().map(|s| s.minutes).sum(),
        picks,
        blocked,
        too_long,
    })
}

/// "45 min" / "1 h" / "1 h 30 min", for speech and previews.
pub fn duration(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-10-21";
    const NOW: i64 = 1_792_540_800_000; // 2026-10-21

    fn task(id: &str, title: &str) -> Task {
        Task {
            id: id.repeat(32).chars().take(32).collect(),
            owner_id: "u".into(),
            parent_id: None,
            project_id: None,
            title: title.into(),
            notes: String::new(),
            status: Status::Todo,
            priority: Priority::None,
            due_date: None,
            estimate_minutes: None,
            created_at: NOW,
            updated_at: NOW,
            completed_at: None,
        }
    }

    fn titles(p: &Proposal) -> Vec<&str> {
        p.picks.iter().map(|s| s.title.as_str()).collect()
    }

    #[test]
    fn urgency_and_importance_rank_with_reasons() {
        let mut overdue = task("a", "Overdue");
        overdue.due_date = Some("2026-10-19".into());
        let mut high = task("b", "High");
        high.priority = Priority::High;
        let mut soon = task("c", "Tomorrow");
        soon.due_date = Some("2026-10-22".into());
        let plain = task("d", "Plain");
        let p = propose(
            &[plain, soon, high, overdue],
            TODAY,
            NOW,
            &Constraints::default(),
        )
        .unwrap();
        assert_eq!(titles(&p), ["Overdue", "Tomorrow", "High", "Plain"]);
        assert_eq!(p.picks[0].score, 54);
        assert_eq!(p.picks[0].factors[0].reason, "overdue by 2 days");
        assert_eq!(p.picks[2].factors[0].reason, "high priority");
        assert!(p.picks[3].factors.is_empty());
    }

    #[test]
    fn parents_wait_for_subtasks_which_inherit_their_urgency() {
        let mut parent = task("a", "Move house");
        parent.due_date = Some("2026-10-21".into());
        parent.priority = Priority::High;
        let mut sub = task("b", "Book van");
        sub.parent_id = Some(parent.id.clone());
        let mut done_sub = task("c", "Pack books");
        done_sub.parent_id = Some(parent.id.clone());
        done_sub.status = Status::Done;
        let other = task("d", "Read");
        let p = propose(
            &[parent, sub, done_sub, other],
            TODAY,
            NOW,
            &Constraints::default(),
        )
        .unwrap();
        assert_eq!(titles(&p), ["Book van", "Read"]);
        assert_eq!(p.blocked, 1);
        let reasons: Vec<&str> = p.picks[0]
            .factors
            .iter()
            .map(|f| f.reason.as_str())
            .collect();
        assert_eq!(
            reasons,
            [
                "due today (its parent task)",
                "high priority (its parent task)",
                "unblocks “Move house”"
            ]
        );
    }

    #[test]
    fn time_budget_fills_in_rank_order_and_skips_what_does_not_fit() {
        let mut long = task("a", "Long");
        long.priority = Priority::High;
        long.estimate_minutes = Some(90);
        let mut short = task("b", "Short");
        short.priority = Priority::Medium;
        short.estimate_minutes = Some(15);
        let unestimated = task("c", "Unestimated");
        let c = Constraints {
            minutes: Some(45),
            ..Default::default()
        };
        let p = propose(&[long, short, unestimated], TODAY, NOW, &c).unwrap();
        assert_eq!(titles(&p), ["Short", "Unestimated"]);
        assert_eq!(p.total_minutes, 45);
        assert_eq!(p.too_long, 1);
        assert!(!p.picks[1].estimated);
    }

    #[test]
    fn project_filter_and_closed_tasks() {
        let mut a = task("a", "In garden");
        a.project_id = Some("p".repeat(32));
        let mut b = task("b", "Elsewhere");
        b.project_id = None;
        let mut c = task("c", "Done");
        c.project_id = Some("p".repeat(32));
        c.status = Status::Done;
        let cons = Constraints {
            project_id: Some("p".repeat(32)),
            ..Default::default()
        };
        let p = propose(&[a, b, c], TODAY, NOW, &cons).unwrap();
        assert_eq!(titles(&p), ["In garden"]);
    }

    #[test]
    fn stale_undated_tasks_get_a_nudge_and_in_progress_counts() {
        let mut old = task("a", "Old");
        old.created_at = NOW - 20 * DAY_MS;
        let mut started = task("b", "Started");
        started.status = Status::InProgress;
        let fresh = task("c", "Fresh");
        let p = propose(&[fresh, old, started], TODAY, NOW, &Constraints::default()).unwrap();
        assert_eq!(titles(&p), ["Started", "Old", "Fresh"]);
        assert_eq!(p.picks[1].factors[0].reason, "waiting 20 days");
    }

    #[test]
    fn validates_inputs() {
        let bad = |c: Constraints, date: &str| propose(&[], date, NOW, &c).unwrap_err();
        assert_eq!(
            bad(Constraints::default(), "nope"),
            PlanError::Invalid("date", "expected YYYY-MM-DD")
        );
        let zero = Constraints {
            minutes: Some(0),
            ..Default::default()
        };
        assert!(matches!(bad(zero, TODAY), PlanError::Invalid("minutes", _)));
        assert_eq!(duration(45), "45 min");
        assert_eq!(duration(90), "1 h 30 min");
        assert_eq!(duration(120), "2 h");
    }
}
