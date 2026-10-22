//! D1-backed implementations of core repositories.

mod automations;
mod events;
mod export;
mod files;
mod habits;
mod identity;
mod inbox;
mod jarvis;
mod journal;
mod library;
mod links;
mod notes;
mod preferences;
mod presets;
mod projects;
mod rate_limit;
mod search;
mod tasks;

pub use automations::D1AutomationRepository;
pub use events::D1EventRepository;
pub use export::D1ExportSource;
pub use files::D1FileRepository;
pub use habits::D1HabitRepository;
pub use identity::D1IdentityRepository;
pub use inbox::D1InboxRepository;
pub use jarvis::D1JarvisRepository;
pub use journal::D1JournalRepository;
pub use library::D1LibraryRepository;
pub use links::D1LinkRepository;
pub use notes::D1NoteRepository;
pub use preferences::D1PreferencesRepository;
pub use presets::D1PresetRepository;
pub use projects::D1ProjectRepository;
pub use rate_limit::D1RateLimitStore;
pub use search::D1SearchSource;
pub use tasks::D1TaskRepository;

use directory_core::system::SystemRepository;
use worker::wasm_bindgen::JsValue;
use worker::{D1Database, D1PreparedStatement, D1Result};

/// Bind helpers: D1 takes JS values; times are epoch-ms numbers (exact below 2^53).
pub(crate) fn text(v: &str) -> JsValue {
    JsValue::from_str(v)
}

pub(crate) fn int(v: i64) -> JsValue {
    JsValue::from_f64(v as f64)
}

pub(crate) fn opt_text(v: Option<&str>) -> JsValue {
    v.map_or(JsValue::NULL, JsValue::from_str)
}

pub(crate) fn opt_int(v: Option<i64>) -> JsValue {
    v.map_or(JsValue::NULL, int)
}

pub(crate) fn err(e: worker::Error) -> String {
    e.to_string()
}

pub(crate) fn stmt(
    db: &D1Database,
    sql: &str,
    args: &[JsValue],
) -> Result<D1PreparedStatement, String> {
    db.prepare(sql).bind(args).map_err(err)
}

/// Run a `SELECT COUNT(*) AS n …` query.
pub(crate) async fn count(db: &D1Database, sql: &str, args: &[JsValue]) -> Result<u32, String> {
    #[derive(serde::Deserialize)]
    struct N {
        n: f64,
    }
    Ok(stmt(db, sql, args)?
        .first::<N>(None)
        .await
        .map_err(err)?
        .map_or(0, |r| r.n as u32))
}

/// Rows changed by a write.
pub(crate) fn changes(result: &D1Result) -> usize {
    result
        .meta()
        .ok()
        .flatten()
        .and_then(|m| m.changes)
        .unwrap_or(0)
}

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
        let stmt = stmt(
            &self.db,
            "SELECT value FROM system_meta WHERE key = ?1",
            &[text("schema_version")],
        )?;
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
