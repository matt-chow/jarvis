//! D1 implementation of `InboxRepository` (migration 0008).

use super::{changes, err, int, stmt, text};
use directory_core::inbox::{InboxItem, InboxRepository};
use serde::Deserialize;
use worker::D1Database;

pub struct D1InboxRepository {
    db: D1Database,
}

impl D1InboxRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    text: String,
    created_at: f64,
}

impl From<Row> for InboxItem {
    fn from(r: Row) -> Self {
        InboxItem {
            id: r.id,
            owner_id: r.owner_id,
            text: r.text,
            created_at: r.created_at as i64,
        }
    }
}

impl InboxRepository for D1InboxRepository {
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<InboxItem>, String> {
        Ok(stmt(
            &self.db,
            "SELECT id, owner_id, text, created_at FROM inbox_items WHERE owner_id = ?1 ORDER BY created_at LIMIT ?2",
            &[text(owner_id), int(limit.into())],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<Row>()
        .map_err(err)?
        .into_iter()
        .map(InboxItem::from)
        .collect())
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<InboxItem>, String> {
        Ok(stmt(
            &self.db,
            "SELECT id, owner_id, text, created_at FROM inbox_items WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .first::<Row>(None)
        .await
        .map_err(err)?
        .map(InboxItem::from))
    }

    async fn insert(&self, i: &InboxItem) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO inbox_items (id, owner_id, text, created_at) VALUES (?1, ?2, ?3, ?4)",
            &[
                text(&i.id),
                text(&i.owner_id),
                text(&i.text),
                int(i.created_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM inbox_items WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }
}
