//! D1 implementation of `TaskRepository` (migrations 0004, 0005, 0021, 0023).

use super::{changes, count, err, int, opt_int, opt_text, stmt, text};
use directory_core::calendar::Repeat;
use directory_core::tasks::{Filter, Priority, Status, Task, TaskRepository};
use serde::Deserialize;
use worker::wasm_bindgen::JsValue;
use worker::D1Database;

pub struct D1TaskRepository {
    db: D1Database,
}

impl D1TaskRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str =
    "id, owner_id, parent_id, title, notes, status, priority, due_date, created_at, updated_at, completed_at, project_id, estimate_minutes, repeat, repeat_until";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    parent_id: Option<String>,
    title: String,
    notes: String,
    status: String,
    priority: String,
    due_date: Option<String>,
    created_at: f64,
    updated_at: f64,
    completed_at: Option<f64>,
    project_id: Option<String>,
    estimate_minutes: Option<f64>,
    repeat: String,
    repeat_until: Option<String>,
}

impl TryFrom<Row> for Task {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        Ok(Task {
            status: Status::parse(&r.status).ok_or("corrupt task status")?,
            priority: Priority::parse(&r.priority).ok_or("corrupt task priority")?,
            id: r.id,
            owner_id: r.owner_id,
            parent_id: r.parent_id,
            project_id: r.project_id,
            title: r.title,
            notes: r.notes,
            due_date: r.due_date,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
            completed_at: r.completed_at.map(|t| t as i64),
            estimate_minutes: r.estimate_minutes.map(|m| m as u32),
            repeat: Repeat::parse(&r.repeat).ok_or("corrupt task repeat")?,
            repeat_until: r.repeat_until,
        })
    }
}

fn values(t: &Task) -> Vec<JsValue> {
    vec![
        text(&t.id),
        text(&t.owner_id),
        opt_text(t.parent_id.as_deref()),
        text(&t.title),
        text(&t.notes),
        text(t.status.as_str()),
        text(t.priority.as_str()),
        opt_text(t.due_date.as_deref()),
        int(t.created_at),
        int(t.updated_at),
        opt_int(t.completed_at),
        opt_text(t.project_id.as_deref()),
        opt_int(t.estimate_minutes.map(i64::from)),
        text(t.repeat.as_str()),
        opt_text(t.repeat_until.as_deref()),
    ]
}

const PLACEHOLDERS: &str = "?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15";

impl TaskRepository for D1TaskRepository {
    async fn list(&self, owner_id: &str, filter: Filter, limit: u32) -> Result<Vec<Task>, String> {
        let (status_clause, order) = match filter {
            Filter::All => ("", "created_at"),
            Filter::Open => ("AND status IN ('todo', 'in_progress')", "created_at"),
            Filter::Closed => (
                "AND status IN ('done', 'cancelled')",
                "completed_at DESC, updated_at DESC",
            ),
        };
        let sql = format!(
            "SELECT {COLUMNS} FROM tasks WHERE owner_id = ?1 {status_clause} ORDER BY {order} LIMIT ?2"
        );
        stmt(&self.db, &sql, &[text(owner_id), int(limit.into())])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(Task::try_from)
            .collect()
    }

    async fn count_open(&self, owner_id: &str) -> Result<u32, String> {
        count(
            &self.db,
            "SELECT COUNT(*) AS n FROM tasks WHERE owner_id = ?1 AND status IN ('todo', 'in_progress')",
            &[text(owner_id)],
        )
        .await
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Task>, String> {
        let sql = format!("SELECT {COLUMNS} FROM tasks WHERE owner_id = ?1 AND id = ?2");
        stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(Task::try_from)
            .transpose()
    }

    async fn insert(&self, t: &Task) -> Result<(), String> {
        let sql = format!("INSERT INTO tasks ({COLUMNS}) VALUES ({PLACEHOLDERS})");
        stmt(&self.db, &sql, &values(t))?.run().await.map_err(err)?;
        Ok(())
    }

    async fn insert_if_absent(&self, t: &Task) -> Result<bool, String> {
        let sql = format!("INSERT OR IGNORE INTO tasks ({COLUMNS}) VALUES ({PLACEHOLDERS})");
        let r = stmt(&self.db, &sql, &values(t))?.run().await.map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn update(&self, t: &Task) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE tasks SET title = ?3, notes = ?4, status = ?5, priority = ?6, due_date = ?7,
                    updated_at = ?8, completed_at = ?9, project_id = ?10,
                    estimate_minutes = ?11, repeat = ?12, repeat_until = ?13
              WHERE owner_id = ?1 AND id = ?2",
            &[
                text(&t.owner_id),
                text(&t.id),
                text(&t.title),
                text(&t.notes),
                text(t.status.as_str()),
                text(t.priority.as_str()),
                opt_text(t.due_date.as_deref()),
                int(t.updated_at),
                opt_int(t.completed_at),
                opt_text(t.project_id.as_deref()),
                opt_int(t.estimate_minutes.map(i64::from)),
                text(t.repeat.as_str()),
                opt_text(t.repeat_until.as_deref()),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        // Explicit child delete: does not rely on foreign-key enforcement.
        let results = self
            .db
            .batch(vec![
                // Its work sessions, and its subtasks' (time blocks, 0023).
                stmt(
                    &self.db,
                    "DELETE FROM time_blocks WHERE owner_id = ?1 AND (task_id = ?2 OR task_id IN
                        (SELECT id FROM tasks WHERE owner_id = ?1 AND parent_id = ?2))",
                    &[text(owner_id), text(id)],
                )?,
                stmt(
                    &self.db,
                    "DELETE FROM tasks WHERE owner_id = ?1 AND parent_id = ?2",
                    &[text(owner_id), text(id)],
                )?,
                stmt(
                    &self.db,
                    "DELETE FROM tasks WHERE owner_id = ?1 AND id = ?2",
                    &[text(owner_id), text(id)],
                )?,
            ])
            .await
            .map_err(err)?;
        Ok(results.get(2).is_some_and(|r| changes(r) > 0))
    }

    async fn project_exists(&self, owner_id: &str, project_id: &str) -> Result<bool, String> {
        let n = stmt(
            &self.db,
            "SELECT COUNT(*) AS n FROM projects WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(project_id)],
        )?
        .first::<f64>(Some("n"))
        .await
        .map_err(err)?;
        Ok(n.unwrap_or(0.0) > 0.0)
    }
}
