//! D1 implementation of `CategoryRepository` (migration 0025).

use super::{changes, err, int, opt_text, stmt, text};
use directory_core::categories::{Assignment, Category, CategoryRepository};
use serde::Deserialize;
use worker::D1Database;

pub struct D1CategoryRepository {
    db: D1Database,
}

impl D1CategoryRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, owner_id, name, color, icon, created_at, updated_at";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    name: String,
    color: String,
    icon: Option<String>,
    created_at: f64,
    updated_at: f64,
}

impl From<Row> for Category {
    fn from(r: Row) -> Self {
        Category {
            id: r.id,
            owner_id: r.owner_id,
            name: r.name,
            color: r.color,
            icon: r.icon,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        }
    }
}

#[derive(Deserialize)]
struct AssignmentRow {
    task_id: String,
    category_id: String,
}

impl CategoryRepository for D1CategoryRepository {
    async fn list(&self, owner_id: &str) -> Result<Vec<Category>, String> {
        let sql = format!(
            "SELECT {COLUMNS} FROM categories WHERE owner_id = ?1 ORDER BY name COLLATE NOCASE"
        );
        Ok(stmt(&self.db, &sql, &[text(owner_id)])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(Category::from)
            .collect())
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Category>, String> {
        let sql = format!("SELECT {COLUMNS} FROM categories WHERE owner_id = ?1 AND id = ?2");
        Ok(stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(Category::from))
    }

    async fn insert(&self, c: &Category) -> Result<(), String> {
        let sql = format!("INSERT INTO categories ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)");
        stmt(
            &self.db,
            &sql,
            &[
                text(&c.id),
                text(&c.owner_id),
                text(&c.name),
                text(&c.color),
                opt_text(c.icon.as_deref()),
                int(c.created_at),
                int(c.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn update(&self, c: &Category) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE categories SET name = ?3, color = ?4, icon = ?5, updated_at = ?6
              WHERE owner_id = ?1 AND id = ?2",
            &[
                text(&c.owner_id),
                text(&c.id),
                text(&c.name),
                text(&c.color),
                opt_text(c.icon.as_deref()),
                int(c.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        // Assignments go explicitly, in the same batch (no reliance on cascades).
        let results = self
            .db
            .batch(vec![
                stmt(
                    &self.db,
                    "DELETE FROM task_categories WHERE owner_id = ?1 AND category_id = ?2",
                    &[text(owner_id), text(id)],
                )?,
                stmt(
                    &self.db,
                    "UPDATE reminders SET version = version + 1 WHERE owner_id = ?1 AND id IN
                        (SELECT reminder_id FROM reminder_categories WHERE owner_id = ?1 AND category_id = ?2)",
                    &[text(owner_id), text(id)],
                )?,
                stmt(
                    &self.db,
                    "DELETE FROM reminder_categories WHERE owner_id = ?1 AND category_id = ?2",
                    &[text(owner_id), text(id)],
                )?,
                stmt(
                    &self.db,
                    "DELETE FROM categories WHERE owner_id = ?1 AND id = ?2",
                    &[text(owner_id), text(id)],
                )?,
            ])
            .await
            .map_err(err)?;
        Ok(results.get(3).is_some_and(|r| changes(r) > 0))
    }

    async fn task_assignments(&self, owner_id: &str) -> Result<Vec<Assignment>, String> {
        Ok(stmt(
            &self.db,
            "SELECT task_id, category_id FROM task_categories
              WHERE owner_id = ?1 ORDER BY task_id, position",
            &[text(owner_id)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<AssignmentRow>()
        .map_err(err)?
        .into_iter()
        .map(|r| Assignment {
            task_id: r.task_id,
            category_id: r.category_id,
        })
        .collect())
    }

    async fn task_exists(&self, owner_id: &str, task_id: &str) -> Result<bool, String> {
        Ok(super::count(
            &self.db,
            "SELECT COUNT(*) AS n FROM tasks WHERE owner_id = ?1 AND id = ?2",
            &[text(owner_id), text(task_id)],
        )
        .await?
            > 0)
    }

    async fn set_task_categories(
        &self,
        owner_id: &str,
        task_id: &str,
        ids: &[String],
        now: i64,
    ) -> Result<(), String> {
        let mut batch = vec![stmt(
            &self.db,
            "DELETE FROM task_categories WHERE owner_id = ?1 AND task_id = ?2",
            &[text(owner_id), text(task_id)],
        )?];
        for (i, id) in ids.iter().enumerate() {
            // The owner check is in the SELECT: a foreign category id inserts nothing.
            batch.push(stmt(
                &self.db,
                "INSERT INTO task_categories (task_id, category_id, owner_id, position, created_at)
                 SELECT ?2, id, ?1, ?4, ?5 FROM categories WHERE owner_id = ?1 AND id = ?3",
                &[
                    text(owner_id),
                    text(task_id),
                    text(id),
                    int(i as i64),
                    int(now),
                ],
            )?);
        }
        self.db.batch(batch).await.map_err(err)?;
        Ok(())
    }
}
