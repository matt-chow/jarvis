//! Projects: named groups of top-level tasks, owned by a principal.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_NAME: usize = 120;
pub const MAX_DESCRIPTION: usize = 5_000;
pub const MAX_LIST: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Archived,
}

impl ProjectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Project {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub name: String,
    pub description: String,
    pub status: ProjectStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewProject {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<ProjectStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    Invalid(&'static str, &'static str),
    NotFound,
    Backend(String),
}

pub trait ProjectRepository {
    async fn list(
        &self,
        owner_id: &str,
        include_archived: bool,
        limit: u32,
    ) -> Result<Vec<Project>, String>;
    async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Project>, String>;
    async fn insert(&self, project: &Project) -> Result<(), String>;
    async fn update(&self, project: &Project) -> Result<bool, String>;
    /// Delete the project and unassign its tasks (tasks are kept).
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
}

fn backend(e: String) -> ProjectError {
    ProjectError::Backend(e)
}

fn field<T>(name: &'static str, r: Result<T, &'static str>) -> Result<T, ProjectError> {
    r.map_err(|why| ProjectError::Invalid(name, why))
}

pub struct ProjectService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: ProjectRepository, E: Entropy + ?Sized> ProjectService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    pub async fn list(
        &self,
        p: &Principal,
        include_archived: bool,
    ) -> Result<Vec<Project>, ProjectError> {
        self.repo
            .list(&p.user_id, include_archived, MAX_LIST)
            .await
            .map_err(backend)
    }

    pub async fn get(&self, p: &Principal, id: &str) -> Result<Project, ProjectError> {
        if !is_id(id) {
            return Err(ProjectError::NotFound);
        }
        self.repo
            .get(&p.user_id, id)
            .await
            .map_err(backend)?
            .ok_or(ProjectError::NotFound)
    }

    pub async fn create(&self, p: &Principal, input: NewProject) -> Result<Project, ProjectError> {
        let project = Project {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            name: field("name", validate::line(&input.name, MAX_NAME))?,
            description: field(
                "description",
                validate::text(input.description.as_deref().unwrap_or(""), MAX_DESCRIPTION),
            )?,
            status: ProjectStatus::Active,
            created_at: self.now,
            updated_at: self.now,
        };
        self.repo.insert(&project).await.map_err(backend)?;
        Ok(project)
    }

    pub async fn update(
        &self,
        p: &Principal,
        id: &str,
        patch: ProjectPatch,
    ) -> Result<Project, ProjectError> {
        let mut project = self.get(p, id).await?;
        if let Some(n) = patch.name {
            project.name = field("name", validate::line(&n, MAX_NAME))?;
        }
        if let Some(d) = patch.description {
            project.description = field("description", validate::text(&d, MAX_DESCRIPTION))?;
        }
        if let Some(s) = patch.status {
            project.status = s;
        }
        project.updated_at = self.now;
        if !self.repo.update(&project).await.map_err(backend)? {
            return Err(ProjectError::NotFound);
        }
        Ok(project)
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), ProjectError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(ProjectError::NotFound)
        }
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct MemoryProjects(pub RefCell<Vec<Project>>);

    impl ProjectRepository for MemoryProjects {
        async fn list(
            &self,
            owner_id: &str,
            include_archived: bool,
            limit: u32,
        ) -> Result<Vec<Project>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|p| {
                    p.owner_id == owner_id
                        && (include_archived || p.status == ProjectStatus::Active)
                })
                .take(limit as usize)
                .cloned()
                .collect())
        }
        async fn get(&self, owner_id: &str, id: &str) -> Result<Option<Project>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .find(|p| p.owner_id == owner_id && p.id == id)
                .cloned())
        }
        async fn insert(&self, project: &Project) -> Result<(), String> {
            self.0.borrow_mut().push(project.clone());
            Ok(())
        }
        async fn update(&self, project: &Project) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            match all
                .iter_mut()
                .find(|p| p.owner_id == project.owner_id && p.id == project.id)
            {
                Some(p) => {
                    *p = project.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|p| !(p.owner_id == owner_id && p.id == id));
            Ok(all.len() < before)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryProjects;
    use super::*;
    use crate::entropy::testing::CountingEntropy;
    use crate::identity::AuthMethod;
    use crate::test_util::block_on;

    fn owner(id: &str) -> Principal {
        Principal {
            user_id: id.into(),
            method: AuthMethod::Session,
        }
    }

    #[test]
    fn lifecycle_and_archiving() {
        let repo = MemoryProjects::default();
        let e = CountingEntropy::default();
        let svc = ProjectService::new(&repo, &e, 1);
        let me = owner("u1");

        let p = block_on(svc.create(
            &me,
            NewProject {
                name: " Home renovation ".into(),
                description: None,
            },
        ))
        .unwrap();
        assert_eq!(
            (p.name.as_str(), p.status),
            ("Home renovation", ProjectStatus::Active)
        );
        assert_eq!(
            block_on(svc.create(
                &me,
                NewProject {
                    name: "".into(),
                    description: None
                }
            ))
            .unwrap_err(),
            ProjectError::Invalid("name", "required")
        );

        let archived = block_on(svc.update(
            &me,
            &p.id,
            ProjectPatch {
                status: Some(ProjectStatus::Archived),
                ..Default::default()
            },
        ))
        .unwrap();
        assert_eq!(archived.status, ProjectStatus::Archived);
        assert!(block_on(svc.list(&me, false)).unwrap().is_empty());
        assert_eq!(block_on(svc.list(&me, true)).unwrap().len(), 1);

        assert_eq!(
            block_on(svc.get(&owner("u2"), &p.id)).unwrap_err(),
            ProjectError::NotFound
        );
        assert_eq!(
            block_on(svc.delete(&owner("u2"), &p.id)).unwrap_err(),
            ProjectError::NotFound
        );
        block_on(svc.delete(&me, &p.id)).unwrap();
        assert_eq!(
            block_on(svc.get(&me, &p.id)).unwrap_err(),
            ProjectError::NotFound
        );
    }
}
