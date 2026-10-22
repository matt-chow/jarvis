//! D1 implementation of `ProjectRepository` (migration 0005).

use super::{changes, err, int, stmt, text};
use directory_core::projects::{Project, ProjectRepository, ProjectStatus};
use serde::Deserialize;
use worker::D1Database;

pub struct D1ProjectRepository {
    db: D1Database,
}

impl D1ProjectRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, owner_id, name, description, status, created_at, updated_at";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    name: String,
    description: String,
    status: String,
    created_at: f64,
    updated_at: f64,
}

impl TryFrom<Row> for Project {
    type Error = String;
    fn try_from(r: Row) -> Result<Self, String> {
        Ok(Project {
            status: ProjectStatus::parse(&r.status).ok_or("corrupt project status")?,
            id: r.id,
            owner_id: r.owner_id,
            name: r.name,
            description: r.description,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        })
    }
}

impl ProjectRepository for D1ProjectRepository {
    async fn list(
        &self,
        owner_id: &str,
        include_archived: bool,
        limit: u32,
    ) -> Result<Vec<Project>, String> {
        let clause = if include_archived {
            ""
        } else {
            "AND status = 'active'"
        };
        let sql = format!("SELECT {COLUMNS} FROM projects WHERE owner_id = ?1 {clause} ORDER BY name COLLATE NOCASE LIMIT ?2");
        stmt(&self.db, &sql, &[text(owner_id), int(limit.into())])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(Project::try_from)
            .collect()
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Project>, String> {
        let sql = format!("SELECT {COLUMNS} FROM projects WHERE owner_id = ?1 AND id = ?2");
        stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(Project::try_from)
            .transpose()
    }

    async fn insert(&self, p: &Project) -> Result<(), String> {
        let sql = format!("INSERT INTO projects ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)");
        stmt(
            &self.db,
            &sql,
            &[
                text(&p.id),
                text(&p.owner_id),
                text(&p.name),
                text(&p.description),
                text(p.status.as_str()),
                int(p.created_at),
                int(p.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn update(&self, p: &Project) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE projects SET name = ?3, description = ?4, status = ?5, updated_at = ?6
              WHERE owner_id = ?1 AND id = ?2",
            &[
                text(&p.owner_id),
                text(&p.id),
                text(&p.name),
                text(&p.description),
                text(p.status.as_str()),
                int(p.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        // Unassign tasks explicitly rather than relying on ON DELETE SET NULL.
        let results = self
            .db
            .batch(vec![
                stmt(
                    &self.db,
                    "UPDATE tasks SET project_id = NULL WHERE owner_id = ?1 AND project_id = ?2",
                    &[text(owner_id), text(id)],
                )?,
                stmt(
                    &self.db,
                    "DELETE FROM projects WHERE owner_id = ?1 AND id = ?2",
                    &[text(owner_id), text(id)],
                )?,
            ])
            .await
            .map_err(err)?;
        Ok(results.get(1).is_some_and(|r| changes(r) > 0))
    }
}
