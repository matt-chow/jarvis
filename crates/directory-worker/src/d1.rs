//! D1-backed implementations of core repositories.

use directory_core::system::SystemRepository;
use worker::D1Database;

pub struct D1SystemRepository {
    db: D1Database,
}

impl D1SystemRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

impl SystemRepository for D1SystemRepository {
    async fn schema_version(&self) -> Result<Option<u32>, String> {
        let stmt = self
            .db
            .prepare("SELECT value FROM system_meta WHERE key = ?1")
            .bind(&["schema_version".into()])
            .map_err(|e| e.to_string())?;
        match stmt.first::<String>(Some("value")).await {
            Ok(value) => value
                .map(|v| v.parse::<u32>().map_err(|e| e.to_string()))
                .transpose(),
            // A missing table means migrations have not been applied yet.
            Err(e) if e.to_string().contains("no such table") => Ok(None),
            Err(e) => {
                worker::console_error!("schema_version query failed: {e}");
                Err(e.to_string())
            }
        }
    }
}
