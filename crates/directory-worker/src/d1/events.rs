//! D1 implementation of `EventRepository` (migrations 0015, 0023).

use super::{changes, count, err, int, opt_int, opt_text, stmt, text};
use directory_core::calendar::{Event, EventRepository, Repeat};
use serde::Deserialize;
use worker::wasm_bindgen::JsValue;
use worker::{D1Database, D1PreparedStatement};

pub struct D1EventRepository {
    db: D1Database,
}

impl D1EventRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, owner_id, title, start_date, end_date, start_time, end_time, location, notes, repeat, repeat_until, created_at, updated_at, fixed, timezone, repeat_interval, repeat_count, skip_dates";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    title: String,
    start_date: String,
    end_date: String,
    start_time: Option<String>,
    end_time: Option<String>,
    location: String,
    notes: String,
    repeat: String,
    repeat_until: Option<String>,
    created_at: f64,
    updated_at: f64,
    fixed: f64,
    timezone: Option<String>,
    repeat_interval: f64,
    repeat_count: Option<f64>,
    skip_dates: String,
}

impl TryFrom<Row> for Event {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        Ok(Event {
            repeat: Repeat::parse(&r.repeat).ok_or("corrupt event repeat")?,
            skip_dates: serde_json::from_str(&r.skip_dates).map_err(|e| e.to_string())?,
            id: r.id,
            owner_id: r.owner_id,
            title: r.title,
            start_date: r.start_date,
            end_date: r.end_date,
            start_time: r.start_time,
            end_time: r.end_time,
            location: r.location,
            notes: r.notes,
            repeat_until: r.repeat_until,
            fixed: r.fixed != 0.0,
            timezone: r.timezone,
            repeat_interval: r.repeat_interval as u32,
            repeat_count: r.repeat_count.map(|n| n as u32),
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        })
    }
}

fn skips(e: &Event) -> JsValue {
    text(&serde_json::to_string(&e.skip_dates).unwrap_or_else(|_| "[]".into()))
}

impl D1EventRepository {
    fn insert_stmt(&self, e: &Event) -> Result<D1PreparedStatement, String> {
        let sql = format!(
            "INSERT INTO events ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)"
        );
        stmt(
            &self.db,
            &sql,
            &[
                text(&e.id),
                text(&e.owner_id),
                text(&e.title),
                text(&e.start_date),
                text(&e.end_date),
                opt_text(e.start_time.as_deref()),
                opt_text(e.end_time.as_deref()),
                text(&e.location),
                text(&e.notes),
                text(e.repeat.as_str()),
                opt_text(e.repeat_until.as_deref()),
                int(e.created_at),
                int(e.updated_at),
                int(i64::from(e.fixed)),
                opt_text(e.timezone.as_deref()),
                int(e.repeat_interval.into()),
                opt_int(e.repeat_count.map(i64::from)),
                skips(e),
            ],
        )
    }

    fn update_stmt(&self, e: &Event) -> Result<D1PreparedStatement, String> {
        stmt(
            &self.db,
            "UPDATE events SET title = ?3, start_date = ?4, end_date = ?5, start_time = ?6, end_time = ?7,
                    location = ?8, notes = ?9, repeat = ?10, repeat_until = ?11, updated_at = ?12,
                    fixed = ?13, timezone = ?14, repeat_interval = ?15, repeat_count = ?16,
                    skip_dates = ?17
              WHERE owner_id = ?1 AND id = ?2",
            &[
                text(&e.owner_id),
                text(&e.id),
                text(&e.title),
                text(&e.start_date),
                text(&e.end_date),
                opt_text(e.start_time.as_deref()),
                opt_text(e.end_time.as_deref()),
                text(&e.location),
                text(&e.notes),
                text(e.repeat.as_str()),
                opt_text(e.repeat_until.as_deref()),
                int(e.updated_at),
                int(i64::from(e.fixed)),
                opt_text(e.timezone.as_deref()),
                int(e.repeat_interval.into()),
                opt_int(e.repeat_count.map(i64::from)),
                skips(e),
            ],
        )
    }
}

impl EventRepository for D1EventRepository {
    async fn count(&self, owner_id: &str) -> Result<u32, String> {
        count(
            &self.db,
            "SELECT COUNT(*) AS n FROM events WHERE owner_id = ?1",
            &[text(owner_id)],
        )
        .await
    }

    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Event>, String> {
        let sql = format!(
            "SELECT {COLUMNS} FROM events WHERE owner_id = ?1 ORDER BY start_date, start_time LIMIT ?2"
        );
        stmt(&self.db, &sql, &[text(owner_id), int(limit.into())])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(Event::try_from)
            .collect()
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Event>, String> {
        let sql = format!("SELECT {COLUMNS} FROM events WHERE owner_id = ?1 AND id = ?2");
        stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(Event::try_from)
            .transpose()
    }

    async fn insert(&self, e: &Event) -> Result<(), String> {
        self.insert_stmt(e)?.run().await.map_err(err)?;
        Ok(())
    }

    async fn update(&self, e: &Event) -> Result<bool, String> {
        let r = self.update_stmt(e)?.run().await.map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM events WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn split(&self, series: &Event, replacement: Option<&Event>) -> Result<bool, String> {
        // One batch is one transaction: both writes land, or neither.
        let mut batch = vec![self.update_stmt(series)?];
        if let Some(r) = replacement {
            batch.push(self.insert_stmt(r)?);
        }
        let results = self.db.batch(batch).await.map_err(err)?;
        Ok(results.first().is_some_and(|r| changes(r) > 0))
    }
}
