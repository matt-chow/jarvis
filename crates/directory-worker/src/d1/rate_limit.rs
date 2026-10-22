//! D1 implementation of `RateLimitStore` (migration 0003).

use super::{err, int, stmt, text};
use directory_core::rate_limit::RateLimitStore;
use serde::Deserialize;
use worker::D1Database;

pub struct D1RateLimitStore {
    db: D1Database,
}

impl D1RateLimitStore {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

#[derive(Deserialize)]
struct CountRow {
    count: f64,
}

impl RateLimitStore for D1RateLimitStore {
    async fn hit(&self, key: &str, window_start: i64, purge_before: i64) -> Result<u32, String> {
        let results = self
            .db
            .batch(vec![
                stmt(
                    &self.db,
                    "DELETE FROM rate_limits WHERE window_start < ?1",
                    &[int(purge_before)],
                )?,
                stmt(
                    &self.db,
                    "INSERT INTO rate_limits (key, window_start, count) VALUES (?1, ?2, 1)
                     ON CONFLICT (key, window_start) DO UPDATE SET count = count + 1
                     RETURNING count",
                    &[text(key), int(window_start)],
                )?,
            ])
            .await
            .map_err(err)?;
        let rows = results
            .get(1)
            .ok_or("missing upsert result")?
            .results::<CountRow>()
            .map_err(err)?;
        Ok(rows.first().map_or(1, |r| r.count as u32))
    }
}
