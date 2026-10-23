//! Full data export: one JSON document with every record the owner has.
//! Contains no credential material (sessions, passkeys, device tokens).

use crate::calendar::Event;
use crate::files::FileEntry;
use crate::habits::{CheckIn, Habit};
use crate::identity::User;
use crate::inbox::InboxItem;
use crate::journal::Entry;
use crate::library::Progress;
use crate::links::{EntityRef, Link};
use crate::music::Playlist;
use crate::notes::Note;
use crate::preferences::Preferences;
use crate::projects::Project;
use crate::tasks::Task;
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "directory-export";
/// Bump when the document shape changes incompatibly.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRecord {
    pub id: String,
    pub a: EntityRef,
    pub b: EntityRef,
    pub created_at: i64,
}

impl From<Link> for LinkRecord {
    fn from(l: Link) -> Self {
        Self {
            id: l.id,
            a: l.a,
            b: l.b,
            created_at: l.created_at,
        }
    }
}

/// A habit with every check-in date, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitRecord {
    #[serde(flatten)]
    pub habit: Habit,
    pub checkins: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Export {
    pub format: &'static str,
    pub format_version: u32,
    pub schema_version: u32,
    pub exported_at: i64,
    pub user: User,
    pub tasks: Vec<Task>,
    pub projects: Vec<Project>,
    pub notes: Vec<Note>,
    pub journal: Vec<Entry>,
    pub inbox: Vec<InboxItem>,
    pub links: Vec<LinkRecord>,
    /// File metadata only; download contents via the files API.
    pub files: Vec<FileEntry>,
    pub reading_progress: Vec<Progress>,
    pub habits: Vec<HabitRecord>,
    pub events: Vec<Event>,
    pub preferences: Preferences,
    pub playlists: Vec<Playlist>,
}

/// Everything owned by `owner_id`, per module.
pub trait ExportSource {
    async fn tasks(&self, owner_id: &str) -> Result<Vec<Task>, String>;
    async fn projects(&self, owner_id: &str) -> Result<Vec<Project>, String>;
    async fn notes(&self, owner_id: &str) -> Result<Vec<Note>, String>;
    async fn journal(&self, owner_id: &str) -> Result<Vec<Entry>, String>;
    async fn inbox(&self, owner_id: &str) -> Result<Vec<InboxItem>, String>;
    async fn links(&self, owner_id: &str) -> Result<Vec<Link>, String>;
    async fn files(&self, owner_id: &str) -> Result<Vec<FileEntry>, String>;
    async fn reading_progress(&self, owner_id: &str) -> Result<Vec<Progress>, String>;
    async fn habits(&self, owner_id: &str) -> Result<Vec<Habit>, String>;
    async fn habit_checkins(&self, owner_id: &str) -> Result<Vec<CheckIn>, String>;
    async fn events(&self, owner_id: &str) -> Result<Vec<Event>, String>;
    async fn preferences(&self, owner_id: &str) -> Result<Preferences, String>;
    async fn playlists(&self, owner_id: &str) -> Result<Vec<Playlist>, String>;
}

