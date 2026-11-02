//! Categories: the owner's labels (name, colour, optional icon) shared by
//! tasks and reminders (D-049). Independent of projects; an item can carry
//! several. Deleting a category removes only its assignments.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_NAME: usize = 40;
pub const MAX_ICON: usize = 8;
pub const MAX_CATEGORIES: u32 = 200;
/// Most categories one task or reminder can carry.
pub const MAX_PER_ITEM: usize = 12;
pub const DEFAULT_COLOR: &str = "#3b3fa8";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub name: String,
    /// `#rrggbb`, lowercase.
    pub color: String,
    /// A short glyph (an emoji or a few letters), if any.
    pub icon: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewCategory {
    /// Client-chosen id (offline create); must be a fresh 32-hex id.
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CategoryPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// `null` clears the icon.
    #[serde(default, deserialize_with = "validate::double_option")]
    pub icon: Option<Option<String>>,
}

/// One task's category, as listed with the categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub task_id: String,
    pub category_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryError {
    Invalid(&'static str, &'static str),
    /// A client-supplied id is already in use (a replayed offline create).
    Exists,
    NotFound,
    Backend(String),
}

pub trait CategoryRepository {
    /// All the owner's categories, by name.
    async fn list(&self, owner_id: &str) -> Result<Vec<Category>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Category>, String>;
    async fn insert(&self, c: &Category) -> Result<(), String>;
    async fn update(&self, c: &Category) -> Result<bool, String>;
    /// Delete the category and its task and reminder assignments (one batch).
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    /// Every task assignment of the owner's.
    async fn task_assignments(&self, owner_id: &str) -> Result<Vec<Assignment>, String>;
    async fn task_exists(&self, owner_id: &str, task_id: &str) -> Result<bool, String>;
    /// Replace a task's categories with `ids` (all already checked as the owner's), atomically.
    async fn set_task_categories(
        &self,
        owner_id: &str,
        task_id: &str,
        ids: &[String],
        now: i64,
    ) -> Result<(), String>;
}

/// Categories and the task assignment map (`GET /categories`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listing {
    pub categories: Vec<Category>,
    pub tasks: Vec<Assignment>,
}

fn backend(e: String) -> CategoryError {
    CategoryError::Backend(e)
}

fn name(raw: &str) -> Result<String, CategoryError> {
    validate::line(raw, MAX_NAME).map_err(|w| CategoryError::Invalid("name", w))
}

/// `#rgb` or `#rrggbb`, normalised to lowercase `#rrggbb`.
pub fn color(raw: &str) -> Result<String, CategoryError> {
    let s = raw.trim().to_ascii_lowercase();
    let hex = s.strip_prefix('#').unwrap_or("");
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CategoryError::Invalid("color", "expected #rrggbb"));
    }
    match hex.len() {
        6 => Ok(s),
        3 => Ok(format!(
            "#{}",
            hex.chars().flat_map(|c| [c, c]).collect::<String>()
        )),
        _ => Err(CategoryError::Invalid("color", "expected #rrggbb")),
    }
}

fn icon(raw: Option<String>) -> Result<Option<String>, CategoryError> {
    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => validate::line(s, MAX_ICON)
            .map(Some)
            .map_err(|w| CategoryError::Invalid("icon", w)),
    }
}

/// Check a requested category set against the owner's categories: known ids,
/// no more than [`MAX_PER_ITEM`], duplicates dropped, order kept.
pub fn checked_ids(requested: &[String], owned: &[Category]) -> Result<Vec<String>, CategoryError> {
    let known: HashSet<&str> = owned.iter().map(|c| c.id.as_str()).collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for id in requested {
        if !known.contains(id.as_str()) {
            return Err(CategoryError::Invalid("category_ids", "unknown category"));
        }
        if seen.insert(id.as_str()) {
            out.push(id.clone());
        }
    }
    if out.len() > MAX_PER_ITEM {
        return Err(CategoryError::Invalid(
            "category_ids",
            "at most 12 categories per item",
        ));
    }
    Ok(out)
}

