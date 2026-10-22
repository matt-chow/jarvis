//! D1 implementation of `HabitRepository` (migration 0014).

use super::{changes, count, err, int, stmt, text};
use directory_core::habits::{days_to_mask, mask_to_days, CheckIn, Habit, HabitRepository};
use serde::Deserialize;
use worker::D1Database;

pub struct D1HabitRepository {
    db: D1Database,
}

impl D1HabitRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const COLUMNS: &str = "id, owner_id, name, days, archived, created_at, updated_at";

#[derive(Deserialize)]
struct Row {
    id: String,
    owner_id: String,
    name: String,
    days: f64,
    archived: f64,
    created_at: f64,
    updated_at: f64,
}

impl From<Row> for Habit {
    fn from(r: Row) -> Self {
        Habit {
            id: r.id,
            owner_id: r.owner_id,
            name: r.name,
            days: mask_to_days(r.days as u8),
            archived: r.archived != 0.0,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        }
    }
}

#[derive(Deserialize)]
struct CheckInRow {
    habit_id: String,
    date: String,
}

impl HabitRepository for D1HabitRepository {
    async fn count(&self, owner_id: &str) -> Result<u32, String> {
        count(
            &self.db,
            "SELECT COUNT(*) AS n FROM habits WHERE owner_id = ?1",
            &[text(owner_id)],
        )
        .await
    }

    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Habit>, String> {
        let sql = format!(
            "SELECT {COLUMNS} FROM habits WHERE owner_id = ?1 ORDER BY created_at LIMIT ?2"
        );
        Ok(stmt(&self.db, &sql, &[text(owner_id), int(limit.into())])?
            .all()
            .await
            .map_err(err)?
            .results::<Row>()
            .map_err(err)?
            .into_iter()
            .map(Habit::from)
            .collect())
    }

    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Habit>, String> {
        let sql = format!("SELECT {COLUMNS} FROM habits WHERE owner_id = ?1 AND id = ?2");
        Ok(stmt(&self.db, &sql, &[text(owner_id), text(id)])?
            .first::<Row>(None)
            .await
            .map_err(err)?
            .map(Habit::from))
    }

    async fn insert(&self, h: &Habit) -> Result<(), String> {
        let sql = format!("INSERT INTO habits ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)");
        stmt(
            &self.db,
            &sql,
            &[
                text(&h.id),
                text(&h.owner_id),
                text(&h.name),
                int(days_to_mask(&h.days).into()),
                int(h.archived.into()),
                int(h.created_at),
                int(h.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn update(&self, h: &Habit) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "UPDATE habits SET name = ?3, days = ?4, archived = ?5, updated_at = ?6
              WHERE owner_id = ?1 AND id = ?2",
            &[
                text(&h.owner_id),
                text(&h.id),
                text(&h.name),
                int(days_to_mask(&h.days).into()),
                int(h.archived.into()),
                int(h.updated_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        // Remove check-ins explicitly rather than relying on ON DELETE CASCADE.
        let results = self
            .db
            .batch(vec![
                stmt(
                    &self.db,
                    "DELETE FROM habit_checkins WHERE owner_id = ?1 AND habit_id = ?2",
                    &[text(owner_id), text(id)],
                )?,
                stmt(
                    &self.db,
                    "DELETE FROM habits WHERE owner_id = ?1 AND id = ?2",
                    &[text(owner_id), text(id)],
                )?,
            ])
            .await
            .map_err(err)?;
        Ok(results.get(1).is_some_and(|r| changes(r) > 0))
    }

    async fn checkins(&self, owner_id: &str, from: &str, to: &str) -> Result<Vec<CheckIn>, String> {
        Ok(stmt(
            &self.db,
            "SELECT habit_id, date FROM habit_checkins
              WHERE owner_id = ?1 AND date BETWEEN ?2 AND ?3 ORDER BY date",
            &[text(owner_id), text(from), text(to)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<CheckInRow>()
        .map_err(err)?
        .into_iter()
        .map(|r| CheckIn {
            habit_id: r.habit_id,
            date: r.date,
        })
        .collect())
    }

    async fn check_in(
        &self,
        owner_id: &str,
        habit_id: &str,
        date: &str,
        now: i64,
    ) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT OR IGNORE INTO habit_checkins (habit_id, owner_id, date, created_at) VALUES (?1, ?2, ?3, ?4)",
            &[text(habit_id), text(owner_id), text(date), int(now)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn uncheck(&self, owner_id: &str, habit_id: &str, date: &str) -> Result<(), String> {
        stmt(
            &self.db,
            "DELETE FROM habit_checkins WHERE owner_id = ?1 AND habit_id = ?2 AND date = ?3",
            &[text(owner_id), text(habit_id), text(date)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }
}
