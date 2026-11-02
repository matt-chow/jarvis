//! In-memory reminder repository for tests.

use super::model::{Reminder, ReminderList};
use super::service::{HistoryEntry, OccurrenceChange, OccurrenceRow, ReminderRepository};
use std::cell::RefCell;

#[derive(Default)]
pub struct MemoryReminders {
    pub reminders: RefCell<Vec<Reminder>>,
    pub lists: RefCell<Vec<ReminderList>>,
    /// (owner, row)
    pub occurrences: RefCell<Vec<(String, OccurrenceRow)>>,
    pub history: RefCell<Vec<(String, HistoryEntry)>>,
    /// (owner, id) pairs that exist.
    pub categories: RefCell<Vec<(String, String)>>,
    pub tasks: RefCell<Vec<(String, String)>>,
    pub projects: RefCell<Vec<(String, String)>>,
}

impl MemoryReminders {
    fn owned(&self, owner_id: &str) -> Vec<Reminder> {
        self.reminders
            .borrow()
            .iter()
            .filter(|r| r.owner_id == owner_id)
            .cloned()
            .collect()
    }

    fn apply(
        &self,
        owner_id: &str,
        changes: &[OccurrenceChange],
        history: &[HistoryEntry],
        id: &str,
    ) {
        let mut occ = self.occurrences.borrow_mut();
        for c in changes {
            let date = match c {
                OccurrenceChange::Set(row) => &row.occurrence,
                OccurrenceChange::Remove(d) => d,
            };
            occ.retain(|(o, row)| {
                !(o == owner_id && row.reminder_id == id && row.occurrence == *date)
            });
            if let OccurrenceChange::Set(row) = c {
                occ.push((owner_id.to_owned(), row.clone()));
            }
        }
        self.history
            .borrow_mut()
            .extend(history.iter().map(|h| (owner_id.to_owned(), h.clone())));
    }
}

fn has(v: &RefCell<Vec<(String, String)>>, owner_id: &str, id: &str) -> bool {
    v.borrow().iter().any(|(o, i)| o == owner_id && i == id)
}

impl ReminderRepository for MemoryReminders {
    async fn list(&self, owner_id: &str) -> Result<Vec<Reminder>, String> {
        Ok(self.owned(owner_id))
    }
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Reminder>, String> {
        Ok(self.owned(owner_id).into_iter().find(|r| r.id == id))
    }
    async fn count(&self, owner_id: &str) -> Result<u32, String> {
        Ok(self.owned(owner_id).len() as u32)
    }
    async fn insert(&self, r: &Reminder) -> Result<(), String> {
        self.reminders.borrow_mut().push(r.clone());
        Ok(())
    }
    async fn save(
        &self,
        r: &Reminder,
        expected: i64,
        changes: &[OccurrenceChange],
        history: &[HistoryEntry],
    ) -> Result<bool, String> {
        {
            let mut all = self.reminders.borrow_mut();
            let Some(slot) = all
                .iter_mut()
                .find(|x| x.owner_id == r.owner_id && x.id == r.id && x.version == expected)
            else {
                return Ok(false);
            };
            *slot = r.clone();
        }
        self.apply(&r.owner_id, changes, history, &r.id);
        Ok(true)
    }
    async fn purge(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let before = self.reminders.borrow().len();
        self.reminders
            .borrow_mut()
            .retain(|r| !(r.owner_id == owner_id && r.id == id));
        self.occurrences
            .borrow_mut()
            .retain(|(o, row)| !(o == owner_id && row.reminder_id == id));
        self.history
            .borrow_mut()
            .retain(|(o, h)| !(o == owner_id && h.reminder_id == id));
        Ok(self.reminders.borrow().len() < before)
    }
    async fn occurrences(
        &self,
        owner_id: &str,
        reminder_id: Option<&str>,
        from: &str,
        to: &str,
    ) -> Result<Vec<OccurrenceRow>, String> {
        let mut v: Vec<OccurrenceRow> = self
            .occurrences
            .borrow()
            .iter()
            .filter(|(o, row)| {
                o == owner_id
                    && reminder_id.is_none_or(|id| row.reminder_id == id)
                    && row.occurrence.as_str() >= from
                    && row.occurrence.as_str() <= to
            })
            .map(|(_, row)| row.clone())
            .collect();
        v.sort_by(|a, b| a.occurrence.cmp(&b.occurrence));
        Ok(v)
    }
    async fn history(
        &self,
        owner_id: &str,
        id: &str,
        limit: u32,
    ) -> Result<Vec<HistoryEntry>, String> {
        let mut v: Vec<HistoryEntry> = self
            .history
            .borrow()
            .iter()
            .filter(|(o, h)| o == owner_id && h.reminder_id == id)
            .map(|(_, h)| h.clone())
            .collect();
        v.reverse();
        v.truncate(limit as usize);
        Ok(v)
    }
    async fn category_ids(&self, owner_id: &str) -> Result<Vec<String>, String> {
        Ok(self
            .categories
            .borrow()
            .iter()
            .filter(|(o, _)| o == owner_id)
            .map(|(_, i)| i.clone())
            .collect())
    }
    async fn task_exists(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        Ok(has(&self.tasks, owner_id, id))
    }
    async fn project_exists(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        Ok(has(&self.projects, owner_id, id))
    }
    async fn linked_to_task(&self, owner_id: &str, task_id: &str) -> Result<Vec<Reminder>, String> {
        Ok(self
            .owned(owner_id)
            .into_iter()
            .filter(|r| r.task_id.as_deref() == Some(task_id))
            .collect())
    }
    async fn lists(&self, owner_id: &str) -> Result<Vec<ReminderList>, String> {
        Ok(self
            .lists
            .borrow()
            .iter()
            .filter(|l| l.owner_id == owner_id)
            .cloned()
            .collect())
    }
    async fn insert_list(&self, l: &ReminderList) -> Result<(), String> {
        self.lists.borrow_mut().push(l.clone());
        Ok(())
    }
    async fn update_list(&self, l: &ReminderList) -> Result<bool, String> {
        let mut all = self.lists.borrow_mut();
        match all
            .iter_mut()
            .find(|x| x.owner_id == l.owner_id && x.id == l.id)
        {
            Some(slot) => {
                *slot = l.clone();
                Ok(true)
            }
            None => Ok(false),
        }
    }
    async fn delete_list(&self, owner_id: &str, id: &str, now: i64) -> Result<bool, String> {
        let before = self.lists.borrow().len();
        self.lists
            .borrow_mut()
            .retain(|l| !(l.owner_id == owner_id && l.id == id));
        for r in self.reminders.borrow_mut().iter_mut() {
            if r.owner_id == owner_id && r.list_id.as_deref() == Some(id) {
                r.list_id = None;
                r.version += 1;
                r.updated_at = now;
            }
        }
        Ok(self.lists.borrow().len() < before)
    }
    async fn due_for_scan(&self, now: i64, limit: u32) -> Result<Vec<Reminder>, String> {
        let mut v: Vec<Reminder> = self
            .reminders
            .borrow()
            .iter()
            .filter(|r| {
                r.wake_at.is_some_and(|w| w <= now)
                    && r.deleted_at.is_none()
                    && r.completed_at.is_none()
            })
            .cloned()
            .collect();
        v.sort_by_key(|r| r.wake_at);
        v.truncate(limit as usize);
        Ok(v)
    }
}
