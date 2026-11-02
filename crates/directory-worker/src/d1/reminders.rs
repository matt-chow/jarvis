//! D1 implementation of `ReminderRepository` (migration 0026).
//!
//! A save is one batch: the reminder row is updated only if its version is
//! still the expected one, and the dependent rows (categories, occurrences,
//! history) are written only if that update took, checked through a
//! per-save `write_id`.

use super::{changes, count, err, int, opt_int, opt_text, stmt, text};
use crate::entropy::WebCryptoEntropy;
use directory_core::entropy::new_id;
use directory_core::notifications::Notice;
use directory_core::reminders::{
    HistoryEntry, OccurrenceChange, OccurrenceRow, Reminder, ReminderList, ReminderRepository,
};
use directory_core::tasks::Priority;
use serde::Deserialize;
use std::collections::HashMap;
use worker::wasm_bindgen::JsValue;
use worker::{D1Database, D1PreparedStatement};

pub struct D1ReminderRepository {
    db: D1Database,
}

impl D1ReminderRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }

    /// Every history line the owner has, oldest first (export).
    pub async fn all_history(&self, owner_id: &str) -> Result<Vec<HistoryEntry>, String> {
        self.history_where("owner_id = ?1 ORDER BY at, rowid", &[text(owner_id)])
            .await
    }

    async fn history_where(
        &self,
        filter: &str,
        args: &[JsValue],
    ) -> Result<Vec<HistoryEntry>, String> {
        let sql = format!(
            "SELECT id, reminder_id, occurrence, action, at, until FROM reminder_history WHERE {filter}"
        );
        let rows = stmt(&self.db, &sql, args)?
            .all()
            .await
            .map_err(err)?
            .results::<HistRow>()
            .map_err(err)?;
        Ok(rows
            .into_iter()
            .map(|h| HistoryEntry {
                id: h.id,
                reminder_id: h.reminder_id,
                occurrence: h.occurrence,
                action: h.action,
                at: h.at as i64,
                until: h.until.map(|u| u as i64),
            })
            .collect())
    }
}

const COLUMNS: &str = "id, owner_id, title, notes, links, due_date, due_time, timezone, list_id, \
    priority, flagged, project_id, task_id, complete_with_task, alerts, recurrence, advance, \
    missed, checklist, snoozed_until, completed_ahead, backlog, completed_at, deleted_at, \
    created_at, updated_at, version, field_versions, due_at, notified_through, wake_at";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    title: String,
    notes: String,
    links: String,
    due_date: Option<String>,
    due_time: Option<String>,
    timezone: String,
    list_id: Option<String>,
    priority: String,
    flagged: f64,
    project_id: Option<String>,
    task_id: Option<String>,
    complete_with_task: f64,
    alerts: String,
    recurrence: Option<String>,
    advance: String,
    missed: String,
    checklist: String,
    snoozed_until: Option<f64>,
    completed_ahead: String,
    backlog: f64,
    completed_at: Option<f64>,
    deleted_at: Option<f64>,
    created_at: f64,
    updated_at: f64,
    version: f64,
    field_versions: String,
    due_at: Option<f64>,
    notified_through: f64,
    wake_at: Option<f64>,
}

fn json<T: serde::de::DeserializeOwned + Default>(s: &str) -> T {
    serde_json::from_str(s).unwrap_or_default()
}

fn word<T: serde::de::DeserializeOwned + Default>(s: &str) -> T {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).unwrap_or_default()
}

fn to_json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

/// A serde enum's wire word (`"calendar"` → `calendar`).
fn word_of<T: serde::Serialize>(v: &T) -> String {
    to_json(v).trim_matches('"').to_owned()
}

