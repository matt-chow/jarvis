//! Reminder records and their validation (D-049).

use super::recurrence::{self, Rule};
use crate::tasks::Priority;
use crate::validate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_TITLE: usize = 200;
pub const MAX_NOTES: usize = 10_000;
pub const MAX_LINKS: usize = 5;
pub const MAX_LINK: usize = 2_000;
pub const MAX_ALERTS: usize = 5;
/// Furthest an alert may lead its occurrence: four weeks.
pub const MAX_LEAD_MINUTES: u32 = 40_320;
pub const MAX_LEAD_DAYS: u32 = 28;
pub const MAX_CHECKLIST: usize = 50;
pub const MAX_ITEM: usize = 200;
pub const MAX_REMINDERS: u32 = 5_000;
pub const MAX_LISTS: u32 = 100;
pub const MAX_LIST_NAME: usize = 60;
/// Date-only reminders alert and anchor "minutes before" alerts at 09:00 local.
pub const DAY_ANCHOR_MINUTE: u32 = 9 * 60;

/// When to notify for each occurrence. Independent of whether the reminder
/// has a time: a date-only reminder can alert at 08:00 the day before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Alert {
    /// `minutes` before the due time (09:00 for a date-only reminder).
    Before { minutes: u32 },
    /// At a local time, `days_before` the occurrence's date.
    At {
        time: String,
        #[serde(default)]
        days_before: u32,
    },
}