/// The owner's category with this name (case-insensitive), if any.
pub fn by_name<'c>(all: &'c [Category], name: &str) -> Option<&'c Category> {
    let want = name.trim().to_lowercase();
    all.iter().find(|c| c.name.to_lowercase() == want)
}

pub struct CategoryService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: CategoryRepository, E: Entropy + ?Sized> CategoryService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    pub async fn list(&self, p: &Principal) -> Result<Listing, CategoryError> {
        Ok(Listing {
            categories: self.repo.list(&p.user_id).await.map_err(backend)?,
            tasks: self
                .repo
                .task_assignments(&p.user_id)
                .await
                .map_err(backend)?,
        })
    }

    pub async fn get(&self, p: &Principal, id: &str) -> Result<Category, CategoryError> {
        if !is_id(id) {
            return Err(CategoryError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(CategoryError::NotFound)
    }

    pub async fn create(
        &self,
        p: &Principal,
        input: NewCategory,
    ) -> Result<Category, CategoryError> {
        let id = match input.id {
            Some(id) if !is_id(&id) => {
                return Err(CategoryError::Invalid("id", "expected 32 hex characters"))
            }
            Some(id)
                if self
                    .repo
                    .get(&p.user_id, &id)
                    .await
                    .map_err(backend)?
                    .is_some() =>
            {
                return Err(CategoryError::Exists)
            }
            Some(id) => id,
            None => new_id(self.entropy),
        };
        let all = self.repo.list(&p.user_id).await.map_err(backend)?;
        let category = Category {
            id,
            owner_id: p.user_id.clone(),
            name: name(&input.name)?,
            color: color(input.color.as_deref().unwrap_or(DEFAULT_COLOR))?,
            icon: icon(input.icon)?,
            created_at: self.now,
            updated_at: self.now,
        };
        if by_name(&all, &category.name).is_some() {
            return Err(CategoryError::Invalid("name", "already used"));
        }
        if all.len() as u32 >= MAX_CATEGORIES {
            return Err(CategoryError::Invalid(
                "categories",
                "limit of 200 categories reached; delete some first",
            ));
        }
        self.repo.insert(&category).await.map_err(backend)?;
        Ok(category)
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: CategoryPatch,
    ) -> Result<Category, CategoryError> {
        let mut c = self.get(p, id).await?;
        if let Some(n) = patch.name {
            let n = name(&n)?;
            let all = self.repo.list(&p.user_id).await.map_err(backend)?;
            if by_name(&all, &n).is_some_and(|other| other.id != c.id) {
                return Err(CategoryError::Invalid("name", "already used"));
            }
            c.name = n;
        }
        if let Some(col) = patch.color {
            c.color = color(&col)?;
        }
        if let Some(i) = patch.icon {
            c.icon = icon(i)?;
        }
        c.updated_at = self.now;
        if !self.repo.update(&c).await.map_err(backend)? {
            return Err(CategoryError::NotFound);
        }
        Ok(c)
    }

    /// Removes the category and its assignments; tasks and reminders stay.
    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), CategoryError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(CategoryError::NotFound)
        }
    }

    /// Replace a task's categories (idempotent, so offline replays are safe).
    pub async fn set_task(
        &self,
        p: &Principal,
        task_id: &str,
        ids: &[String],
    ) -> Result<Vec<String>, CategoryError> {
        if !is_id(task_id)
            || !self
                .repo
                .task_exists(&p.user_id, task_id)
                .await
                .map_err(backend)?
        {
            return Err(CategoryError::NotFound);
        }
        let owned = self.repo.list(&p.user_id).await.map_err(backend)?;
        let ids = checked_ids(ids, &owned)?;
        self.repo
            .set_task_categories(&p.user_id, task_id, &ids, self.now)
            .await
            .map_err(backend)?;
        Ok(ids)
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::cell::RefCell;

    /// Categories, (owner, assignment) pairs, and the (owner, task id) pairs that exist.
    #[derive(Default)]
    pub struct MemoryCategories(
        pub RefCell<Vec<Category>>,
        pub RefCell<Vec<(String, Assignment)>>,
        pub RefCell<Vec<(String, String)>>,
    );

    impl CategoryRepository for MemoryCategories {
        async fn list(&self, owner_id: &str) -> Result<Vec<Category>, String> {
            let mut v: Vec<Category> = self
                .0
                .borrow()
                .iter()
                .filter(|c| c.owner_id == owner_id)
                .cloned()
                .collect();
            v.sort_by_key(|c| c.name.to_lowercase());
            Ok(v)
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Category>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|c| c.owner_id == owner_id && c.id == id)
                .cloned())
        }
        async fn insert(&self, c: &Category) -> Result<(), String> {
            self.0.borrow_mut().push(c.clone());
            Ok(())
        }
        async fn update(&self, c: &Category) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            match all
                .iter_mut()
                .find(|x| x.owner_id == c.owner_id && x.id == c.id)
            {
                Some(x) => {
                    *x = c.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|c| !(c.owner_id == owner_id && c.id == id));
            self.1
                .borrow_mut()
                .retain(|(o, a)| !(o == owner_id && a.category_id == id));
            Ok(all.len() < before)
        }
        async fn task_assignments(&self, owner_id: &str) -> Result<Vec<Assignment>, String> {
            Ok(self
                .1
                .borrow()
                .iter()
                .filter(|(o, _)| o == owner_id)
                .map(|(_, a)| a.clone())
                .collect())
        }
        async fn task_exists(&self, owner_id: &str, task_id: &str) -> Result<bool, String> {
            Ok(self
                .2
                .borrow()
                .iter()
                .any(|(o, t)| o == owner_id && t == task_id))
        }
        async fn set_task_categories(
            &self,
            owner_id: &str,
            task_id: &str,
            ids: &[String],
            _now: i64,
        ) -> Result<(), String> {
            let mut all = self.1.borrow_mut();
            all.retain(|(o, a)| !(o == owner_id && a.task_id == task_id));
            for id in ids {
                all.push((
                    owner_id.into(),
                    Assignment {
                        task_id: task_id.into(),
                        category_id: id.clone(),
                    },
                ));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryCategories;
    use super::*;
    use crate::entropy::testing::CountingEntropy;
    use crate::test_util::block_on;

    fn owner(id: &str) -> Principal {
        Principal {
            user_id: id.into(),
            method: crate::identity::AuthMethod::Session,
        }
    }

    fn new(name: &str) -> NewCategory {
        NewCategory {
            name: name.into(),
            ..Default::default()
        }
    }

    const TASK: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn create_validates_and_rejects_duplicate_names() {
        let repo = MemoryCategories::default();
        let e = CountingEntropy::default();
        let s = CategoryService::new(&repo, &e, 1);
        let p = owner("u1");
        let c = block_on(s.create(
            &p,
            NewCategory {
                name: " School ".into(),
                color: Some("#ABC".into()),
                icon: Some("📚".into()),
                id: None,
            },
        ))
        .unwrap();
        assert_eq!((c.name.as_str(), c.color.as_str()), ("School", "#aabbcc"));
        assert_eq!(c.icon.as_deref(), Some("📚"));
        assert_eq!(
            block_on(s.create(&p, new("school"))),
            Err(CategoryError::Invalid("name", "already used"))
        );
        assert!(matches!(
            block_on(s.create(&p, new(""))),
            Err(CategoryError::Invalid("name", _))
        ));
        let bad = NewCategory {
            color: Some("red".into()),
            ..new("Red")
        };
        assert!(matches!(
            block_on(s.create(&p, bad)),
            Err(CategoryError::Invalid("color", _))
        ));
        // Another owner may use the same name.
        assert!(block_on(s.create(&owner("u2"), new("School"))).is_ok());
    }

    #[test]
    fn replayed_create_is_exists() {
        let repo = MemoryCategories::default();
        let e = CountingEntropy::default();
        let s = CategoryService::new(&repo, &e, 1);
        let p = owner("u1");
        let input = NewCategory {
            id: Some("a".repeat(32)),
            ..new("Work")
        };
        block_on(s.create(&p, input.clone())).unwrap();
        assert_eq!(block_on(s.create(&p, input)), Err(CategoryError::Exists));
    }

    #[test]
    fn update_renames_recolours_and_clears_icon() {
        let repo = MemoryCategories::default();
        let e = CountingEntropy::default();
        let s = CategoryService::new(&repo, &e, 1);
        let p = owner("u1");
        let a = block_on(s.create(
            &p,
            NewCategory {
                icon: Some("A".into()),
                ..new("A")
            },
        ))
        .unwrap();
        block_on(s.create(&p, new("B"))).unwrap();
        let patch = CategoryPatch {
            name: Some("b".into()),
            ..Default::default()
        };
        assert_eq!(
            block_on(s.update(&p, &a.id, patch)),
            Err(CategoryError::Invalid("name", "already used"))
        );
        let patch = CategoryPatch {
            name: Some("a".into()),
            color: Some("#00FF00".into()),
            icon: Some(None),
        };
        let a2 = block_on(s.update(&p, &a.id, patch)).unwrap();
        assert_eq!(
            (a2.name.as_str(), a2.color.as_str(), a2.icon),
            ("a", "#00ff00", None)
        );
        assert_eq!(
            block_on(s.update(&owner("u2"), &a.id, CategoryPatch::default())),
            Err(CategoryError::NotFound)
        );
    }

    #[test]
    fn task_assignment_checks_ownership_and_delete_keeps_tasks() {
        let repo = MemoryCategories::default();
        repo.2.borrow_mut().push(("u1".into(), TASK.into()));
        let e = CountingEntropy::default();
        let s = CategoryService::new(&repo, &e, 1);
        let p = owner("u1");
        let school = block_on(s.create(&p, new("School"))).unwrap();
        let imp = block_on(s.create(&p, new("Important"))).unwrap();
        let theirs = block_on(s.create(&owner("u2"), new("Theirs"))).unwrap();

        let ids = vec![school.id.clone(), imp.id.clone(), school.id.clone()];
        assert_eq!(
            block_on(s.set_task(&p, TASK, &ids)).unwrap(),
            vec![school.id.clone(), imp.id.clone()]
        );
        // Idempotent replay.
        block_on(s.set_task(&p, TASK, &ids)).unwrap();
        assert_eq!(block_on(s.list(&p)).unwrap().tasks.len(), 2);
        // Another owner's category or task is refused.
        assert!(matches!(
            block_on(s.set_task(&p, TASK, std::slice::from_ref(&theirs.id))),
            Err(CategoryError::Invalid("category_ids", _))
        ));
        assert_eq!(
            block_on(s.set_task(&owner("u2"), TASK, &[])),
            Err(CategoryError::NotFound)
        );
        // Deleting a category removes only its assignment.
        block_on(s.delete(&p, &school.id)).unwrap();
        let l = block_on(s.list(&p)).unwrap();
        assert_eq!(l.categories.len(), 1);
        assert_eq!(
            l.tasks,
            vec![Assignment {
                task_id: TASK.into(),
                category_id: imp.id.clone()
            }]
        );
        assert!(block_on(repo.task_exists("u1", TASK)).unwrap());
    }

    #[test]
    fn checked_ids_caps_per_item() {
        let owned: Vec<Category> = (0..13)
            .map(|i| Category {
                id: format!("{i:032x}"),
                owner_id: "u".into(),
                name: format!("c{i}"),
                color: DEFAULT_COLOR.into(),
                icon: None,
                created_at: 0,
                updated_at: 0,
            })
            .collect();
        let ids: Vec<String> = owned.iter().map(|c| c.id.clone()).collect();
        assert!(checked_ids(&ids[..12], &owned).is_ok());
        assert!(checked_ids(&ids, &owned).is_err());
        assert_eq!(
            by_name(&owned, " C3 ").map(|c| c.id.as_str()),
            Some(ids[3].as_str())
        );
    }
}