impl Row {
    fn into_reminder(self, categories: Vec<String>) -> Reminder {
        Reminder {
            id: self.id,
            owner_id: self.owner_id,
            title: self.title,
            notes: self.notes,
            links: json(&self.links),
            due_date: self.due_date,
            due_time: self.due_time,
            timezone: self.timezone,
            list_id: self.list_id,
            category_ids: categories,
            priority: Priority::parse(&self.priority).unwrap_or(Priority::None),
            flagged: self.flagged != 0.0,
            project_id: self.project_id,
            task_id: self.task_id,
            complete_with_task: self.complete_with_task != 0.0,
            alerts: json(&self.alerts),
            recurrence: self.recurrence.and_then(|r| serde_json::from_str(&r).ok()),
            advance: word(&self.advance),
            missed: word(&self.missed),
            checklist: json(&self.checklist),
            snoozed_until: self.snoozed_until.map(|v| v as i64),
            completed_ahead: json(&self.completed_ahead),
            backlog: self.backlog as u32,
            completed_at: self.completed_at.map(|v| v as i64),
            deleted_at: self.deleted_at.map(|v| v as i64),
            created_at: self.created_at as i64,
            updated_at: self.updated_at as i64,
            version: self.version as i64,
            field_versions: json(&self.field_versions),
            due_at: self.due_at.map(|v| v as i64),
            notified_through: self.notified_through as i64,
            wake_at: self.wake_at.map(|v| v as i64),
        }
    }
}

/// The bound values for COLUMNS, in order.
fn values(r: &Reminder) -> Vec<JsValue> {
    vec![
        text(&r.id),
        text(&r.owner_id),
        text(&r.title),
        text(&r.notes),
        text(&to_json(&r.links)),
        opt_text(r.due_date.as_deref()),
        opt_text(r.due_time.as_deref()),
        text(&r.timezone),
        opt_text(r.list_id.as_deref()),
        text(r.priority.as_str()),
        int(i64::from(r.flagged)),
        opt_text(r.project_id.as_deref()),
        opt_text(r.task_id.as_deref()),
        int(i64::from(r.complete_with_task)),
        text(&to_json(&r.alerts)),
        r.recurrence
            .as_ref()
            .map_or(JsValue::NULL, |x| text(&to_json(x))),
        text(&word_of(&r.advance)),
        text(&word_of(&r.missed)),
        text(&to_json(&r.checklist)),
        opt_int(r.snoozed_until),
        text(&to_json(&r.completed_ahead)),
        int(i64::from(r.backlog)),
        opt_int(r.completed_at),
        opt_int(r.deleted_at),
        int(r.created_at),
        int(r.updated_at),
        int(r.version),
        text(&to_json(&r.field_versions)),
        opt_int(r.due_at),
        int(r.notified_through),
        opt_int(r.wake_at),
    ]
}

#[derive(Deserialize)]
struct CategoryRow {
    reminder_id: String,
    category_id: String,
}

#[derive(Deserialize)]
struct OccRow {
    reminder_id: String,
    occurrence: String,
    state: String,
    at: f64,
}

#[derive(Deserialize)]
struct HistRow {
    id: String,
    reminder_id: String,
    occurrence: String,
    action: String,
    at: f64,
    until: Option<f64>,
}

#[derive(Deserialize)]
struct ListRow {
    id: String,
    owner_id: String,
    name: String,
    color: String,
    created_at: f64,
    updated_at: f64,
}

impl From<ListRow> for ReminderList {
    fn from(l: ListRow) -> Self {
        ReminderList {
            id: l.id,
            owner_id: l.owner_id,
            name: l.name,
            color: l.color,
            created_at: l.created_at as i64,
            updated_at: l.updated_at as i64,
        }
    }
}

/// Only if this batch's reminder update took.
const TOOK: &str = "EXISTS (SELECT 1 FROM reminders WHERE id = ?1 AND write_id = ?2)";

