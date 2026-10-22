//! Links: undirected references between an owner's entities (task, note,
//! project, journal day). Each pair is stored once, in a normalized order.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_LINKS_PER_ENTITY: u32 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Journal,
    Note,
    Project,
    Task,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Journal => "journal",
            Self::Note => "note",
            Self::Project => "project",
            Self::Task => "task",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "journal" => Self::Journal,
            "note" => Self::Note,
            "project" => Self::Project,
            "task" => Self::Task,
            _ => return None,
        })
    }
}

/// An entity reference. For journal days the id is the date (`YYYY-MM-DD`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    pub kind: EntityKind,
    pub id: String,
}

impl EntityRef {
    fn validate(&self, field: &'static str) -> Result<(), LinkError> {
        let ok = match self.kind {
            EntityKind::Journal => validate::date(&self.id).is_ok(),
            _ => is_id(&self.id),
        };
        if ok {
            Ok(())
        } else {
            Err(LinkError::Invalid(field, "malformed id"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub id: String,
    pub owner_id: String,
    /// Normalized so `a < b`.
    pub a: EntityRef,
    pub b: EntityRef,
    pub created_at: i64,
}

impl Link {
    /// The end that isn't `this`.
    pub fn other(&self, this: &EntityRef) -> &EntityRef {
        if &self.a == this {
            &self.b
        } else {
            &self.a
        }
    }
}

/// One row of "what is linked to X", with the other end's display title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Linked {
    pub link_id: String,
    pub kind: EntityKind,
    pub id: String,
    pub title: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewLink {
    pub from: EntityRef,
    pub to: EntityRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    Invalid(&'static str, &'static str),
    NotFound,
    Backend(String),
}

pub trait LinkRepository {
    /// Display title of an owned entity, or `None` if it doesn't exist (or isn't theirs).
    async fn title_of(&self, owner_id: &str, entity: &EntityRef) -> Result<Option<String>, String>;
    async fn find(
        &self,
        owner_id: &str,
        a: &EntityRef,
        b: &EntityRef,
    ) -> Result<Option<Link>, String>;
    async fn insert(&self, link: &Link) -> Result<(), String>;
    /// Links where `entity` is either end.
    async fn list_for(
        &self,
        owner_id: &str,
        entity: &EntityRef,
        limit: u32,
    ) -> Result<Vec<Link>, String>;
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
}

fn backend(e: String) -> LinkError {
    LinkError::Backend(e)
}

pub struct LinkService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: LinkRepository, E: Entropy + ?Sized> LinkService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    /// Create a link (idempotent). Returns the other end as seen from `from`.
    pub async fn link(&self, p: &Principal, input: NewLink) -> Result<Linked, LinkError> {
        input.from.validate("from")?;
        input.to.validate("to")?;
        if input.from == input.to {
            return Err(LinkError::Invalid("to", "cannot link an item to itself"));
        }
        if self
            .repo
            .title_of(&p.user_id, &input.from)
            .await
            .map_err(backend)?
            .is_none()
        {
            return Err(LinkError::Invalid("from", "not found"));
        }
        let Some(title) = self
            .repo
            .title_of(&p.user_id, &input.to)
            .await
            .map_err(backend)?
        else {
            return Err(LinkError::Invalid("to", "not found"));
        };
        let (a, b) = if input.from < input.to {
            (input.from, input.to.clone())
        } else {
            (input.to.clone(), input.from)
        };
        let link = match self.repo.find(&p.user_id, &a, &b).await.map_err(backend)? {
            Some(existing) => existing,
            None => {
                let link = Link {
                    id: new_id(self.entropy),
                    owner_id: p.user_id.clone(),
                    a,
                    b,
                    created_at: self.now,
                };
                self.repo.insert(&link).await.map_err(backend)?;
                link
            }
        };
        Ok(Linked {
            link_id: link.id,
            kind: input.to.kind,
            id: input.to.id,
            title,
            created_at: link.created_at,
        })
    }

    /// Everything linked to `entity`. Links whose other end was deleted are
    /// dropped from the result and removed.
    pub async fn list(&self, p: &Principal, entity: EntityRef) -> Result<Vec<Linked>, LinkError> {
        entity.validate("id")?;
        let links = self
            .repo
            .list_for(&p.user_id, &entity, MAX_LINKS_PER_ENTITY)
            .await
            .map_err(backend)?;
        let mut out = Vec::with_capacity(links.len());
        for link in links {
            let other = link.other(&entity).clone();
            match self
                .repo
                .title_of(&p.user_id, &other)
                .await
                .map_err(backend)?
            {
                Some(title) => out.push(Linked {
                    link_id: link.id,
                    kind: other.kind,
                    id: other.id,
                    title,
                    created_at: link.created_at,
                }),
                None => {
                    self.repo
                        .delete(&p.user_id, &link.id)
                        .await
                        .map_err(backend)?;
                }
            }
        }
        out.sort_by(|x, y| {
            x.kind
                .cmp(&y.kind)
                .then(x.title.to_lowercase().cmp(&y.title.to_lowercase()))
        });
        Ok(out)
    }

    pub async fn unlink(&self, p: &Principal, link_id: &str) -> Result<(), LinkError> {
        if is_id(link_id)
            && self
                .repo
                .delete(&p.user_id, link_id)
                .await
                .map_err(backend)?
        {
            Ok(())
        } else {
            Err(LinkError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entropy::testing::CountingEntropy;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Memory {
        entities: RefCell<HashMap<(String, EntityRef), String>>,
        links: RefCell<Vec<Link>>,
    }

    impl LinkRepository for Memory {
        async fn title_of(&self, owner_id: &str, e: &EntityRef) -> Result<Option<String>, String> {
            Ok(self
                .entities
                .borrow()
                .get(&(owner_id.to_owned(), e.clone()))
                .cloned())
        }
        async fn find(
            &self,
            owner_id: &str,
            a: &EntityRef,
            b: &EntityRef,
        ) -> Result<Option<Link>, String> {
            Ok(self
                .links
                .borrow()
                .iter()
                .find(|l| l.owner_id == owner_id && &l.a == a && &l.b == b)
                .cloned())
        }
        async fn insert(&self, link: &Link) -> Result<(), String> {
            self.links.borrow_mut().push(link.clone());
            Ok(())
        }
        async fn list_for(
            &self,
            owner_id: &str,
            e: &EntityRef,
            limit: u32,
        ) -> Result<Vec<Link>, String> {
            Ok(self
                .links
                .borrow()
                .iter()
                .filter(|l| l.owner_id == owner_id && (&l.a == e || &l.b == e))
                .take(limit as usize)
                .cloned()
                .collect())
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.links.borrow_mut();
            let before = all.len();
            all.retain(|l| !(l.owner_id == owner_id && l.id == id));
            Ok(all.len() < before)
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    fn r(kind: EntityKind, id: &str) -> EntityRef {
        EntityRef {
            kind,
            id: id.into(),
        }
    }

    fn fixture() -> (Memory, EntityRef, EntityRef, EntityRef) {
        let m = Memory::default();
        let task = r(EntityKind::Task, &"1".repeat(32));
        let note = r(EntityKind::Note, &"2".repeat(32));
        let day = r(EntityKind::Journal, "2026-10-21");
        for (e, t) in [
            (&task, "Book flights"),
            (&note, "Trip ideas"),
            (&day, "2026-10-21"),
        ] {
            m.entities
                .borrow_mut()
                .insert(("u1".into(), e.clone()), t.into());
        }
        (m, task, note, day)
    }

    #[test]
    fn links_are_symmetric_and_idempotent() {
        let (m, task, note, day) = fixture();
        let e = CountingEntropy::default();
        let svc = LinkService::new(&m, &e, 5);
        let a = block_on(svc.link(
            &me(),
            NewLink {
                from: task.clone(),
                to: note.clone(),
            },
        ))
        .unwrap();
        assert_eq!((a.kind, a.title.as_str()), (EntityKind::Note, "Trip ideas"));
        let again = block_on(svc.link(
            &me(),
            NewLink {
                from: note.clone(),
                to: task.clone(),
            },
        ))
        .unwrap();
        assert_eq!(
            again.link_id, a.link_id,
            "reverse direction is the same link"
        );
        assert_eq!(m.links.borrow().len(), 1);

        block_on(svc.link(
            &me(),
            NewLink {
                from: day.clone(),
                to: task.clone(),
            },
        ))
        .unwrap();
        let from_task = block_on(svc.list(&me(), task.clone())).unwrap();
        assert_eq!(
            from_task
                .iter()
                .map(|l| l.title.as_str())
                .collect::<Vec<_>>(),
            ["2026-10-21", "Trip ideas"]
        );
        let from_note = block_on(svc.list(&me(), note)).unwrap();
        assert_eq!(from_note[0].title, "Book flights");
    }

    #[test]
    fn validates_ends_and_ownership() {
        let (m, task, note, _) = fixture();
        let e = CountingEntropy::default();
        let svc = LinkService::new(&m, &e, 5);
        assert_eq!(
            block_on(svc.link(
                &me(),
                NewLink {
                    from: task.clone(),
                    to: task.clone()
                }
            ))
            .unwrap_err(),
            LinkError::Invalid("to", "cannot link an item to itself")
        );
        assert_eq!(
            block_on(svc.link(
                &me(),
                NewLink {
                    from: task.clone(),
                    to: r(EntityKind::Journal, "2026-13-01")
                }
            ))
            .unwrap_err(),
            LinkError::Invalid("to", "malformed id")
        );
        assert_eq!(
            block_on(svc.link(
                &me(),
                NewLink {
                    from: task.clone(),
                    to: r(EntityKind::Project, &"9".repeat(32))
                }
            ))
            .unwrap_err(),
            LinkError::Invalid("to", "not found")
        );
        let other = Principal {
            user_id: "u2".into(),
            method: AuthMethod::Session,
        };
        assert_eq!(
            block_on(svc.link(
                &other,
                NewLink {
                    from: task,
                    to: note
                }
            ))
            .unwrap_err(),
            LinkError::Invalid("from", "not found")
        );
    }

    #[test]
    fn dangling_links_are_hidden_and_cleaned_up() {
        let (m, task, note, _) = fixture();
        let e = CountingEntropy::default();
        let svc = LinkService::new(&m, &e, 5);
        let l = block_on(svc.link(
            &me(),
            NewLink {
                from: task.clone(),
                to: note.clone(),
            },
        ))
        .unwrap();
        m.entities.borrow_mut().remove(&("u1".into(), note));
        assert!(block_on(svc.list(&me(), task)).unwrap().is_empty());
        assert!(m.links.borrow().is_empty());
        assert_eq!(
            block_on(svc.unlink(&me(), &l.link_id)).unwrap_err(),
            LinkError::NotFound
        );
    }
}