impl Alert {
    /// The dedup slot of this alert within one occurrence.
    pub fn slot(&self) -> String {
        match self {
            Alert::Before { minutes } => format!("b{minutes}"),
            Alert::At { time, days_before } => format!("a{days_before}-{}", time.replace(':', "")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Advance {
    /// The next occurrence follows the calendar rule.
    #[default]
    Calendar,
    /// The next occurrence is counted from the day it was completed.
    Completion,
}

/// What happens to an unresolved occurrence when the next one comes due.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Missed {
    /// Keep every occurrence open until each is resolved.
    Each,
    /// Carry one forward: the newest replaces the old, which is recorded missed.
    #[default]
    One,
    /// Record it missed at the end of its day and move on.
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistItem {
    #[serde(default)]
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub title: String,
    pub notes: String,
    pub links: Vec<String>,
    /// The current occurrence's date; `None` is unscheduled (never notifies).
    pub due_date: Option<String>,
    /// `HH:MM`, local to `timezone`; `None` is a date-only reminder.
    pub due_time: Option<String>,
    pub timezone: String,
    pub list_id: Option<String>,
    pub category_ids: Vec<String>,
    pub priority: Priority,
    pub flagged: bool,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    /// Complete this reminder when its linked task is completed.
    pub complete_with_task: bool,
    pub alerts: Vec<Alert>,
    pub recurrence: Option<Rule>,
    pub advance: Advance,
    pub missed: Missed,
    pub checklist: Vec<ChecklistItem>,
    /// The current occurrence is snoozed until this instant.
    pub snoozed_until: Option<i64>,
    /// With `missed: each`, later occurrences already completed (dates after `due_date`).
    pub completed_ahead: Vec<String>,
    /// With `missed: each`, open occurrences after the current one that are overdue too.
    pub backlog: u32,
    pub completed_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub version: i64,
    /// The version at which each field last changed (field-level merge).
    pub field_versions: BTreeMap<String, i64>,
    /// When the current occurrence is overdue (derived; for sorting and views).
    pub due_at: Option<i64>,
    /// Alerts at or before this instant have been handled.
    #[serde(skip)]
    pub notified_through: i64,
    /// When the scheduler next needs to look at this reminder.
    #[serde(skip)]
    pub wake_at: Option<i64>,
}

impl Reminder {
    /// The current occurrence's stable id: `<reminder id>:<date>`, or
    /// `<reminder id>:-` when unscheduled.
    pub fn occurrence_id(&self) -> String {
        occurrence_id(&self.id, self.due_date.as_deref())
    }

    pub fn recurring(&self) -> bool {
        self.recurrence.is_some() && self.due_date.is_some()
    }
}

pub fn occurrence_id(id: &str, date: Option<&str>) -> String {
    format!("{id}:{}", date.unwrap_or("-"))
}

fn invalid(field: &'static str) -> impl Fn(&'static str) -> (&'static str, &'static str) {
    move |why| (field, why)
}

pub type Bad = (&'static str, &'static str);

pub fn title(raw: &str) -> Result<String, Bad> {
    validate::line(raw, MAX_TITLE).map_err(invalid("title"))
}

pub fn notes(raw: &str) -> Result<String, Bad> {
    if raw.trim().is_empty() {
        return Ok(String::new());
    }
    validate::text(raw, MAX_NOTES).map_err(invalid("notes"))
}

pub fn links(raw: Vec<String>) -> Result<Vec<String>, Bad> {
    let out: Vec<String> = raw
        .iter()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect();
    if out.len() > MAX_LINKS {
        return Err(("links", "at most 5 links"));
    }
    for l in &out {
        let rest = l
            .strip_prefix("https://")
            .or_else(|| l.strip_prefix("http://"));
        if l.len() > MAX_LINK
            || rest.is_none_or(|r| r.is_empty())
            || l.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(("links", "expected http or https URLs"));
        }
    }
    Ok(out)
}

/// `HH:MM`, 00:00–23:59.
pub fn time(raw: &str) -> Result<String, Bad> {
    let m = validate::clock(raw, false).map_err(invalid("due_time"))?;
    Ok(format!("{:02}:{:02}", m / 60, m % 60))
}

pub fn minute_of(time: &str) -> u32 {
    validate::clock(time, false).unwrap_or(DAY_ANCHOR_MINUTE)
}

pub fn alerts(raw: Vec<Alert>) -> Result<Vec<Alert>, Bad> {
    if raw.len() > MAX_ALERTS {
        return Err(("alerts", "at most 5 alerts"));
    }
    let mut out: Vec<Alert> = Vec::new();
    for a in raw {
        let a = match a {
            Alert::Before { minutes } if minutes <= MAX_LEAD_MINUTES => Alert::Before { minutes },
            Alert::Before { .. } => return Err(("alerts", "at most four weeks before")),
            Alert::At {
                time: t,
                days_before,
            } if days_before <= MAX_LEAD_DAYS => Alert::At {
                time: time(&t).map_err(|_| ("alerts", "expected HH:MM"))?,
                days_before,
            },
            Alert::At { .. } => return Err(("alerts", "at most 28 days before")),
        };
        if !out.contains(&a) {
            out.push(a);
        }
    }
    Ok(out)
}

/// The alerts a new reminder gets when none are given: at the due time, or
/// 09:00 on the day for a date-only reminder.
pub fn default_alerts(due_time: Option<&str>) -> Vec<Alert> {
    match due_time {
        Some(_) => vec![Alert::Before { minutes: 0 }],
        None => vec![Alert::At {
            time: "09:00".into(),
            days_before: 0,
        }],
    }
}

pub fn checklist<F: FnMut() -> String>(
    raw: Vec<ChecklistItem>,
    mut new_id: F,
) -> Result<Vec<ChecklistItem>, Bad> {
    if raw.len() > MAX_CHECKLIST {
        return Err(("checklist", "at most 50 items"));
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in raw {
        let text = validate::line(&item.text, MAX_ITEM).map_err(invalid("checklist"))?;
        let ok_id = !item.id.is_empty()
            && item.id.len() <= 32
            && item
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-');
        let mut id = if ok_id { item.id } else { new_id() };
        if !seen.insert(id.clone()) {
            id = new_id();
            seen.insert(id.clone());
        }
        out.push(ChecklistItem {
            id,
            text,
            done: item.done,
        });
    }
    Ok(out)
}

/// A rule for a reminder dated `due_date`; repeating needs a date.
pub fn rule(raw: Option<Rule>, due_date: Option<&str>) -> Result<Option<Rule>, Bad> {
    match (raw, due_date) {
        (None, _) => Ok(None),
        (Some(_), None) => Err(("recurrence", "a repeating reminder needs a date")),
        (Some(r), Some(d)) => recurrence::check(r, d).map(Some),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderList {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub name: String,
    pub color: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewList {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

pub fn list_name(raw: &str) -> Result<String, Bad> {
    validate::line(raw, MAX_LIST_NAME).map_err(invalid("name"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_fields() {
        assert!(title("  ").is_err());
        assert_eq!(title(" Pay rent ").unwrap(), "Pay rent");
        assert_eq!(
            links(vec![" https://a.example/x ".into(), "".into()]).unwrap(),
            ["https://a.example/x"]
        );
        assert!(links(vec!["javascript:alert(1)".into()]).is_err());
        assert!(links(vec!["https://".into()]).is_err());
        assert!(links(vec!["https://a.example/x y".into()]).is_err());
        assert!(links(vec!["https://a.example".into(); 6]).is_err());
        assert_eq!(time("08:05").unwrap(), "08:05");
        assert!(time("8:05").is_err());
        assert!(time("24:00").is_err());
        let a = alerts(vec![
            Alert::Before { minutes: 15 },
            Alert::Before { minutes: 15 },
            Alert::At {
                time: "08:00".into(),
                days_before: 1,
            },
        ])
        .unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[1].slot(), "a1-0800");
        assert!(alerts(vec![Alert::Before {
            minutes: MAX_LEAD_MINUTES + 1
        }])
        .is_err());
        assert!(alerts(vec![Alert::At {
            time: "08:00".into(),
            days_before: 29
        }])
        .is_err());
        assert!(alerts(vec![Alert::Before { minutes: 0 }; 6]).is_err());
        let mut n = 0;
        let items = checklist(
            vec![
                ChecklistItem {
                    id: "a".into(),
                    text: "one".into(),
                    done: false,
                },
                ChecklistItem {
                    id: "a".into(),
                    text: "two".into(),
                    done: true,
                },
                ChecklistItem {
                    id: "<bad>".into(),
                    text: "three".into(),
                    done: false,
                },
            ],
            || {
                n += 1;
                format!("n{n}")
            },
        )
        .unwrap();
        assert_eq!(
            items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["a", "n1", "n2"]
        );
        assert!(rule(
            Some(Rule {
                freq: recurrence::Freq::Daily,
                interval: 1,
                weekdays: vec![],
                until: None,
                anchor: String::new()
            }),
            None
        )
        .is_err());
    }
}
