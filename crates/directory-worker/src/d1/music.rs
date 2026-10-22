//! D1 implementation of `MusicRepository` (migration 0018; audio comes from files).

use super::{changes, count, err, int, stmt, text, D1FileRepository};
use directory_core::files::FileEntry;
use directory_core::music::{MusicRepository, Playlist};
use serde::Deserialize;
use worker::D1Database;

pub struct D1MusicRepository {
    db: D1Database,
    files: D1FileRepository,
}

impl D1MusicRepository {
    pub fn new(db: D1Database, files: D1FileRepository) -> Self {
        Self { db, files }
    }
}

const COLUMNS: &str = "id, owner_id, name, tracks_json, created_at, updated_at";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    name: String,
    tracks_json: String,
    created_at: f64,
    updated_at: f64,
}

impl TryFrom<Row> for Playlist {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        Ok(Playlist {
            tracks: serde_json::from_str(&r.tracks_json)
                .map_err(|e| format!("corrupt playlist: {e}"))?,
            id: r.id,
            owner_id: r.owner_id,
            name: r.name,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        })
    }
}

fn tracks_json(p: &Playlist) -> Result<String, String> {
    serde_json::to_string(&p.tracks).map_err(|e| e.to_string())
}

impl MusicRepository for D1MusicRepository {
    async fn files(&self, owner_id: &str) -> Result<Vec<FileEntry>, String> {
        self.files.all(owner_id).await
    }

    async fn playlists(&self, owner_id: &str, limit: u32) -> Result<Vec<Playlist>, String> {
        let sql = format!(
            "SELECT {COLUMNS} FROM playlists WHERE owner_id = ?1 ORDER BY name COLLATE NOCASE LIMIT ?2"
        );
        stmt(&self.db, &sql, &[text(owner_id), int(limit.into())])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(Playlist::try_from)
            .collect()
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Playlist>, String> {
        let sql = format!("SELECT {COLUMNS} FROM playlists WHERE owner_id = ?1 AND id = ?2");
        stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(Playlist::try_from)
            .transpose()
    }

    async fn insert(&self, p: &Playlist) -> Result<(), String> {
        let sql = format!("INSERT INTO playlists ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)");
        stmt(
            &self.db,
            &sql,
            &[
                text(&p.id),
                text(&p.owner_id),
                text(&p.name),
                text(&tracks_json(p)?),
                int(p.created_at),
                int(p.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn update(&self, p: &Playlist) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE playlists SET name = ?3, tracks_json = ?4, updated_at = ?5 WHERE owner_id = ?1 AND id = ?2",
            &[
                text(&p.owner_id),
                text(&p.id),
                text(&p.name),
                text(&tracks_json(p)?),
                int(p.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM playlists WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn count(&self, owner_id: &str) -> Result<u32, String> {
        count(
            &self.db,
            "SELECT COUNT(*) AS n FROM playlists WHERE owner_id = ?1",
            &[text(owner_id)],
        )
        .await
    }
}
