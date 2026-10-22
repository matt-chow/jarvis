//! D1 implementation of `LinkRepository` (migration 0009).

use super::{changes, err, int, stmt, text};
use directory_core::links::{EntityKind, EntityRef, Link, LinkRepository};
use directory_core::notes::{NoteHead, NoteSummary};
use serde::Deserialize;
use worker::D1Database;

pub struct D1LinkRepository {
    db: D1Database,
}

impl D1LinkRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }

    /// One text column from a row keyed by `(owner_id, id)`.
    async fn single_text(
        &self,
        sql: &str,
        owner_id: &str,
        id: &str,
        col: &str,
    ) -> Result<Option<String>, String> {
        stmt(&self.db, sql, &[text(owner_id), text(id)])?
            .first::<String>(Some(col))
            .await
            .map_err(err)
    }
}

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    a_kind: String,
    a_id: String,
    b_kind: String,
    b_id: String,
    created_at: f64,
}

impl TryFrom<Row> for Link {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        let kind = |k: &str| EntityKind::parse(k).ok_or_else(|| "corrupt link kind".to_string());
        Ok(Link {
            id: r.id,
            owner_id: r.owner_id,
            a: EntityRef {
                kind: kind(&r.a_kind)?,
                id: r.a_id,
            },
            b: EntityRef {
                kind: kind(&r.b_kind)?,
                id: r.b_id,
            },
            created_at: r.created_at as i64,
        })
    }
}

#[derive(Deserialize)]
struct NoteTitleRow {
    title: String,
    body_head: String,
}

const COLUMNS: &str = "id, owner_id, a_kind, a_id, b_kind, b_id, created_at";

impl LinkRepository for D1LinkRepository {
    async fn title_of(&self, owner_id: &str, e: &EntityRef) -> Result<Option<String>, String> {
        let single =
            |sql: &'static str, col: &'static str| self.single_text(sql, owner_id, &e.id, col);
        match e.kind {
            EntityKind::Task => {
                single(
                    "SELECT title FROM tasks WHERE owner_id = ?1 AND id = ?2",
                    "title",
                )
                .await
            }
            EntityKind::Project => {
                single(
                    "SELECT name FROM projects WHERE owner_id = ?1 AND id = ?2",
                    "name",
                )
                .await
            }
            EntityKind::Journal => {
                single(
                    "SELECT date FROM journal_entries WHERE owner_id = ?1 AND date = ?2",
                    "date",
                )
                .await
            }
            EntityKind::Note => {
                // Same derivation as the notes list (explicit title, else first line).
                let row = stmt(
                    &self.db,
                    "SELECT title, substr(body, 1, 400) AS body_head FROM notes WHERE owner_id = ?1 AND id = ?2",
                    &[text(owner_id), text(&e.id)],
                )?
                .first::<NoteTitleRow>(None)
                .await
                .map_err(err)?;
                Ok(row.map(|r| {
                    NoteSummary::from(NoteHead {
                        id: e.id.clone(),
                        title: r.title,
                        body_head: r.body_head,
                        pinned: false,
                        created_at: 0,
                        updated_at: 0,
                    })
                    .title
                }))
            }
        }
    }

    async fn find(
        &self,
        owner_id: &str,
        a: &EntityRef,
        b: &EntityRef,
    ) -> Result<Option<Link>, String> {
        let sql = format!(
            "SELECT {COLUMNS} FROM links WHERE owner_id = ?1 AND a_kind = ?2 AND a_id = ?3 AND b_kind = ?4 AND b_id = ?5"
        );
        stmt(
            &self.db,
            &sql,
            &[
                text(owner_id),
                text(a.kind.as_str()),
                text(&a.id),
                text(b.kind.as_str()),
                text(&b.id),
            ],
        )?
        .first::<Row>(None)
        .await
        .map_err(err)?
        .map(Link::try_from)
        .transpose()
    }

    async fn insert(&self, l: &Link) -> Result<(), String> {
        // A concurrent identical insert is harmless: the unique pair is kept once.
        let sql = format!("INSERT INTO links ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT DO NOTHING");
        stmt(
            &self.db,
            &sql,
            &[
                text(&l.id),
                text(&l.owner_id),
                text(l.a.kind.as_str()),
                text(&l.a.id),
                text(l.b.kind.as_str()),
                text(&l.b.id),
                int(l.created_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn list_for(
        &self,
        owner_id: &str,
        e: &EntityRef,
        limit: u32,
    ) -> Result<Vec<Link>, String> {
        let sql = format!(
            "SELECT {COLUMNS} FROM links WHERE owner_id = ?1
               AND ((a_kind = ?2 AND a_id = ?3) OR (b_kind = ?2 AND b_id = ?3))
             ORDER BY created_at LIMIT ?4"
        );
        stmt(
            &self.db,
            &sql,
            &[
                text(owner_id),
                text(e.kind.as_str()),
                text(&e.id),
                int(limit.into()),
            ],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<Row>()
        .map_err(err)?
        .into_iter()
        .map(Link::try_from)
        .collect()
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM links WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }
}
