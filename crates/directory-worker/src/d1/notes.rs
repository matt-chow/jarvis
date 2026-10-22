//! D1 implementation of `NoteRepository` (migration 0006).

use super::{changes, err, int, stmt, text};
use directory_core::notes::{Note, NoteHead, NoteRepository, HEAD_CHARS};
use serde::Deserialize;
use worker::D1Database;

pub struct D1NoteRepository {
    db: D1Database,
}

impl D1NoteRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }

    /// Every note with full body (data export).
    pub async fn all(&self, owner_id: &str) -> Result<Vec<Note>, String> {
        Ok(stmt(
            &self.db,
            "SELECT id, owner_id, title, body, pinned, created_at, updated_at FROM notes WHERE owner_id = ?1 ORDER BY created_at",
            &[text(owner_id)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<Row>()
        .map_err(err)?
        .into_iter()
        .map(Note::from)
        .collect())
    }
}

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    title: String,
    body: String,
    pinned: f64,
    created_at: f64,
    updated_at: f64,
}

impl From<Row> for Note {
    fn from(r: Row) -> Self {
        Note {
            id: r.id,
            owner_id: r.owner_id,
            title: r.title,
            body: r.body,
            pinned: r.pinned != 0.0,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        }
    }
}

#[derive(Deserialize)]
struct HeadRow {
    id: String,
    title: String,
    body_head: String,
    pinned: f64,
    created_at: f64,
    updated_at: f64,
}

impl NoteRepository for D1NoteRepository {
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<NoteHead>, String> {
        // Only the start of each body leaves the database for list views.
        Ok(stmt(
            &self.db,
            "SELECT id, title, substr(body, 1, ?3) AS body_head, pinned, created_at, updated_at
               FROM notes WHERE owner_id = ?1 ORDER BY updated_at DESC LIMIT ?2",
            &[text(owner_id), int(limit.into()), int(HEAD_CHARS as i64)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<HeadRow>()
        .map_err(err)?
        .into_iter()
        .map(|r| NoteHead {
            id: r.id,
            title: r.title,
            body_head: r.body_head,
            pinned: r.pinned != 0.0,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        })
        .collect())
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Note>, String> {
        Ok(stmt(
            &self.db,
            "SELECT id, owner_id, title, body, pinned, created_at, updated_at FROM notes WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .first::<Row>(None)
        .await
        .map_err(err)?
        .map(Note::from))
    }

    async fn insert(&self, n: &Note) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO notes (id, owner_id, title, body, pinned, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[
                text(&n.id),
                text(&n.owner_id),
                text(&n.title),
                text(&n.body),
                int(n.pinned.into()),
                int(n.created_at),
                int(n.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn update_if(&self, n: &Note, expected_updated_at: i64) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE notes SET title = ?3, body = ?4, pinned = ?5, updated_at = ?6
              WHERE owner_id = ?1 AND id = ?2 AND updated_at = ?7",
            &[
                text(&n.owner_id),
                text(&n.id),
                text(&n.title),
                text(&n.body),
                int(n.pinned.into()),
                int(n.updated_at),
                int(expected_updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn body_like(
        &self,
        owner_id: &str,
        pattern: &str,
        limit: u32,
    ) -> Result<Vec<Note>, String> {
        Ok(stmt(
            &self.db,
            "SELECT id, owner_id, title, body, pinned, created_at, updated_at FROM notes
              WHERE owner_id = ?1 AND body LIKE ?2 ESCAPE '\\'
              ORDER BY updated_at DESC LIMIT ?3",
            &[text(owner_id), text(pattern), int(limit.into())],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<Row>()
        .map_err(err)?
        .into_iter()
        .map(Note::from)
        .collect())
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM notes WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }
}