impl D1ReminderRepository {
    /// Reminders matching `filter` (with `?1` the owner, or the scan bound), with categories.
    async fn select(
        &self,
        filter: &str,
        categories_filter: &str,
        args: &[JsValue],
    ) -> Result<Vec<Reminder>, String> {
        let sql = format!("SELECT {COLUMNS} FROM reminders WHERE {filter}");
        let rows = stmt(&self.db, &sql, args)?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let csql = format!(
            "SELECT reminder_id, category_id FROM reminder_categories
              WHERE {categories_filter} ORDER BY reminder_id, position"
        );
        let mut cats: HashMap<String, Vec<String>> = HashMap::new();
        for c in stmt(&self.db, &csql, args)?
            .all()
            .await
            .map_err(err)?
            .results::<CategoryRow>()
            .map_err(err)?
        {
            cats.entry(c.reminder_id).or_default().push(c.category_id);
        }
        Ok(rows
            .into_iter()
            .map(|r| {
                let c = cats.remove(&r.id).unwrap_or_default();
                r.into_reminder(c)
            })
            .collect())
    }

    fn category_rows(
        &self,
        r: &Reminder,
        guard: Option<&str>,
    ) -> Result<Vec<D1PreparedStatement>, String> {
        let token = guard.unwrap_or("");
        let cond = if guard.is_some() { TOOK } else { "1" };
        let mut out = vec![stmt(
            &self.db,
            &format!(
                "DELETE FROM reminder_categories WHERE reminder_id = ?1 AND ?2 = ?2 AND {cond}"
            ),
            &[text(&r.id), text(token)],
        )?];
        for (i, c) in r.category_ids.iter().enumerate() {
            out.push(stmt(
                &self.db,
                &format!(
                    "INSERT INTO reminder_categories (reminder_id, category_id, owner_id, position, created_at)
                     SELECT ?1, ?3, ?4, ?5, ?6 WHERE ?2 = ?2 AND {cond}
                       AND EXISTS (SELECT 1 FROM categories WHERE id = ?3 AND owner_id = ?4)"
                ),
                &[
                    text(&r.id),
                    text(token),
                    text(c),
                    text(&r.owner_id),
                    int(i as i64),
                    int(r.updated_at),
                ],
            )?);
        }
        Ok(out)
    }
}

impl ReminderRepository for D1ReminderRepository {
    async fn list(&self, owner_id: &str) -> Result<Vec<Reminder>, String> {
        self.select(
            "owner_id = ?1 ORDER BY due_at IS NULL, due_at, created_at",
            "owner_id = ?1",
            &[text(owner_id)],
        )
        .await
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Reminder>, String> {
        Ok(self
            .select(
                "owner_id = ?1 AND id = ?2",
                "owner_id = ?1 AND reminder_id = ?2",
                &[text(owner_id), text(id)],
            )
            .await?
            .pop())
    }

    async fn count(&self, owner_id: &str) -> Result<u32, String> {
        count(
            &self.db,
            "SELECT COUNT(*) AS n FROM reminders WHERE owner_id = ?1",
            &[text(owner_id)],
        )
        .await
    }

    async fn insert(&self, r: &Reminder) -> Result<(), String> {
        let marks: Vec<String> = (1..=31).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "INSERT INTO reminders ({COLUMNS}) VALUES ({})",
            marks.join(", ")
        );
        let mut batch = vec![stmt(&self.db, &sql, &values(r))?];
        batch.extend(self.category_rows(r, None)?);
        self.db.batch(batch).await.map_err(err)?;
        Ok(())
    }

