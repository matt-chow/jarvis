//! D1 implementation of `PreferencesRepository` (migration 0016).

use super::{err, int, stmt, text};
use directory_core::preferences::PreferencesRepository;
use worker::D1Database;

pub struct D1PreferencesRepository {
    db: D1Database,
}

impl D1PreferencesRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

impl PreferencesRepository for D1PreferencesRepository {
    async fn get(&self, owner_id: &str) -> Result<Option<String>, String> {
        stmt(
            &self.db,
            "SELECT data FROM preferences WHERE owner_id = ?1",
            &[text(owner_id)],
        )?
        .first::<String>(Some("data"))
        .await
        .map_err(err)
    }

    async fn put(&self, owner_id: &str, data: &str, now: i64) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO preferences (owner_id, data, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (owner_id) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
            &[text(owner_id), text(data), int(now)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }
}
