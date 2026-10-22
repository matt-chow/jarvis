//! Full data export: one JSON document with every record the owner has.
//! Contains no credential material (sessions, passkeys, device tokens).

use crate::files::FileEntry;
use crate::identity::User;
use crate::inbox::InboxItem;
use crate::journal::Entry;
use crate::links::{EntityRef, Link};
use crate::notes::Note;
use crate::projects::Project;
use crate::tasks::Task;
use serde::Serialize;

pub const FORMAT: &str = "directory-export";
/// Bump when the document shape changes incompatibly.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
}

pub async fn export<S: ExportSource>(
    source: &S,
    user: User,
    schema_version: u32,
    now: i64,
) -> Result<Export, String> {
    let owner = user.id.clone();
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
        let text = json.to_string();
        for secret in ["token_hash", "public_key", "sign_count", "owner_id"] {
            assert!(!text.contains(secret), "{secret} leaked");
        }
    }
}
