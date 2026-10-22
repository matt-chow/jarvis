//! Saved JARVIS commands. Running a preset is just planning its input, so it
//! goes through the same preview and approval as typed commands.

use super::parser::{self, ParseError};
use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};

pub const MAX_NAME: usize = 60;
pub const MAX_PRESETS: u32 = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preset {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub name: String,
    pub input: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewPreset {
    pub name: String,
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetError {
    Invalid(&'static str, &'static str),
    /// The command text doesn't parse.
    Parse(ParseError),
    Limit,
    NotFound,
    Backend(String),
}

pub trait PresetRepository {
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Preset>, String>;
    async fn count(&self, owner_id: &str) -> Result<u32, String>;
    async fn insert(&self, preset: &Preset) -> Result<(), String>;
    async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String>;
}

fn backend(e: String) -> PresetError {
    PresetError::Backend(e)
}

pub struct PresetService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: PresetRepository, E: Entropy + ?Sized> PresetService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    pub async fn list(&self, p: &Principal) -> Result<Vec<Preset>, PresetError> {
        self.repo
            .list(&p.user_id, MAX_PRESETS)
            .await
            .map_err(backend)
    }

    pub async fn create(&self, p: &Principal, input: NewPreset) -> Result<Preset, PresetError> {
        let name =
            validate::line(&input.name, MAX_NAME).map_err(|w| PresetError::Invalid("name", w))?;
        let text = validate::line(&input.input, super::MAX_INPUT)
            .map_err(|w| PresetError::Invalid("input", w))?;
        // Relative dates resolve when the preset runs; any valid date checks syntax.
        parser::parse(&text, "2000-01-01").map_err(PresetError::Parse)?;
        if self.repo.count(&p.user_id).await.map_err(backend)? >= MAX_PRESETS {
            return Err(PresetError::Limit);
        }
        let preset = Preset {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            name,
            input: text,
            created_at: self.now,
        };
        self.repo.insert(&preset).await.map_err(backend)?;
        Ok(preset)
    }

    pub async fn delete(&self, p: &Principal, id: &str) -> Result<(), PresetError> {
        if is_id(id) && self.repo.delete(&p.user_id, id).await.map_err(backend)? {
            Ok(())
        } else {
            Err(PresetError::NotFound)
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

    #[derive(Default)]
    struct Memory(RefCell<Vec<Preset>>);

    impl PresetRepository for Memory {
        async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<Preset>, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|p| p.owner_id == owner_id)
                .take(limit as usize)
                .cloned()
                .collect())
        }
        async fn count(&self, owner_id: &str) -> Result<u32, String> {
            Ok(self
                .0
                .borrow()
                .iter()
                .filter(|p| p.owner_id == owner_id)
                .count() as u32)
        }
        async fn insert(&self, preset: &Preset) -> Result<(), String> {
            self.0.borrow_mut().push(preset.clone());
            Ok(())
        }
        async fn delete(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            let mut all = self.0.borrow_mut();
            let before = all.len();
            all.retain(|p| !(p.owner_id == owner_id && p.id == id));
            Ok(all.len() < before)
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    fn new(name: &str, input: &str) -> NewPreset {
        NewPreset {
            name: name.into(),
            input: input.into(),
        }
    }

    #[test]
    fn create_validates_and_parses() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let svc = PresetService::new(&repo, &e, 1);
        let p = block_on(svc.create(
            &me(),
            new(" Morning ", "list tasks; journal started the day"),
        ))
        .unwrap();
        assert_eq!(
            (p.name.as_str(), p.input.as_str()),
            ("Morning", "list tasks; journal started the day")
        );
        assert!(matches!(
            block_on(svc.create(&me(), new("Bad", "dance"))),
            Err(PresetError::Parse(_))
        ));
        assert_eq!(
            block_on(svc.create(&me(), new("", "list tasks"))).unwrap_err(),
            PresetError::Invalid("name", "required")
        );
        assert!(matches!(
            block_on(svc.create(&me(), new("Multi", "list\ntasks"))),
            Err(PresetError::Invalid("input", _))
        ));
    }

    #[test]
    fn limit_scope_and_delete() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let svc = PresetService::new(&repo, &e, 1);
        for i in 0..MAX_PRESETS {
            block_on(svc.create(&me(), new(&format!("p{i}"), "list tasks"))).unwrap();
        }
        assert_eq!(
            block_on(svc.create(&me(), new("one more", "list tasks"))).unwrap_err(),
            PresetError::Limit
        );
        let first = block_on(svc.list(&me())).unwrap()[0].clone();
        let other = Principal {
            user_id: "u2".into(),
            method: AuthMethod::Session,
        };
        assert!(block_on(svc.list(&other)).unwrap().is_empty());
        assert_eq!(
            block_on(svc.delete(&other, &first.id)).unwrap_err(),
            PresetError::NotFound
        );
        block_on(svc.delete(&me(), &first.id)).unwrap();
    }
}
