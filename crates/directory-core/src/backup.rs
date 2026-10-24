//! Restore from a data export (DECISIONS D-025): merge-only, one kind at a
//! time, in small batches. Records keep their ids and timestamps, become the
//! importing owner's, and are validated like ordinary creates. A record whose
//! id already exists is skipped, never overwritten. List caps (D-022) apply.

use crate::calendar::{self, Event, EventRepository};
use crate::entropy::is_id;
use crate::export::{HabitRecord, LinkRecord, FORMAT_VERSION};
use crate::habits::{self, HabitRepository};
use crate::identity::Principal;
use crate::inbox::{self, InboxItem, InboxRepository};
use crate::journal::{self, Entry, JournalRepository};
use crate::links::{Link, LinkRepository};
use crate::music::{self, MusicRepository, Playlist};
use crate::notes::{self, Note, NoteRepository};
use crate::preferences::{from_stored, Preferences, PreferencesRepository};
use crate::projects::{self, Project, ProjectRepository};
use crate::tasks::{self, Task, TaskRepository};
use crate::validate;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

/// Records per request; keeps each one well inside the Workers CPU budget.
pub const MAX_BATCH: usize = 100;

/// What can be restored, in the order a client should send it (later kinds
/// refer to earlier ones). File contents aren't in an export, so files and
/// reading progress can't be restored.
pub const KINDS: &[&str] = &[
    "projects",
    "tasks",
    "notes",
    "journal",
    "inbox",
    "habits",
    "events",
    "playlists",
    "links",
    "preferences",
];

