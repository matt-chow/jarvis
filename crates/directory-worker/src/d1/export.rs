//! `ExportSource` over the D1 repositories (no new SQL beyond each module's).

use super::{
    D1EventRepository, D1FileRepository, D1HabitRepository, D1InboxRepository, D1JournalRepository,
    D1LibraryRepository, D1LinkRepository, D1NoteRepository, D1PreferencesRepository,
    D1ProjectRepository, D1TaskRepository,
};
use directory_core::calendar::{Event, EventRepository, MAX_EVENTS};
use directory_core::export::ExportSource;
use directory_core::files::FileEntry;
use directory_core::habits::{CheckIn, Habit, HabitRepository};
use directory_core::inbox::{InboxItem, InboxRepository};
use directory_core::journal::Entry;
use directory_core::library::{LibraryRepository, Progress};
use directory_core::links::Link;
use directory_core::notes::Note;
use directory_core::preferences::{from_stored, Preferences, PreferencesRepository};
use directory_core::projects::{Project, ProjectRepository};
use directory_core::tasks::{Filter, Task, TaskRepository};

/// Large enough for a personal dataset; export streams in a later milestone.
const EXPORT_LIMIT: u32 = 100_000;

pub struct D1ExportSource {
    pub tasks: D1TaskRepository,
    pub projects: D1ProjectRepository,
    pub notes: D1NoteRepository,
    pub journal: D1JournalRepository,
    pub inbox: D1InboxRepository,
    pub links: D1LinkRepository,
    pub files: D1FileRepository,
    pub library: D1LibraryRepository,
    pub habits: D1HabitRepository,
    pub events: D1EventRepository,
    pub preferences: D1PreferencesRepository,
}

impl ExportSource for D1ExportSource {
    async fn tasks(&self, owner_id: &str) -> Result<Vec<Task>, String> {
        self.tasks.list(owner_id, Filter::All, EXPORT_LIMIT).await
    }
    async fn projects(&self, owner_id: &str) -> Result<Vec<Project>, String> {
        self.projects.list(owner_id, true, EXPORT_LIMIT).await
    }
    async fn notes(&self, owner_id: &str) -> Result<Vec<Note>, String> {
        self.notes.all(owner_id).await
    }
    async fn journal(&self, owner_id: &str) -> Result<Vec<Entry>, String> {
        self.journal.all(owner_id).await
    }
    async fn inbox(&self, owner_id: &str) -> Result<Vec<InboxItem>, String> {
        self.inbox.list(owner_id, EXPORT_LIMIT).await
    }
    async fn links(&self, owner_id: &str) -> Result<Vec<Link>, String> {
        self.links.all(owner_id).await
    }
    async fn files(&self, owner_id: &str) -> Result<Vec<FileEntry>, String> {
        self.files.all(owner_id).await
    }
    async fn reading_progress(&self, owner_id: &str) -> Result<Vec<Progress>, String> {
        self.library.progress(owner_id).await
    }
    async fn habits(&self, owner_id: &str) -> Result<Vec<Habit>, String> {
        self.habits.list(owner_id, EXPORT_LIMIT).await
    }
    async fn habit_checkins(&self, owner_id: &str) -> Result<Vec<CheckIn>, String> {
        self.habits
            .checkins(owner_id, "0000-01-01", "9999-12-31")
            .await
    }
    async fn events(&self, owner_id: &str) -> Result<Vec<Event>, String> {
        self.events.list(owner_id, MAX_EVENTS).await
    }
    async fn preferences(&self, owner_id: &str) -> Result<Preferences, String> {
        Ok(self
            .preferences
            .get(owner_id)
            .await?
            .map(|d| from_stored(&d))
            .unwrap_or_default())
    }
}