pub async fn export<S: ExportSource>(
    source: &S,
    user: User,
    schema_version: u32,
    now: i64,
) -> Result<Export, String> {
    let owner = user.id.clone();
    let mut checkins = source.habit_checkins(&owner).await?;
    checkins.sort_by(|a, b| a.date.cmp(&b.date));
    let habits = source
        .habits(&owner)
        .await?
        .into_iter()
        .map(|habit| HabitRecord {
            checkins: checkins
                .iter()
                .filter(|c| c.habit_id == habit.id)
                .map(|c| c.date.clone())
                .collect(),
            habit,
        })
        .collect();
    Ok(Export {
        format: FORMAT,
        format_version: FORMAT_VERSION,
        schema_version,
        exported_at: now,
        tasks: source.tasks(&owner).await?,
        projects: source.projects(&owner).await?,
        notes: source.notes(&owner).await?,
        journal: source.journal(&owner).await?,
        inbox: source.inbox(&owner).await?,
        links: source
            .links(&owner)
            .await?
            .into_iter()
            .map(LinkRecord::from)
            .collect(),
        files: source.files(&owner).await?,
        reading_progress: source.reading_progress(&owner).await?,
        habits,
        events: source.events(&owner).await?,
        preferences: source.preferences(&owner).await?,
        playlists: source.playlists(&owner).await?,
        user,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::EntityKind;
    use crate::tasks::{Priority, Status};
    use crate::test_util::block_on;

    struct Fake;

    impl ExportSource for Fake {
        async fn tasks(&self, owner_id: &str) -> Result<Vec<Task>, String> {
            Ok(vec![Task {
                id: "t".into(),
                owner_id: owner_id.into(),
                parent_id: None,
                project_id: None,
                title: "T".into(),
                notes: String::new(),
                status: Status::Todo,
                priority: Priority::None,
                due_date: None,
                created_at: 1,
                updated_at: 1,
                completed_at: None,
            }])
        }
        async fn projects(&self, _: &str) -> Result<Vec<Project>, String> {
            Ok(vec![])
        }
        async fn notes(&self, _: &str) -> Result<Vec<Note>, String> {
            Ok(vec![])
        }
        async fn journal(&self, _: &str) -> Result<Vec<Entry>, String> {
            Ok(vec![])
        }
        async fn inbox(&self, _: &str) -> Result<Vec<InboxItem>, String> {
            Ok(vec![])
        }
        async fn links(&self, owner_id: &str) -> Result<Vec<Link>, String> {
            let r = |id: &str| EntityRef {
                kind: EntityKind::Task,
                id: id.into(),
            };
            Ok(vec![Link {
                id: "l".into(),
                owner_id: owner_id.into(),
                a: r("a"),
                b: r("b"),
                created_at: 2,
            }])
        }
        async fn files(&self, _: &str) -> Result<Vec<FileEntry>, String> {
            Ok(vec![])
        }
        async fn reading_progress(&self, _: &str) -> Result<Vec<Progress>, String> {
            Ok(vec![])
        }
        async fn habits(&self, owner_id: &str) -> Result<Vec<Habit>, String> {
            Ok(vec![Habit {
                id: "h".into(),
                owner_id: owner_id.into(),
                name: "Stretch".into(),
                days: vec![0, 2],
                archived: false,
                created_at: 1,
                updated_at: 1,
            }])
        }
        async fn habit_checkins(&self, _: &str) -> Result<Vec<CheckIn>, String> {
            let c = |date: &str| CheckIn {
                habit_id: "h".into(),
                date: date.into(),
            };
            Ok(vec![c("2026-10-21"), c("2026-10-19")])
        }
        async fn events(&self, _: &str) -> Result<Vec<Event>, String> {
            Ok(vec![])
        }
        async fn preferences(&self, _: &str) -> Result<Preferences, String> {
            Ok(Preferences::default())
        }
        async fn playlists(&self, _: &str) -> Result<Vec<Playlist>, String> {
            Ok(vec![])
        }
    }

    #[test]
    fn document_shape_has_no_owner_or_secret_fields() {
        let user = User {
            id: "u1".into(),
            display_name: "Owner".into(),
            created_at: 0,
        };
        let doc = block_on(export(&Fake, user, 9, 100)).unwrap();
        let json = serde_json::to_value(&doc).unwrap();
        assert_eq!(json["format"], FORMAT);
        assert_eq!(json["schema_version"], 9);
        assert_eq!(json["tasks"][0]["title"], "T");
        assert!(json["tasks"][0].get("owner_id").is_none());
        assert_eq!(json["links"][0]["a"]["kind"], "task");
        assert_eq!(json["habits"][0]["name"], "Stretch");
        assert_eq!(json["habits"][0]["days"], serde_json::json!([0, 2]));
        assert_eq!(
            json["habits"][0]["checkins"],
            serde_json::json!(["2026-10-19", "2026-10-21"])
        );
        let text = json.to_string();
        for secret in ["token_hash", "public_key", "sign_count", "owner_id"] {
            assert!(!text.contains(secret), "{secret} leaked");
        }
    }
}