    async fn save(
        &self,
        r: &Reminder,
        expected: i64,
        occurrence_changes: &[OccurrenceChange],
        history: &[HistoryEntry],
        notices: &[Notice],
    ) -> Result<bool, String> {
        let token = new_id(&WebCryptoEntropy);
        // ?1 id, ?2 owner, ?3.. the other columns, then the version check and token.
        let cols: Vec<&str> = COLUMNS.split(", ").map(str::trim).collect();
        let sets: Vec<String> = cols
            .iter()
            .enumerate()
            .skip(2)
            .map(|(i, c)| format!("{c} = ?{}", i + 1))
            .collect();
        let sql = format!(
            "UPDATE reminders SET {}, write_id = ?33 WHERE id = ?1 AND owner_id = ?2 AND version = ?32",
            sets.join(", ")
        );
        let mut args = values(r);
        args.push(int(expected));
        args.push(text(&token));
        let mut batch = vec![stmt(&self.db, &sql, &args)?];
        batch.extend(self.category_rows(r, Some(&token))?);
        for c in occurrence_changes {
            batch.push(match c {
                OccurrenceChange::Set(row) => stmt(
                    &self.db,
                    &format!(
                        "INSERT OR REPLACE INTO reminder_occurrences (reminder_id, occurrence, owner_id, state, at)
                         SELECT ?1, ?3, ?4, ?5, ?6 WHERE {TOOK}"
                    ),
                    &[
                        text(&r.id),
                        text(&token),
                        text(&row.occurrence),
                        text(&r.owner_id),
                        text(&row.state),
                        int(row.at),
                    ],
                )?,
                OccurrenceChange::Remove(d) => stmt(
                    &self.db,
                    &format!(
                        "DELETE FROM reminder_occurrences WHERE reminder_id = ?1 AND occurrence = ?3 AND {TOOK}"
                    ),
                    &[text(&r.id), text(&token), text(d)],
                )?,
            });
        }
        for h in history {
            batch.push(stmt(
                &self.db,
                &format!(
                    "INSERT INTO reminder_history (id, reminder_id, owner_id, occurrence, action, at, until)
                     SELECT ?3, ?1, ?4, ?5, ?6, ?7, ?8 WHERE {TOOK}"
                ),
                &[
                    text(&r.id),
                    text(&token),
                    text(&h.id),
                    text(&r.owner_id),
                    text(&h.occurrence),
                    text(&h.action),
                    int(h.at),
                    opt_int(h.until),
                ],
            )?);
        }
        for n in notices {
            batch.push(stmt(
                &self.db,
                &format!(
                    "INSERT OR IGNORE INTO notification_log (id, owner_id, reminder_id, occurrence, slot, due_at, state, created_at)
                     SELECT ?3, ?4, ?1, ?5, ?6, ?7, ?8, ?9 WHERE {TOOK}"
                ),
                &[
                    text(&r.id),
                    text(&token),
                    text(&n.id),
                    text(&r.owner_id),
                    text(&n.occurrence),
                    text(&n.slot),
                    int(n.due_at),
                    text(&n.state),
                    int(n.created_at),
                ],
            )?);
        }
        let results = self.db.batch(batch).await.map_err(err)?;
        Ok(results.first().is_some_and(|x| changes(x) > 0))
    }

    async fn purge(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let args = [text(owner_id), text(id)];
        let mut batch = vec![stmt(
            &self.db,
            "DELETE FROM notification_deliveries WHERE owner_id = ?1 AND notice_id IN
             (SELECT id FROM notification_log WHERE owner_id = ?1 AND reminder_id = ?2)",
            &args,
        )?];
        for table in [
            "notification_log",
            "reminder_history",
            "reminder_occurrences",
            "reminder_categories",
        ] {
            batch.push(stmt(
                &self.db,
                &format!("DELETE FROM {table} WHERE owner_id = ?1 AND reminder_id = ?2"),
                &args,
            )?);
        }
        batch.push(stmt(
            &self.db,
            "DELETE FROM reminders WHERE owner_id = ?1 AND id = ?2",
            &args,
        )?);
        let results = self.db.batch(batch).await.map_err(err)?;
        Ok(results.last().is_some_and(|r| changes(r) > 0))
    }

