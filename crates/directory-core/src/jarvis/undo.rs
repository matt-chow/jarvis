//! Reversals recorded when a plan's steps run (DECISIONS D-048).
//!
//! "Undo" never asks a model what happened: each step that can be reversed
//! stores an [`UndoOp`] in its result, built from what the step actually
//! changed. A reversal refuses to touch an item that changed since (its
//! `updated_at` moved on), so undoing never overwrites later edits.

use crate::blocks::TimeBlock;
use crate::preferences::JarvisPrefs;
use crate::tasks::{Priority, Status};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum UndoOp {
    /// Remove a task the step created (its subtasks go with it).
    DeleteTask {
        id: String,
        title: String,
        updated_at: i64,
    },
    /// Put back fields a step changed on a task.
    RestoreTask {
        id: String,
        title: String,
        status: Status,
        priority: Priority,
        due_date: Option<String>,
        estimate_minutes: Option<u32>,
        /// The task's `updated_at` after the step.
        updated_at: i64,
    },
    DeleteBlock {
        id: String,
        title: String,
        updated_at: i64,
    },
    /// Put a session back as it was: moved back when `updated_at` (after the
    /// step) is set, created again when the step removed it.
    RestoreBlock {
        block: TimeBlock,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        updated_at: Option<i64>,
    },
    DeleteEvent {
        id: String,
        title: String,
        updated_at: i64,
    },
    /// Move an event back.
    RestoreEvent {
        id: String,
        title: String,
        start_date: String,
        end_date: String,
        start_time: Option<String>,
        end_time: Option<String>,
        updated_at: i64,
    },
    /// Put JARVIS preferences back (`None` = the defaults). `after` is what
    /// the step saved; a later change stops the reversal.
    RestorePrefs {
        before: Option<JarvisPrefs>,
        after: Option<JarvisPrefs>,
    },
}

impl UndoOp {
    /// Whether reversing removes something.
    pub fn deletes(&self) -> bool {
        matches!(
            self,
            Self::DeleteTask { .. } | Self::DeleteBlock { .. } | Self::DeleteEvent { .. }
        )
    }

    pub fn summary(&self) -> String {
        match self {
            Self::DeleteTask { title, .. } => format!("Delete the task “{title}”"),
            Self::RestoreTask { title, .. } => format!("Restore the task “{title}”"),
            Self::DeleteBlock { title, .. } => format!("Remove the session “{title}”"),
            Self::RestoreBlock {
                block,
                updated_at: Some(_),
            } => format!(
                "Move “{}” back to {} {}–{}",
                block.title,
                super::day_label(&block.date),
                block.start_time,
                block.end_time
            ),
            Self::RestoreBlock { block, .. } => format!(
                "Put back the session “{}” {} {}–{}",
                block.title,
                super::day_label(&block.date),
                block.start_time,
                block.end_time
            ),
            Self::DeleteEvent { title, .. } => format!("Delete the event “{title}”"),
            Self::RestoreEvent {
                title, start_date, ..
            } => format!(
                "Move the event “{title}” back to {}",
                super::day_label(start_date)
            ),
            Self::RestorePrefs { before: None, .. } => "Reset JARVIS scheduling preferences".into(),
            Self::RestorePrefs {
                before: Some(p), ..
            } => format!("Restore preferences: {}", describe_prefs(p)),
        }
    }
}

/// "no sessions after 21:00, 10 min breaks, …" (or "defaults").
pub fn describe_prefs(p: &JarvisPrefs) -> String {
    let mut parts = Vec::new();
    if let Some(t) = &p.avoid_after {
        parts.push(format!("no sessions after {t}"));
    }
    if p.break_minutes > 0 {
        parts.push(format!("{} min breaks", p.break_minutes));
    }
    if let Some(m) = p.default_task_minutes {
        parts.push(format!(
            "tasks without an estimate take {}",
            crate::planner::duration(m)
        ));
    }
    if let Some(m) = p.max_daily_minutes {
        parts.push(format!("at most {} a day", crate::planner::duration(m)));
    }
    if p.earliest_first {
        parts.push("earliest free time first".into());
    }
    if parts.is_empty() {
        "defaults".into()
    } else {
        parts.join(", ")
    }
}
