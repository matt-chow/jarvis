//! Restore route (DECISIONS D-025): `POST /api/v1/import/{kind}` with
//! `{ "records": [...] }`, one batch of one kind from a data export.

use super::auth::principal;
use super::{error, error_with, json, read_json_limited};
use crate::d1::{
    D1BlockRepository, D1EventRepository, D1FileRepository, D1HabitRepository, D1InboxRepository,
    D1JournalRepository, D1LinkRepository, D1MusicRepository, D1NoteRepository,
    D1PreferencesRepository, D1ProjectRepository, D1TaskRepository,
};
use crate::{bindings, try_or_respond};
use directory_core::backup::{self, BackupError, ImportTargets};
use serde::Deserialize;
use serde_json::Value;
use worker::{Env, Request, Response, Result, RouteContext};

/// A batch of 100 notes can be large (100k-character bodies).
const IMPORT_JSON_BYTES: usize = 4 * 1024 * 1024;

#[derive(Deserialize)]
struct Batch {
    records: Vec<Value>,
}

struct Targets {
    tasks: D1TaskRepository,
    projects: D1ProjectRepository,
    notes: D1NoteRepository,
    journal: D1JournalRepository,
    inbox: D1InboxRepository,
    links: D1LinkRepository,
    habits: D1HabitRepository,
    events: D1EventRepository,
    blocks: D1BlockRepository,
    music: D1MusicRepository,
    preferences: D1PreferencesRepository,
}

impl Targets {
    fn from_env(env: &Env) -> Result<Self> {
        let db = || env.d1(bindings::DB);
        Ok(Self {
            tasks: D1TaskRepository::new(db()?),
            projects: D1ProjectRepository::new(db()?),
            notes: D1NoteRepository::new(db()?),
            journal: D1JournalRepository::new(db()?),
            inbox: D1InboxRepository::new(db()?),
            links: D1LinkRepository::new(db()?),
            habits: D1HabitRepository::new(db()?),
            events: D1EventRepository::new(db()?),
            blocks: D1BlockRepository::new(db()?),
            music: D1MusicRepository::new(db()?, D1FileRepository::new(db()?)),
            preferences: D1PreferencesRepository::new(db()?),
        })
    }
}

impl ImportTargets for Targets {
    type Tasks = D1TaskRepository;
    type Projects = D1ProjectRepository;
    type Notes = D1NoteRepository;
    type Journal = D1JournalRepository;
    type Inbox = D1InboxRepository;
    type Links = D1LinkRepository;
    type Habits = D1HabitRepository;
    type Events = D1EventRepository;
    type Blocks = D1BlockRepository;
    type Music = D1MusicRepository;
    type Preferences = D1PreferencesRepository;
    fn tasks(&self) -> &D1TaskRepository {
        &self.tasks
    }
    fn projects(&self) -> &D1ProjectRepository {
        &self.projects
    }
    fn notes(&self) -> &D1NoteRepository {
        &self.notes
    }
    fn journal(&self) -> &D1JournalRepository {
        &self.journal
    }
    fn inbox(&self) -> &D1InboxRepository {
        &self.inbox
    }
    fn links(&self) -> &D1LinkRepository {
        &self.links
    }
    fn habits(&self) -> &D1HabitRepository {
        &self.habits
    }
    fn blocks(&self) -> &D1BlockRepository {
        &self.blocks
    }
    fn events(&self) -> &D1EventRepository {
        &self.events
    }
    fn music(&self) -> &D1MusicRepository {
        &self.music
    }
    fn preferences(&self) -> &D1PreferencesRepository {
        &self.preferences
    }
}

pub async fn import(mut req: Request, rc: RouteContext<()>) -> Result<Response> {
    let p = try_or_respond!(principal(&req, &rc.env).await?);
    let batch: Batch = try_or_respond!(read_json_limited(&mut req, IMPORT_JSON_BYTES).await?);
    let kind = rc.param("kind").cloned().unwrap_or_default();
    let targets = Targets::from_env(&rc.env)?;
    match backup::import(&targets, &p, &kind, batch.records).await {
        Ok(report) => json(200, &report),
        Err(BackupError::Invalid(field, why)) => {
            error_with(400, "invalid_input", format!("{field}: {why}"))
        }
        Err(BackupError::Backend(detail)) => {
            worker::console_error!("import backend error: {detail}");
            error(502, "backend_unavailable")
        }
    }
}
