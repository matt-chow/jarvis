//! D1 implementation of `EventRepository` (migration 0015).

use super::{changes, err, int, opt_text, stmt, text};
use directory_core::calendar::{Event, EventRepository, Repeat};
use serde::Deserialize;
use worker::D1Database;

pub struct D1EventRepository {
    db: D1Database,
}

impl D1EventRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, owner_id, title, start_date, end_date, start_time, end_time, location, notes, repeat, repeat_until, created_at, updated_at";

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
}

impl TryFrom<Row> for Event {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        Ok(Event {
            repeat: Repeat::parse(&r.repeat).ok_or("corrupt event repeat")?,
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
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        })
    }
}

impl EventRepository for D1EventRepository {
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
        let sql = format!(
            "INSERT INTO events ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"
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
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn update(&self, e: &Event) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE events SET title = ?3, start_date = ?4, end_date = ?5, start_time = ?6, end_time = ?7,
                    location = ?8, notes = ?9, repeat = ?10, repeat_until = ?11, updated_at = ?12
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
            "DELETE FROM events WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }
}
