//! D1 implementation of `FileRepository` (migration 0012).

use super::{changes, count, err, int, stmt, text};
use directory_core::files::{FileEntry, FileRepository};
use serde::Deserialize;
use worker::D1Database;

pub struct D1FileRepository {
    db: D1Database,
}

impl D1FileRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }

    /// Every file (data export).
    pub async fn all(&self, owner_id: &str) -> Result<Vec<FileEntry>, String> {
        let sql = format!("SELECT {COLUMNS} FROM files WHERE owner_id = ?1 ORDER BY folder, name");
        rows(
            stmt(&self.db, &sql, &[text(owner_id)])?
                .all()
                .await
                .map_err(err)?,
        )
    }
}

const COLUMNS: &str = "id, owner_id, folder, name, size, content_type, created_at, updated_at";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    folder: String,
    name: String,
    size: f64,
    content_type: String,
    created_at: f64,
    updated_at: f64,
}

impl From<Row> for FileEntry {
    fn from(r: Row) -> Self {
        FileEntry {
            id: r.id,
            owner_id: r.owner_id,
            folder: r.folder,
            name: r.name,
            size: r.size as u64,
            content_type: r.content_type,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        }
    }
}

fn rows(result: worker::D1Result) -> Result<Vec<FileEntry>, String> {
    Ok(result
        .results::<Row>()
        .map_err(err)?
        .into_iter()
        .map(FileEntry::from)
        .collect())
}

#[derive(Deserialize)]
struct FolderRow {
    folder: String,
}

/// `true` for D1 errors caused by the (owner, folder, name) unique constraint.
fn is_unique_violation(e: &worker::Error) -> bool {
    e.to_string().contains("UNIQUE")
}

impl FileRepository for D1FileRepository {
    async fn list(
        &self,
        owner_id: &str,
        folder: &str,
        limit: u32,
    ) -> Result<(Vec<FileEntry>, Vec<String>), String> {
        let sql = format!("SELECT {COLUMNS} FROM files WHERE owner_id = ?1 AND folder = ?2 ORDER BY name LIMIT ?3");
        let files = rows(
            stmt(
                &self.db,
                &sql,
                &[text(owner_id), text(folder), int(limit.into())],
            )?
            .all()
            .await
            .map_err(err)?,
        )?;
        // Folder paths at or below `folder`, compared by prefix (no LIKE escaping needed).
        let folders = stmt(
            &self.db,
            "SELECT DISTINCT folder FROM files WHERE owner_id = ?1
               AND (?2 = '' OR folder = ?2 OR substr(folder, 1, length(?2) + 1) = ?2 || '/')",
            &[text(owner_id), text(folder)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<FolderRow>()
        .map_err(err)?
        .into_iter()
        .map(|r| r.folder)
        .collect();
        Ok((files, folders))
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<FileEntry>, String> {
        let sql = format!("SELECT {COLUMNS} FROM files WHERE owner_id = ?1 AND id = ?2");
        Ok(stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(FileEntry::from))
    }

    async fn exists(&self, owner_id: &str, folder: &str, name: &str) -> Result<bool, String> {
        let n = stmt(
            &self.db,
            "SELECT COUNT(*) AS n FROM files WHERE owner_id = ?1 AND folder = ?2 AND name = ?3",
            &[text(owner_id), text(folder), text(name)],
        )?
        .first::<f64>(Some("n"))
        .await
        .map_err(err)?;
        Ok(n.unwrap_or(0.0) > 0.0)
    }

    async fn count_in_folder(&self, owner_id: &str, folder: &str) -> Result<u32, String> {
        count(
            &self.db,
            "SELECT COUNT(*) AS n FROM files WHERE owner_id = ?1 AND folder = ?2",
            &[text(owner_id), text(folder)],
        )
        .await
    }

    async fn insert(&self, f: &FileEntry) -> Result<bool, String> {
        let sql = format!(
            "INSERT INTO files ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (owner_id, folder, name) DO NOTHING"
        );
        let r = stmt(
            &self.db,
            &sql,
            &[
                text(&f.id),
                text(&f.owner_id),
                text(&f.folder),
                text(&f.name),
                int(f.size as i64),
                text(&f.content_type),
                int(f.created_at),
                int(f.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn update(&self, f: &FileEntry) -> Result<bool, String> {
        let result = stmt(
            &self.db,
            "UPDATE files SET folder = ?3, name = ?4, updated_at = ?5 WHERE owner_id = ?1 AND id = ?2",
            &[text(&f.owner_id), text(&f.id), text(&f.folder), text(&f.name), int(f.updated_at)],
        )?
        .run()
        .await;
        match result {
            Ok(_) => Ok(true),
            Err(e) if is_unique_violation(&e) => Ok(false),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM files WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn usage(&self, owner_id: &str) -> Result<u64, String> {
        let n = stmt(
            &self.db,
            "SELECT COALESCE(SUM(size), 0) AS n FROM files WHERE owner_id = ?1",
            &[text(owner_id)],
        )?
        .first::<f64>(Some("n"))
        .await
        .map_err(err)?;
        Ok(n.unwrap_or(0.0) as u64)
    }

    async fn record_orphan(&self, owner_id: &str, key: &str, at: i64) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO blob_orphans (owner_id, key, at) VALUES (?1, ?2, ?3)
             ON CONFLICT (owner_id, key) DO NOTHING",
            &[text(owner_id), text(key), int(at)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn orphans(&self, owner_id: &str, limit: u32) -> Result<Vec<String>, String> {
        #[derive(Deserialize)]
        struct KeyRow {
            key: String,
        }
        Ok(stmt(
            &self.db,
            "SELECT key FROM blob_orphans WHERE owner_id = ?1 ORDER BY at LIMIT ?2",
            &[text(owner_id), int(limit.into())],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<KeyRow>()
        .map_err(err)?
        .into_iter()
        .map(|r| r.key)
        .collect())
    }

    async fn forget_orphan(&self, owner_id: &str, key: &str) -> Result<(), String> {
        stmt(
            &self.db,
            "DELETE FROM blob_orphans WHERE owner_id = ?1 AND key = ?2",
            &[text(owner_id), text(key)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }
}