    async fn occurrences(
        &self,
        owner_id: &str,
        reminder_id: Option<&str>,
        from: &str,
        to: &str,
    ) -> Result<Vec<OccurrenceRow>, String> {
        let rows = stmt(
            &self.db,
            "SELECT reminder_id, occurrence, state, at FROM reminder_occurrences
              WHERE owner_id = ?1 AND (?2 IS NULL OR reminder_id = ?2)
                AND occurrence >= ?3 AND occurrence <= ?4
              ORDER BY occurrence LIMIT 5000",
            &[text(owner_id), opt_text(reminder_id), text(from), text(to)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<OccRow>()
        .map_err(err)?;
        Ok(rows
            .into_iter()
            .map(|o| OccurrenceRow {
                reminder_id: o.reminder_id,
                occurrence: o.occurrence,
                state: o.state,
                at: o.at as i64,
            })
            .collect())
    }

    async fn history(
        &self,
        owner_id: &str,
        id: &str,
        limit: u32,
    ) -> Result<Vec<HistoryEntry>, String> {
        self.history_where(
            "owner_id = ?1 AND reminder_id = ?2 ORDER BY at DESC, rowid DESC LIMIT ?3",
            &[text(owner_id), text(id), int(i64::from(limit))],
        )
        .await
    }

    async fn category_ids(&self, owner_id: &str) -> Result<Vec<String>, String> {
        #[derive(Deserialize)]
        struct Id {
            id: String,
        }
        Ok(stmt(
            &self.db,
            "SELECT id FROM categories WHERE owner_id = ?1",
            &[text(owner_id)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<Id>()
        .map_err(err)?
        .into_iter()
        .map(|r| r.id)
        .collect())
    }

    async fn task_exists(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        Ok(count(
            &self.db,
            "SELECT COUNT(*) AS n FROM tasks WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )
        .await?
            > 0)
    }

    async fn project_exists(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        Ok(count(
            &self.db,
            "SELECT COUNT(*) AS n FROM projects WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )
        .await?
            > 0)
    }

    async fn linked_to_task(&self, owner_id: &str, task_id: &str) -> Result<Vec<Reminder>, String> {
        self.select(
            "owner_id = ?1 AND task_id = ?2",
            "owner_id = ?1 AND reminder_id IN (SELECT id FROM reminders WHERE owner_id = ?1 AND task_id = ?2)",
            &[text(owner_id), text(task_id)],
        )
        .await
    }

    async fn lists(&self, owner_id: &str) -> Result<Vec<ReminderList>, String> {
        Ok(stmt(
            &self.db,
            "SELECT id, owner_id, name, color, created_at, updated_at FROM reminder_lists
              WHERE owner_id = ?1 ORDER BY name COLLATE NOCASE",
            &[text(owner_id)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<ListRow>()
        .map_err(err)?
        .into_iter()
        .map(ReminderList::from)
        .collect())
    }

    async fn insert_list(&self, l: &ReminderList) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO reminder_lists (id, owner_id, name, color, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[
                text(&l.id),
                text(&l.owner_id),
                text(&l.name),
                text(&l.color),
                int(l.created_at),
                int(l.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn update_list(&self, l: &ReminderList) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE reminder_lists SET name = ?3, color = ?4, updated_at = ?5
              WHERE owner_id = ?1 AND id = ?2",
            &[
                text(&l.owner_id),
                text(&l.id),
                text(&l.name),
                text(&l.color),
                int(l.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete_list(&self, owner_id: &str, id: &str, now: i64) -> Result<bool, String> {
        let results = self
            .db
            .batch(vec![
                stmt(
                    &self.db,
                    "UPDATE reminders SET list_id = NULL, version = version + 1, updated_at = ?3
                      WHERE owner_id = ?1 AND list_id = ?2",
                    &[text(owner_id), text(id), int(now)],
                )?,
                stmt(
                    &self.db,
                    "DELETE FROM reminder_lists WHERE owner_id = ?1 AND id = ?2",
                    &[text(owner_id), text(id)],
                )?,
            ])
            .await
            .map_err(err)?;
        Ok(results.get(1).is_some_and(|r| changes(r) > 0))
    }

    async fn due_for_scan(&self, now: i64, limit: u32) -> Result<Vec<Reminder>, String> {
        let due = "wake_at <= ?1 AND deleted_at IS NULL AND completed_at IS NULL";
        self.select(
            &format!("{due} ORDER BY wake_at LIMIT ?2"),
            &format!(
                "?2 = ?2 AND reminder_id IN (SELECT id FROM reminders WHERE {due} ORDER BY wake_at LIMIT ?2)"
            ),
            &[int(now), int(i64::from(limit))],
        )
        .await
    }
}
