//! D1 implementation of `AutomationRepository` (migration 0017).

use super::{changes, count, err, int, opt_int, opt_text, stmt, text};
use directory_core::automations::{AutomationRepository, Rule, Trigger};
use serde::Deserialize;
use worker::D1Database;

pub struct D1AutomationRepository {
    db: D1Database,
}

impl D1AutomationRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, owner_id, name, trigger_json, command, enabled, created_at, updated_at, last_run_at, last_ok, last_message";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    name: String,
    trigger_json: String,
    command: String,
    enabled: f64,
    created_at: f64,
    updated_at: f64,
    last_run_at: Option<f64>,
    last_ok: Option<f64>,
    last_message: Option<String>,
}

impl TryFrom<Row> for Rule {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        let trigger: Trigger =
            serde_json::from_str(&r.trigger_json).map_err(|e| format!("corrupt trigger: {e}"))?;
        Ok(Rule {
            id: r.id,
            owner_id: r.owner_id,
            name: r.name,
            trigger,
            command: r.command,
            enabled: r.enabled != 0.0,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
            last_run_at: r.last_run_at.map(|t| t as i64),
            last_ok: r.last_ok.map(|v| v != 0.0),
            last_message: r.last_message,
        })
    }
}

fn trigger_json(t: &Trigger) -> Result<String, String> {
    serde_json::to_string(t).map_err(|e| e.to_string())
}

impl AutomationRepository for D1AutomationRepository {
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Rule>, String> {
        let sql = format!(
            "SELECT {COLUMNS} FROM automations WHERE owner_id = ?1 ORDER BY created_at LIMIT ?2"
        );
        stmt(&self.db, &sql, &[text(owner_id), int(limit.into())])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(Rule::try_from)
            .collect()
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Rule>, String> {
        let sql = format!("SELECT {COLUMNS} FROM automations WHERE owner_id = ?1 AND id = ?2");
        stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(Rule::try_from)
            .transpose()
    }

    async fn insert(&self, r: &Rule) -> Result<(), String> {
        let sql = format!(
            "INSERT INTO automations ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
        );
        stmt(
            &self.db,
            &sql,
            &[
                text(&r.id),
                text(&r.owner_id),
                text(&r.name),
                text(&trigger_json(&r.trigger)?),
                text(&r.command),
                int(r.enabled.into()),
                int(r.created_at),
                int(r.updated_at),
                opt_int(r.last_run_at),
                opt_int(r.last_ok.map(i64::from)),
                opt_text(r.last_message.as_deref()),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn update(&self, r: &Rule) -> Result<bool, String> {
        let res = stmt(
            &self.db,
            "UPDATE automations SET name = ?3, trigger_json = ?4, command = ?5, enabled = ?6, updated_at = ?7
              WHERE owner_id = ?1 AND id = ?2",
            &[
                text(&r.owner_id),
                text(&r.id),
                text(&r.name),
                text(&trigger_json(&r.trigger)?),
                text(&r.command),
                int(r.enabled.into()),
                int(r.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&res) > 0)
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let res = stmt(
            &self.db,
            "DELETE FROM automations WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&res) > 0)
    }

    async fn count(&self, owner_id: &str) -> Result<u32, String> {
        count(
            &self.db,
            "SELECT COUNT(*) AS n FROM automations WHERE owner_id = ?1",
            &[text(owner_id)],
        )
        .await
    }

    async fn record_run(
        &self,
        owner_id: &str,
        id: &str,
        at: i64,
        ok: bool,
        message: &str,
    ) -> Result<(), String> {
        stmt(
            &self.db,
            "UPDATE automations SET last_run_at = ?3, last_ok = ?4, last_message = ?5 WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id), int(at), int(ok.into()), text(message)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }
}
