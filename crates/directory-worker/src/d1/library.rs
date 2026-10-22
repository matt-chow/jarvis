//! D1 implementation of `LibraryRepository` (migration 0013). File rows come
//! from the files repository; this adds only the progress table.

use super::{changes, err, int, opt_int, opt_text, stmt, text, D1FileRepository};
use directory_core::files::{FileEntry, FileRepository};
use directory_core::library::{LibraryRepository, Progress, ReadingStatus};
use serde::Deserialize;
use worker::D1Database;

pub struct D1LibraryRepository {
    db: D1Database,
    files: D1FileRepository,
}

impl D1LibraryRepository {
    pub fn new(db: D1Database, files: D1FileRepository) -> Self {
        Self { db, files }
    }
}

const COLUMNS: &str =
    "owner_id, file_id, status, current_page, total_pages, position, percent, started_at, finished_at, updated_at";

#[derive(Deserialize)]
struct Row {
    owner_id: String,
    file_id: String,
    status: String,
    current_page: Option<f64>,
    total_pages: Option<f64>,
    position: Option<String>,
    percent: f64,
    started_at: Option<f64>,
    finished_at: Option<f64>,
    updated_at: f64,
}

impl TryFrom<Row> for Progress {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        Ok(Progress {
            status: ReadingStatus::parse(&r.status).ok_or("corrupt reading status")?,
            owner_id: r.owner_id,
            file_id: r.file_id,
            current_page: r.current_page.map(|v| v as u32),
            total_pages: r.total_pages.map(|v| v as u32),
            position: r.position,
            percent: r.percent as u8,
            started_at: r.started_at.map(|v| v as i64),
            finished_at: r.finished_at.map(|v| v as i64),
            updated_at: r.updated_at as i64,
        })
    }
}

impl LibraryRepository for D1LibraryRepository {
    async fn files(&self, owner_id: &str) -> Result<Vec<FileEntry>, String> {
        self.files.all(owner_id).await
    }

    async fn file(&self, owner_id: &str, file_id: &str) -> Result<Option<FileEntry>, String> {
        self.files.get(owner_id, file_id).await
    }

    async fn progress(&self, owner_id: &str) -> Result<Vec<Progress>, String> {
        let sql = format!("SELECT {COLUMNS} FROM reading_progress WHERE owner_id = ?1");
        stmt(&self.db, &sql, &[text(owner_id)])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(Progress::try_from)
            .collect()
    }

    async fn progress_for(
        &self,
        owner_id: &str,
        file_id: &str,
    ) -> Result<Option<Progress>, String> {
        let sql =
            format!("SELECT {COLUMNS} FROM reading_progress WHERE owner_id = ?1 AND file_id = ?2");
        stmt(&self.db, &sql, &[text(owner_id), text(file_id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(Progress::try_from)
            .transpose()
    }

    async fn upsert(&self, p: &Progress) -> Result<(), String> {
        let sql = format!(
            "INSERT INTO reading_progress ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (owner_id, file_id) DO UPDATE SET
               status = excluded.status, current_page = excluded.current_page,
               total_pages = excluded.total_pages, position = excluded.position,
               percent = excluded.percent, started_at = excluded.started_at,
               finished_at = excluded.finished_at, updated_at = excluded.updated_at"
        );
        stmt(
            &self.db,
            &sql,
            &[
                text(&p.owner_id),
                text(&p.file_id),
                text(p.status.as_str()),
                opt_int(p.current_page.map(i64::from)),
                opt_int(p.total_pages.map(i64::from)),
                opt_text(p.position.as_deref()),
                int(p.percent.into()),
                opt_int(p.started_at),
                opt_int(p.finished_at),
                int(p.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn delete(&self, owner_id: &str, file_id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM reading_progress WHERE owner_id = ?1 AND file_id = ?2",
            &[text(owner_id), text(file_id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }
}
