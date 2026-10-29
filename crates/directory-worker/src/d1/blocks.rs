//! D1 implementation of `BlockRepository` (migration 0023).

use super::{changes, count, err, int, opt_int, opt_text, stmt, text};
use directory_core::blocks::{BlockRepository, BlockStatus, TimeBlock};
use serde::Deserialize;
use worker::D1Database;

pub struct D1BlockRepository {
    db: D1Database,
}

impl D1BlockRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, owner_id, task_id, title, date, start_time, end_time, timezone, fixed, status, actual_minutes, notes, created_at, updated_at";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    task_id: Option<String>,
    title: String,
    date: String,
    start_time: String,
    end_time: String,
    timezone: Option<String>,
    fixed: f64,
    status: String,
    actual_minutes: Option<f64>,
    notes: String,
    created_at: f64,
    updated_at: f64,
}

impl TryFrom<Row> for TimeBlock {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        Ok(TimeBlock {
            status: BlockStatus::parse(&r.status).ok_or("corrupt block status")?,
            id: r.id,
            owner_id: r.owner_id,
            task_id: r.task_id,
            title: r.title,
            date: r.date,
            start_time: r.start_time,
            end_time: r.end_time,
            timezone: r.timezone,
            fixed: r.fixed != 0.0,
            actual_minutes: r.actual_minutes.map(|m| m as u32),
            notes: r.notes,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        })
    }
}

impl BlockRepository for D1BlockRepository {
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<TimeBlock>, String> {
        let sql = format!(
            "SELECT {COLUMNS} FROM time_blocks WHERE owner_id = ?1 ORDER BY date, start_time LIMIT ?2"
        );
        stmt(&self.db, &sql, &[text(owner_id), int(limit.into())])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(TimeBlock::try_from)
            .collect()
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<TimeBlock>, String> {
        let sql = format!("SELECT {COLUMNS} FROM time_blocks WHERE owner_id = ?1 AND id = ?2");
        stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(TimeBlock::try_from)
            .transpose()
    }

    async fn count(&self, owner_id: &str) -> Result<u32, String> {
        count(
            &self.db,
            "SELECT COUNT(*) AS n FROM time_blocks WHERE owner_id = ?1",
            &[text(owner_id)],
        )
        .await
    }

    async fn insert(&self, b: &TimeBlock) -> Result<(), String> {
        // The task must still be the owner's when the row is written.
        let sql = format!(
            "INSERT INTO time_blocks ({COLUMNS})
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
              WHERE ?3 IS NULL OR EXISTS (SELECT 1 FROM tasks WHERE id = ?3 AND owner_id = ?2)"
        );
        let r = stmt(
            &self.db,
            &sql,
            &[
                text(&b.id),
                text(&b.owner_id),
                opt_text(b.task_id.as_deref()),
                text(&b.title),
                text(&b.date),
                text(&b.start_time),
                text(&b.end_time),
                opt_text(b.timezone.as_deref()),
                int(i64::from(b.fixed)),
                text(b.status.as_str()),
                opt_int(b.actual_minutes.map(i64::from)),
                text(&b.notes),
                int(b.created_at),
                int(b.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        if changes(&r) == 0 {
            return Err("time block task vanished".into());
        }
        Ok(())
    }

    async fn update(&self, b: &TimeBlock) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE time_blocks SET task_id = ?3, title = ?4, date = ?5, start_time = ?6, end_time = ?7,
                    timezone = ?8, fixed = ?9, status = ?10, actual_minutes = ?11, notes = ?12,
                    updated_at = ?13
              WHERE owner_id = ?1 AND id = ?2
                AND (?3 IS NULL OR EXISTS (SELECT 1 FROM tasks WHERE id = ?3 AND owner_id = ?1))",
            &[
                text(&b.owner_id),
                text(&b.id),
                opt_text(b.task_id.as_deref()),
                text(&b.title),
                text(&b.date),
                text(&b.start_time),
                text(&b.end_time),
                opt_text(b.timezone.as_deref()),
                int(i64::from(b.fixed)),
                text(b.status.as_str()),
                opt_int(b.actual_minutes.map(i64::from)),
                text(&b.notes),
                int(b.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM time_blocks WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn task_exists(&self, owner_id: &str, task_id: &str) -> Result<bool, String> {
        count(
            &self.db,
            "SELECT COUNT(*) AS n FROM tasks WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(task_id)],
        )
        .await
        .map(|n| n > 0)
    }
}