/// Repositories an import writes through (the Worker's D1 adapters, fakes in tests).
pub trait ImportTargets {
    type Tasks: TaskRepository;
    type Projects: ProjectRepository;
    type Notes: NoteRepository;
    type Journal: JournalRepository;
    type Inbox: InboxRepository;
    type Links: LinkRepository;
    type Habits: HabitRepository;
    type Events: EventRepository;
    type Music: MusicRepository;
    type Preferences: PreferencesRepository;
    fn tasks(&self) -> &Self::Tasks;
    fn projects(&self) -> &Self::Projects;
    fn notes(&self) -> &Self::Notes;
    fn journal(&self) -> &Self::Journal;
    fn inbox(&self) -> &Self::Inbox;
    fn links(&self) -> &Self::Links;
    fn habits(&self) -> &Self::Habits;
    fn events(&self) -> &Self::Events;
    fn music(&self) -> &Self::Music;
    fn preferences(&self) -> &Self::Preferences;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rejected {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub inserted: u32,
    /// Already present (same id): left as is.
    pub skipped: u32,
    pub rejected: Vec<Rejected>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupError {
    Invalid(&'static str, String),
    Backend(String),
}

enum Outcome {
    Inserted,
    Skipped,
    Refused(String),
}
use Outcome::*;

fn reject(why: impl Into<String>) -> Result<Outcome, String> {
    Ok(Refused(why.into()))
}

fn ok_or_reject<T>(r: Result<T, &'static str>, field: &str) -> Result<T, String> {
    r.map_err(|w| format!("{field}: {w}"))
}

/// Check an export's header before sending any batch.
pub fn check_header(format: &str, version: u32) -> Result<(), BackupError> {
    if format != crate::export::FORMAT {
        return Err(BackupError::Invalid(
            "format",
            "not a Directory export".into(),
        ));
    }
    if version != FORMAT_VERSION {
        return Err(BackupError::Invalid(
            "format_version",
            format!("version {version} isn't supported (expected {FORMAT_VERSION})"),
        ));
    }
    Ok(())
}

/// The id shown in a report for a record that couldn't be parsed.
fn label(v: &Value) -> String {
    ["id", "date"]
        .iter()
        .find_map(|k| v.get(k).and_then(Value::as_str))
        .unwrap_or("?")
        .chars()
        .take(64)
        .collect()
}

/// Import one batch of `kind`.
pub async fn import<T: ImportTargets>(
    t: &T,
    p: &Principal,
    kind: &str,
    records: Vec<Value>,
) -> Result<ImportReport, BackupError> {
    if !KINDS.contains(&kind) {
        return Err(BackupError::Invalid("kind", format!("unknown kind {kind}")));
    }
    if records.len() > MAX_BATCH {
        return Err(BackupError::Invalid(
            "records",
            format!("at most {MAX_BATCH} per batch"),
        ));
    }
    let mut report = ImportReport::default();
    for v in records {
        let id = label(&v);
        let outcome = match kind {
            "projects" => one(v, |r| project(t, p, r)).await,
            "tasks" => one(v, |r| task(t, p, r)).await,
            "notes" => one(v, |r| note(t, p, r)).await,
            "journal" => one(v, |r| entry(t, p, r)).await,
            "inbox" => one(v, |r| inbox_item(t, p, r)).await,
            "habits" => one(v, |r| habit(t, p, r)).await,
            "events" => one(v, |r| event(t, p, r)).await,
            "playlists" => one(v, |r| playlist(t, p, r)).await,
            "links" => one(v, |r| link(t, p, r)).await,
            _ => one(v, |r| prefs(t, p, r)).await,
        }
        .map_err(BackupError::Backend)?;
        match outcome {
            Inserted => report.inserted += 1,
            Skipped => report.skipped += 1,
            Refused(reason) => report.rejected.push(Rejected { id, reason }),
        }
    }
    Ok(report)
}

/// Parse one record, then import it; a malformed record is rejected, not fatal.
async fn one<R: DeserializeOwned, F, Fut>(v: Value, f: F) -> Result<Outcome, String>
where
    F: FnOnce(R) -> Fut,
    Fut: std::future::Future<Output = Result<Outcome, String>>,
{
    match serde_json::from_value::<R>(v) {
        Ok(r) => f(r).await,
        Err(e) => reject(format!("malformed: {e}")),
    }
}

fn timestamps(created: i64, updated: i64) -> Result<(), String> {
    if created < 0 || updated < created {
        return Err("timestamps: invalid".into());
    }
    Ok(())
}

macro_rules! try_reject {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(why) => return reject(why),
        }
    };
}

async fn project<T: ImportTargets>(
    t: &T,
    p: &Principal,
    mut r: Project,
) -> Result<Outcome, String> {
    if !is_id(&r.id) {
        return reject("id: malformed");
    }
    let repo = t.projects();
    if repo.get(&p.user_id, &r.id).await?.is_some() {
        return Ok(Skipped);
    }
    r.owner_id = p.user_id.clone();
    r.name = try_reject!(ok_or_reject(
        validate::line(&r.name, projects::MAX_NAME),
        "name"
    ));
    r.description = try_reject!(ok_or_reject(
        validate::text(&r.description, projects::MAX_DESCRIPTION),
        "description"
    ));
    try_reject!(timestamps(r.created_at, r.updated_at));
    if repo.count(&p.user_id).await? >= projects::MAX_LIST {
        return reject("projects: limit reached");
    }
    repo.insert(&r).await?;
    Ok(Inserted)
}

async fn task<T: ImportTargets>(t: &T, p: &Principal, mut r: Task) -> Result<Outcome, String> {
    if !is_id(&r.id) {
        return reject("id: malformed");
    }
    let repo = t.tasks();
    if repo.get(&p.user_id, &r.id).await?.is_some() {
        return Ok(Skipped);
    }
    r.owner_id = p.user_id.clone();
    r.title = try_reject!(ok_or_reject(
        validate::line(&r.title, tasks::MAX_TITLE),
        "title"
    ));
    r.notes = try_reject!(ok_or_reject(
        validate::text(&r.notes, tasks::MAX_NOTES),
        "notes"
    ));
    if let Some(d) = &r.due_date {
        try_reject!(ok_or_reject(validate::date(d), "due_date"));
    }
    if r.estimate_minutes
        .is_some_and(|m| m == 0 || m > tasks::MAX_ESTIMATE)
    {
        return reject("estimate_minutes: must be 1 to 1440");
    }
    try_reject!(timestamps(r.created_at, r.updated_at));
    if let Some(parent) = &r.parent_id {
        // Parents are sent before subtasks; one level only.
        match repo.get(&p.user_id, parent).await? {
            Some(pt) if pt.parent_id.is_none() => r.project_id = None,
            Some(_) => return reject("parent_id: subtasks cannot have subtasks"),
            None => return reject("parent_id: not found (import its parent first)"),
        }
    }
    if let Some(project) = &r.project_id {
        if !repo.project_exists(&p.user_id, project).await? {
            // Keep the task, just unassigned.
            r.project_id = None;
        }
    }
    if r.status.is_open() && repo.count_open(&p.user_id).await? >= tasks::MAX_OPEN {
        return reject("tasks: open-task limit reached");
    }
    repo.insert(&r).await?;
    Ok(Inserted)
}

async fn note<T: ImportTargets>(t: &T, p: &Principal, mut r: Note) -> Result<Outcome, String> {
    if !is_id(&r.id) {
        return reject("id: malformed");
    }
    let repo = t.notes();
    if repo.get(&p.user_id, &r.id).await?.is_some() {
        return Ok(Skipped);
    }
    r.owner_id = p.user_id.clone();
    r.title = r.title.trim().to_owned();
    if r.title.chars().count() > notes::MAX_TITLE {
        return reject("title: too long");
    }
    r.body = try_reject!(ok_or_reject(
        validate::text(&r.body, notes::MAX_BODY),
        "body"
    ));
    try_reject!(timestamps(r.created_at, r.updated_at));
    if repo.count(&p.user_id).await? >= notes::MAX_LIST {
        return reject("notes: limit reached");
    }
    repo.insert(&r).await?;
    Ok(Inserted)
}

async fn entry<T: ImportTargets>(t: &T, p: &Principal, mut r: Entry) -> Result<Outcome, String> {
    r.owner_id = p.user_id.clone();
    r.date = try_reject!(ok_or_reject(validate::date(&r.date), "date"));
    r.body = try_reject!(ok_or_reject(
        validate::text(&r.body, journal::MAX_BODY),
        "body"
    ));
    if r.mood.is_some_and(|m| !(1..=5).contains(&m)) {
        return reject("mood: expected 1 to 5");
    }
    try_reject!(timestamps(r.created_at, r.updated_at));
    // A day that already has an entry is kept as is.
    Ok(if t.journal().insert_if_absent(&r).await? {
        Inserted
    } else {
        Skipped
    })
}

async fn inbox_item<T: ImportTargets>(
    t: &T,
    p: &Principal,
    mut r: InboxItem,
) -> Result<Outcome, String> {
    if !is_id(&r.id) {
        return reject("id: malformed");
    }
    let repo = t.inbox();
    if repo.get(&p.user_id, &r.id).await?.is_some() {
        return Ok(Skipped);
    }
    r.owner_id = p.user_id.clone();
    r.text = try_reject!(ok_or_reject(
        validate::text(r.text.trim_start(), inbox::MAX_TEXT),
        "text"
    ));
    if r.text.is_empty() {
        return reject("text: required");
    }
    if repo.count(&p.user_id).await? >= inbox::MAX_LIST {
        return reject("inbox: limit reached");
    }
    repo.insert(&r).await?;
    Ok(Inserted)
}

async fn habit<T: ImportTargets>(t: &T, p: &Principal, r: HabitRecord) -> Result<Outcome, String> {
    let mut h = r.habit;
    if !is_id(&h.id) {
        return reject("id: malformed");
    }
    let repo = t.habits();
    if repo.get(&p.user_id, &h.id).await?.is_some() {
        return Ok(Skipped);
    }
    h.owner_id = p.user_id.clone();
    h.name = try_reject!(ok_or_reject(
        validate::line(&h.name, habits::MAX_NAME),
        "name"
    ));
    h.days = match habits::valid_days(Some(h.days)) {
        Ok(d) => d,
        Err(_) => return reject("days: invalid"),
    };
    try_reject!(timestamps(h.created_at, h.updated_at));
    if r.checkins.iter().any(|d| validate::date(d).is_err()) {
        return reject("checkins: invalid date");
    }
    if repo.count(&p.user_id).await? >= habits::MAX_HABITS {
        return reject("habits: limit reached");
    }
    repo.insert(&h).await?;
    for d in &r.checkins {
        repo.check_in(&p.user_id, &h.id, d, h.updated_at).await?;
    }
    Ok(Inserted)
}

async fn event<T: ImportTargets>(t: &T, p: &Principal, mut r: Event) -> Result<Outcome, String> {
    if !is_id(&r.id) {
        return reject("id: malformed");
    }
    let repo = t.events();
    if repo.get(&p.user_id, &r.id).await?.is_some() {
        return Ok(Skipped);
    }
    r.owner_id = p.user_id.clone();
    try_reject!(timestamps(r.created_at, r.updated_at));
    let r = match calendar::check(r) {
        Ok(e) => e,
        Err(calendar::CalendarError::Invalid(f, w)) => return reject(format!("{f}: {w}")),
        Err(_) => return reject("invalid event"),
    };
    if repo.count(&p.user_id).await? >= calendar::MAX_EVENTS {
        return reject("events: limit reached");
    }
    repo.insert(&r).await?;
    Ok(Inserted)
}

async fn playlist<T: ImportTargets>(
    t: &T,
    p: &Principal,
    mut r: Playlist,
) -> Result<Outcome, String> {
    if !is_id(&r.id) {
        return reject("id: malformed");
    }
    let repo = t.music();
    if repo.get(&p.user_id, &r.id).await?.is_some() {
        return Ok(Skipped);
    }
    r.owner_id = p.user_id.clone();
    r.name = try_reject!(ok_or_reject(
        validate::line(&r.name, music::MAX_NAME),
        "name"
    ));
    // Tracks are file ids; files whose content isn't uploaded again are
    // skipped when the playlist is read.
    if r.tracks.len() > music::MAX_TRACKS || r.tracks.iter().any(|id| !is_id(id)) {
        return reject("tracks: invalid");
    }
    try_reject!(timestamps(r.created_at, r.updated_at));
    if repo.count(&p.user_id).await? >= music::MAX_PLAYLISTS {
        return reject("playlists: limit reached");
    }
    repo.insert(&r).await?;
    Ok(Inserted)
}

async fn link<T: ImportTargets>(t: &T, p: &Principal, r: LinkRecord) -> Result<Outcome, String> {
    if !is_id(&r.id) {
        return reject("id: malformed");
    }
    let repo = t.links();
    let (a, b) = if r.a <= r.b { (r.a, r.b) } else { (r.b, r.a) };
    if a == b {
        return reject("a link needs two different ends");
    }
    if repo.find(&p.user_id, &a, &b).await?.is_some() {
        return Ok(Skipped);
    }
    for end in [&a, &b] {
        if repo.title_of(&p.user_id, end).await?.is_none() {
            return reject(format!(
                "{} {} isn't in this account",
                end.kind.as_str(),
                end.id
            ));
        }
    }
    repo.insert(&Link {
        id: r.id,
        owner_id: p.user_id.clone(),
        a,
        b,
        created_at: r.created_at,
    })
    .await?;
    Ok(Inserted)
}

/// Applied only where the account has no preference of its own yet.
async fn prefs<T: ImportTargets>(t: &T, p: &Principal, r: Preferences) -> Result<Outcome, String> {
    let repo = t.preferences();
    let mut current = repo
        .get(&p.user_id)
        .await?
        .map(|d| from_stored(&d))
        .unwrap_or_default();
    // Validate by round-tripping through the lenient reader.
    let incoming = from_stored(&serde_json::to_string(&r).map_err(|e| e.to_string())?);
    if current.dashboard.is_some() || incoming.dashboard.is_none() {
        return Ok(Skipped);
    }
    current.dashboard = incoming.dashboard;
    let data = serde_json::to_string(&current).map_err(|e| e.to_string())?;
    repo.put(&p.user_id, &data, 0).await?;
    Ok(Inserted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::memory::MemoryEvents;
    use crate::habits::memory::MemoryHabits;
    use crate::identity::AuthMethod;
    use crate::inbox::memory::MemoryInbox;
    use crate::journal::testing::MemoryJournal;
    use crate::links::testing::MemoryLinks;
    use crate::links::{EntityKind, EntityRef};
    use crate::music::testing::MemoryMusic;
    use crate::notes::testing::MemoryNotes;
    use crate::preferences::testing::MemoryPreferences;
    use crate::projects::memory::MemoryProjects;
    use crate::tasks::memory::MemoryTasks;
    use crate::test_util::block_on;
    use serde_json::json;

    #[derive(Default)]
    struct T {
        tasks: MemoryTasks,
        projects: MemoryProjects,
        notes: MemoryNotes,
        journal: MemoryJournal,
        inbox: MemoryInbox,
        links: MemoryLinks,
        habits: MemoryHabits,
        events: MemoryEvents,
        music: MemoryMusic,
        prefs: MemoryPreferences,
    }

    impl ImportTargets for T {
        type Tasks = MemoryTasks;
        type Projects = MemoryProjects;
        type Notes = MemoryNotes;
        type Journal = MemoryJournal;
        type Inbox = MemoryInbox;
        type Links = MemoryLinks;
        type Habits = MemoryHabits;
        type Events = MemoryEvents;
        type Music = MemoryMusic;
        type Preferences = MemoryPreferences;
        fn tasks(&self) -> &MemoryTasks {
            &self.tasks
        }
        fn projects(&self) -> &MemoryProjects {
            &self.projects
        }
        fn notes(&self) -> &MemoryNotes {
            &self.notes
        }
        fn journal(&self) -> &MemoryJournal {
            &self.journal
        }
        fn inbox(&self) -> &MemoryInbox {
            &self.inbox
        }
        fn links(&self) -> &MemoryLinks {
            &self.links
        }
        fn habits(&self) -> &MemoryHabits {
            &self.habits
        }
        fn events(&self) -> &MemoryEvents {
            &self.events
        }
        fn music(&self) -> &MemoryMusic {
            &self.music
        }
        fn preferences(&self) -> &MemoryPreferences {
            &self.prefs
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u2".into(),
            method: AuthMethod::Session,
        }
    }
    fn id(c: char) -> String {
        c.to_string().repeat(32)
    }
    fn run(t: &T, kind: &str, records: Vec<Value>) -> ImportReport {
        block_on(import(t, &me(), kind, records)).unwrap()
    }
    fn task(c: char, parent: Option<char>, project: Option<char>) -> Value {
        json!({
            "id": id(c), "parent_id": parent.map(id), "project_id": project.map(id),
            "title": format!("task {c}"), "notes": "", "status": "todo", "priority": "none",
            "due_date": null, "created_at": 1, "updated_at": 2, "completed_at": null
        })
    }

    #[test]
    fn restores_in_dependency_order_and_never_overwrites() {
        let t = T::default();
        let project = json!({"id": id('1'), "name": "Garden", "description": "", "status": "active", "created_at": 1, "updated_at": 1});
        assert_eq!(run(&t, "projects", vec![project.clone()]).inserted, 1);
        // The task repository checks project ownership through its own table.
        t.tasks.1.borrow_mut().push(("u2".into(), id('1')));

        let r = run(
            &t,
            "tasks",
            vec![
                task('a', None, Some('1')),
                task('b', Some('a'), None),
                task('c', Some('9'), None),
                json!({"id": id('d')}),
            ],
        );
        assert_eq!((r.inserted, r.skipped), (2, 0));
        assert_eq!(r.rejected.len(), 2);
        assert!(
            r.rejected[0].reason.starts_with("parent_id: not found"),
            "{:?}",
            r.rejected
        );
        assert!(
            r.rejected[1].reason.starts_with("malformed"),
            "{:?}",
            r.rejected
        );
        let stored = t
            .tasks
            .0
            .borrow()
            .iter()
            .find(|x| x.id == id('a'))
            .cloned()
            .unwrap();
        assert_eq!(
            (
                stored.owner_id.as_str(),
                stored.project_id.clone(),
                stored.updated_at
            ),
            ("u2", Some(id('1')), 2)
        );

        // Importing again changes nothing.
        let again = run(&t, "tasks", vec![task('a', None, None)]);
        assert_eq!((again.inserted, again.skipped), (0, 1));
        assert_eq!(run(&t, "projects", vec![project]).skipped, 1);

        let day = |body: &str| json!({"date": "2026-10-21", "body": body, "mood": 4, "created_at": 1, "updated_at": 1});
        assert_eq!(run(&t, "journal", vec![day("first")]).inserted, 1);
        assert_eq!(run(&t, "journal", vec![day("other device")]).skipped, 1);
        assert_eq!(t.journal.0.borrow()[0].body, "first");

        let habit = json!({"id": id('2'), "name": "Read", "days": [0, 2], "archived": false, "created_at": 1, "updated_at": 1, "checkins": ["2026-10-20", "2026-10-21"]});
        assert_eq!(run(&t, "habits", vec![habit]).inserted, 1);
        assert_eq!(t.habits.1.borrow().len(), 2, "check-ins restored");

        let bad_event = json!({"id": id('e'), "title": "x", "start_date": "2026-10-22", "end_date": "2026-10-01",
            "start_time": null, "end_time": null, "location": "", "notes": "", "repeat": "none", "repeat_until": null, "created_at": 1, "updated_at": 1});
        let r = run(&t, "events", vec![bad_event]);
        assert_eq!(r.rejected[0].reason, "end_date: before start_date");
    }

    #[test]
    fn links_need_both_ends_and_preferences_never_replace_the_accounts_own() {
        let t = T::default();
        let task_ref = EntityRef {
            kind: EntityKind::Task,
            id: id('a'),
        };
        let note_ref = EntityRef {
            kind: EntityKind::Note,
            id: id('3'),
        };
        t.links
            .entities
            .borrow_mut()
            .insert(("u2".into(), task_ref.clone()), "task".into());
        let link = |b: &EntityRef| json!({"id": id('4'), "a": task_ref, "b": b, "created_at": 1});
        let r = run(&t, "links", vec![link(&note_ref)]);
        assert!(r.rejected[0].reason.contains("isn't in this account"));
        t.links
            .entities
            .borrow_mut()
            .insert(("u2".into(), note_ref.clone()), "note".into());
        assert_eq!(run(&t, "links", vec![link(&note_ref)]).inserted, 1);
        assert_eq!(run(&t, "links", vec![link(&note_ref)]).skipped, 1);

        let prefs = json!({"dashboard": {"widgets": ["today", "weather"]}});
        assert_eq!(run(&t, "preferences", vec![prefs.clone()]).inserted, 1);
        let stored = from_stored(&t.prefs.0.borrow()["u2"]);
        assert_eq!(
            stored.dashboard.unwrap().widgets,
            ["today"],
            "unknown widgets dropped"
        );
        assert_eq!(run(&t, "preferences", vec![prefs]).skipped, 1);
    }

    #[test]
    fn batches_and_headers_are_checked() {
        let t = T::default();
        assert!(matches!(
            block_on(import(&t, &me(), "files", vec![])),
            Err(BackupError::Invalid("kind", _))
        ));
        let many = vec![json!({}); MAX_BATCH + 1];
        assert!(matches!(
            block_on(import(&t, &me(), "notes", many)),
            Err(BackupError::Invalid("records", _))
        ));
        assert!(check_header("directory-export", FORMAT_VERSION).is_ok());
        assert!(check_header("other", 1).is_err());
        assert!(check_header("directory-export", 99).is_err());
    }
}
