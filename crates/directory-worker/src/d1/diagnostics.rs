//! D1 implementation of `DiagnosticsRepository` (migration 0019).

use super::{err, int, stmt, text};
use directory_core::diagnostics::{ClientError, DiagnosticsRepository};
use serde::Deserialize;
use worker::D1Database;

pub struct D1DiagnosticsRepository {
    db: D1Database,
}

impl D1DiagnosticsRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, owner_id, at, module, message, stack, user_agent, app_version";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    at: f64,
    module: String,
    message: String,
    stack: String,
    user_agent: String,
    app_version: String,
}

impl DiagnosticsRepository for D1DiagnosticsRepository {
    async fn insert(&self, e: &ClientError) -> Result<(), String> {
        let sql = format!(
            "INSERT INTO client_errors ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        );
        stmt(
            &self.db,
            &sql,
            &[
                text(&e.id),
                text(&e.owner_id),
                int(e.at),
                text(&e.module),
                text(&e.message),
                text(&e.stack),
                text(&e.user_agent),
                text(&e.app_version),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn prune(&self, owner_id: &str, keep: u32) -> Result<(), String> {
        stmt(
            &self.db,
            "DELETE FROM client_errors WHERE owner_id = ?1 AND id NOT IN
               (SELECT id FROM client_errors WHERE owner_id = ?1 ORDER BY at DESC LIMIT ?2)",
            &[text(owner_id), int(keep.into())],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<ClientError>, String> {
        let sql = format!(
            "SELECT {COLUMNS} FROM client_errors WHERE owner_id = ?1 ORDER BY at DESC LIMIT ?2"
        );
        Ok(stmt(&self.db, &sql, &[text(owner_id), int(limit.into())])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(|r| ClientError {
                id: r.id,
                owner_id: r.owner_id,
                at: r.at as i64,
                module: r.module,
                message: r.message,
                stack: r.stack,
                user_agent: r.user_agent,
                app_version: r.app_version,
            })
            .collect())
    }

    async fn clear(&self, owner_id: &str) -> Result<(), String> {
        stmt(
            &self.db,
            "DELETE FROM client_errors WHERE owner_id = ?1",
            &[text(owner_id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }
}
