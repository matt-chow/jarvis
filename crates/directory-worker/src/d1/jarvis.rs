//! D1 implementation of `JarvisRepository` (migration 0010).

use super::{changes, err, int, opt_text, stmt, text};
use directory_core::jarvis::conversation::{Conversation, ConversationRepository};
use directory_core::jarvis::{ActionResult, JarvisRepository, Plan, PlanStatus};
use serde::Deserialize;
use worker::D1Database;

pub struct D1JarvisRepository {
    db: D1Database,
}

impl D1JarvisRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str =
    "id, owner_id, input, actions, status, requires_approval, results, created_at, expires_at, reversed_by, revises";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    input: String,
    actions: String,
    status: String,
    requires_approval: f64,
    results: String,
    created_at: f64,
    expires_at: f64,
    reversed_by: Option<String>,
    revises: Option<String>,
}

impl TryFrom<Row> for Plan {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        Ok(Plan {
            actions: serde_json::from_str(&r.actions)
                .map_err(|e| format!("corrupt plan actions: {e}"))?,
            results: serde_json::from_str(&r.results)
                .map_err(|e| format!("corrupt plan results: {e}"))?,
            status: PlanStatus::parse(&r.status).ok_or("corrupt plan status")?,
            id: r.id,
            owner_id: r.owner_id,
            input: r.input,
            requires_approval: r.requires_approval != 0.0,
            created_at: r.created_at as i64,
            expires_at: r.expires_at as i64,
            reversed_by: r.reversed_by,
            revises: r.revises,
        })
    }
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}

impl JarvisRepository for D1JarvisRepository {
    async fn insert(&self, p: &Plan) -> Result<(), String> {
        let sql = format!(
            "INSERT INTO jarvis_plans ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
        );
        stmt(
            &self.db,
            &sql,
            &[
                text(&p.id),
                text(&p.owner_id),
                text(&p.input),
                text(&to_json(&p.actions)?),
                text(p.status.as_str()),
                int(p.requires_approval.into()),
                text(&to_json(&p.results)?),
                int(p.created_at),
                int(p.expires_at),
                opt_text(p.reversed_by.as_deref()),
                opt_text(p.revises.as_deref()),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Plan>, String> {
        let sql = format!("SELECT {COLUMNS} FROM jarvis_plans WHERE owner_id = ?1 AND id = ?2");
        stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(Plan::try_from)
            .transpose()
    }

    async fn claim(&self, owner_id: &str, id: &str, now: i64) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE jarvis_plans SET status = 'running'
              WHERE owner_id = ?1 AND id = ?2 AND status = 'pending' AND expires_at > ?3",
            &[text(owner_id), text(id), int(now)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn finish(
        &self,
        owner_id: &str,
        id: &str,
        status: PlanStatus,
        results: &[ActionResult],
    ) -> Result<(), String> {
        stmt(
            &self.db,
            "UPDATE jarvis_plans SET status = ?3, results = ?4 WHERE owner_id = ?1 AND id = ?2",
            &[
                text(owner_id),
                text(id),
                text(status.as_str()),
                text(&to_json(&results)?),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn cancel(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE jarvis_plans SET status = 'cancelled' WHERE owner_id = ?1 AND id = ?2 AND status = 'pending'",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn mark_reversed(&self, owner_id: &str, id: &str, by: &str) -> Result<(), String> {
        stmt(
            &self.db,
            "UPDATE jarvis_plans SET reversed_by = ?3 WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id), text(by)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn history(&self, owner_id: &str, limit: u32) -> Result<Vec<Plan>, String> {
        let sql = format!("SELECT {COLUMNS} FROM jarvis_plans WHERE owner_id = ?1 ORDER BY created_at DESC LIMIT ?2");
        stmt(&self.db, &sql, &[text(owner_id), int(limit.into())])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(Plan::try_from)
            .collect()
    }
}

/// Conversation state (migration 0024): one JSON row per conversation.
pub struct D1ConversationRepository {
    db: D1Database,
}

impl D1ConversationRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

#[derive(Deserialize)]
struct ConversationRow {
    id: String,
    owner_id: String,
    state: String,
    updated_at: f64,
    expires_at: f64,
}

impl ConversationRepository for D1ConversationRepository {
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Conversation>, String> {
        let row = stmt(
            &self.db,
            "SELECT id, owner_id, state, updated_at, expires_at FROM jarvis_conversations
              WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .first::<ConversationRow>(None)
        .await
        .map_err(err)?;
        let Some(r) = row else { return Ok(None) };
        // A row that no longer parses (older shape) starts afresh.
        let state = serde_json::from_str(&r.state).unwrap_or_default();
        Ok(Some(Conversation {
            id: r.id,
            owner_id: r.owner_id,
            state,
            updated_at: r.updated_at as i64,
            expires_at: r.expires_at as i64,
        }))
    }

    async fn put(&self, c: &Conversation) -> Result<(), String> {
        // The update only ever touches the owner's own row.
        stmt(
            &self.db,
            "INSERT INTO jarvis_conversations (id, owner_id, state, updated_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (id) DO UPDATE SET state = excluded.state,
                 updated_at = excluded.updated_at, expires_at = excluded.expires_at
              WHERE jarvis_conversations.owner_id = excluded.owner_id",
            &[
                text(&c.id),
                text(&c.owner_id),
                text(&to_json(&c.state)?),
                int(c.updated_at),
                int(c.expires_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        // Expired conversations aren't kept.
        stmt(
            &self.db,
            "DELETE FROM jarvis_conversations WHERE owner_id = ?1 AND expires_at < ?2",
            &[text(&c.owner_id), int(c.updated_at)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }
}
