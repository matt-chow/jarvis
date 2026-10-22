//! D1 `SearchSource`: escaped LIKE queries per entity kind.
//! (SQLite LIKE is case-insensitive for ASCII only; see PROGRESS debt.)

use super::{err, int, stmt, text};
use directory_core::links::EntityKind;
use directory_core::notes::{NoteHead, NoteSummary};
use directory_core::search::{Candidate, SearchSource};
use serde::Deserialize;
use worker::D1Database;

pub struct D1SearchSource {
    db: D1Database,
}

impl D1SearchSource {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

#[derive(Deserialize)]
struct Row {
    id: String,
    title: String,
    body: String,
    updated_at: f64,
}

impl SearchSource for D1SearchSource {
    async fn candidates(
        &self,
        owner_id: &str,
        kind: EntityKind,
        pattern: &str,
        limit: u32,
    ) -> Result<Vec<Candidate>, String> {
        let sql = match kind {
            EntityKind::Task => {
                "SELECT id, title, notes AS body, updated_at FROM tasks
                  WHERE owner_id = ?1 AND (title LIKE ?2 ESCAPE '\\' OR notes LIKE ?2 ESCAPE '\\')
                  ORDER BY updated_at DESC LIMIT ?3"
            }
            EntityKind::Note => {
                "SELECT id, title, body, updated_at FROM notes
                  WHERE owner_id = ?1 AND (title LIKE ?2 ESCAPE '\\' OR body LIKE ?2 ESCAPE '\\')
                  ORDER BY updated_at DESC LIMIT ?3"
            }
            EntityKind::Project => {
                "SELECT id, name AS title, description AS body, updated_at FROM projects
                  WHERE owner_id = ?1 AND (name LIKE ?2 ESCAPE '\\' OR description LIKE ?2 ESCAPE '\\')
                  ORDER BY updated_at DESC LIMIT ?3"
            }
            EntityKind::Journal => {
                "SELECT date AS id, date AS title, body, updated_at FROM journal_entries
                  WHERE owner_id = ?1 AND body LIKE ?2 ESCAPE '\\'
                  ORDER BY date DESC LIMIT ?3"
            }
        };
        let rows = stmt(
            &self.db,
            sql,
            &[text(owner_id), text(pattern), int(limit.into())],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<Row>()
        .map_err(err)?;
        Ok(rows
            .into_iter()
            .map(|r| {
                // Notes without a title use the same derived title as the notes list.
                let title = if kind == EntityKind::Note {
                    NoteSummary::from(NoteHead {
                        id: r.id.clone(),
                        title: r.title,
                        body_head: r.body.chars().take(400).collect(),
                        pinned: false,
                        created_at: 0,
                        updated_at: 0,
                    })
                    .title
                } else {
                    r.title
                };
                Candidate {
                    kind,
                    id: r.id,
                    title,
                    body: r.body,
                    updated_at: r.updated_at as i64,
                }
            })
            .collect())
    }
}
