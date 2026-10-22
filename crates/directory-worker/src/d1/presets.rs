//! D1 implementation of `PresetRepository` (migration 0011).

use super::{changes, err, int, stmt, text};
use directory_core::jarvis::presets::{Preset, PresetRepository};
use serde::Deserialize;
use worker::D1Database;

pub struct D1PresetRepository {
    db: D1Database,
}

impl D1PresetRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    name: String,
    input: String,
    created_at: f64,
}

impl PresetRepository for D1PresetRepository {
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Preset>, String> {
        Ok(stmt(
            &self.db,
            "SELECT id, owner_id, name, input, created_at FROM jarvis_presets WHERE owner_id = ?1 ORDER BY created_at LIMIT ?2",
            &[text(owner_id), int(limit.into())],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<Row>()
        .map_err(err)?
        .into_iter()
        .map(|r| Preset { id: r.id, owner_id: r.owner_id, name: r.name, input: r.input, created_at: r.created_at as i64 })
        .collect())
    }

    async fn count(&self, owner_id: &str) -> Result<u32, String> {
        let n = stmt(
            &self.db,
            "SELECT COUNT(*) AS n FROM jarvis_presets WHERE owner_id = ?1",
            &[text(owner_id)],
        )?
        .first::<f64>(Some("n"))
        .await
        .map_err(err)?;
        Ok(n.unwrap_or(0.0) as u32)
    }

    async fn insert(&self, p: &Preset) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO jarvis_presets (id, owner_id, name, input, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            &[text(&p.id), text(&p.owner_id), text(&p.name), text(&p.input), int(p.created_at)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM jarvis_presets WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }
}
