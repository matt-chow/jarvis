//! D1 implementation of `JournalRepository` (migration 0007).

use super::{changes, err, int, opt_int, stmt, text};
use directory_core::journal::{Entry, EntryHead, JournalRepository, HEAD_CHARS};
use serde::Deserialize;
use worker::D1Database;

pub struct D1JournalRepository {
    db: D1Database,
}

impl D1JournalRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

#[derive(Deserialize)]
struct Row {
    owner_id: String,
    date: String,
    body: String,
    mood: Option<f64>,
    created_at: f64,
    updated_at: f64,
}

#[derive(Deserialize)]
struct HeadRow {
    date: String,
    body_head: String,
    mood: Option<f64>,
    updated_at: f64,
}

impl JournalRepository for D1JournalRepository {
    async fn list(&self, owner_id: &str, from: &str, to: &str) -> Result<Vec<EntryHead>, String> {
        Ok(stmt(
            &self.db,
            "SELECT date, substr(body, 1, ?4) AS body_head, mood, updated_at FROM journal_entries
              WHERE owner_id = ?1 AND date BETWEEN ?2 AND ?3 ORDER BY date DESC",
            &[text(owner_id), text(from), text(to), int(HEAD_CHARS as i64)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<HeadRow>()
        .map_err(err)?
        .into_iter()
        .map(|r| EntryHead {
            date: r.date,
            body_head: r.body_head,
            mood: r.mood.map(|m| m as u8),
            updated_at: r.updated_at as i64,
        })
        .collect())
    }

    async fn get(&self, owner_id: &str, date: &str) -> Result<Option<Entry>, String> {
        Ok(stmt(
            &self.db,
            "SELECT owner_id, date, body, mood, created_at, updated_at FROM journal_entries WHERE owner_id = ?1 AND date = ?2",
            &[text(owner_id), text(date)],
        )?
        .first::<Row>(None)
        .await
        .map_err(err)?
        .map(|r| Entry {
            owner_id: r.owner_id,
            date: r.date,
            body: r.body,
            mood: r.mood.map(|m| m as u8),
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        }))
    }

    async fn insert_if_absent(&self, e: &Entry) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "INSERT INTO journal_entries (owner_id, date, body, mood, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (owner_id, date) DO NOTHING",
            &[
                text(&e.owner_id),
                text(&e.date),
                text(&e.body),
                opt_int(e.mood.map(i64::from)),
                int(e.created_at),
                int(e.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn update_if(&self, e: &Entry, expected_updated_at: i64) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE journal_entries SET body = ?3, mood = ?4, updated_at = ?5
              WHERE owner_id = ?1 AND date = ?2 AND updated_at = ?6",
            &[
                text(&e.owner_id),
                text(&e.date),
                text(&e.body),
                opt_int(e.mood.map(i64::from)),
                int(e.updated_at),
                int(expected_updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete(&self, owner_id: &str, date: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM journal_entries WHERE owner_id = ?1 AND date = ?2",
            &[text(owner_id), text(date)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }
}
