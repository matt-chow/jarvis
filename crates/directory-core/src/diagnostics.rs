//! Client error reports (DECISIONS D-027): what crashed in the owner's
//! browsers, kept small and bounded so problems are visible without a paid
//! monitoring service.

use crate::entropy::{new_id, Entropy};
use crate::identity::Principal;
use serde::{Deserialize, Serialize};

/// Reports kept per owner (older ones are pruned on each new report).
pub const KEEP: u32 = 200;
pub const LIST_LIMIT: u32 = 50;
pub const MAX_MESSAGE: usize = 1_000;
pub const MAX_STACK: usize = 4_000;
const MAX_SHORT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientError {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub at: i64,
    pub module: String,
    pub message: String,
    pub stack: String,
    pub user_agent: String,
    pub app_version: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewClientError {
    pub message: String,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticsError {
    Invalid(&'static str, &'static str),
    Backend(String),
}

pub trait DiagnosticsRepository {
    async fn insert(&self, e: &ClientError) -> Result<(), String>;
    /// Delete all but the newest `keep` reports.
    async fn prune(&self, owner_id: &str, keep: u32) -> Result<(), String>;
    /// Newest first.
    async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<ClientError>, String>;
    async fn clear(&self, owner_id: &str) -> Result<(), String>;
}

fn backend(e: String) -> DiagnosticsError {
    DiagnosticsError::Backend(e)
}

/// Trim, cap length, and drop control characters other than newlines/tabs.
fn clean(s: &str, max: usize) -> String {
    s.trim()
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(max)
        .collect()
}

pub struct DiagnosticsService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: DiagnosticsRepository, E: Entropy + ?Sized> DiagnosticsService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    pub async fn record(
        &self,
        p: &Principal,
        input: NewClientError,
        user_agent: &str,
    ) -> Result<ClientError, DiagnosticsError> {
        let message = clean(&input.message, MAX_MESSAGE);
        if message.is_empty() {
            return Err(DiagnosticsError::Invalid("message", "required"));
        }
        let e = ClientError {
            id: new_id(self.entropy),
            owner_id: p.user_id.clone(),
            at: self.now,
            module: clean(input.module.as_deref().unwrap_or(""), MAX_SHORT),
            message,
            stack: clean(input.stack.as_deref().unwrap_or(""), MAX_STACK),
            user_agent: clean(user_agent, MAX_SHORT),
            app_version: clean(input.app_version.as_deref().unwrap_or(""), MAX_SHORT),
        };
        self.repo.insert(&e).await.map_err(backend)?;
        self.repo.prune(&p.user_id, KEEP).await.map_err(backend)?;
        Ok(e)
    }

    pub async fn list(&self, p: &Principal) -> Result<Vec<ClientError>, DiagnosticsError> {
        self.repo
            .list(&p.user_id, LIST_LIMIT)
            .await
            .map_err(backend)
    }

    pub async fn clear(&self, p: &Principal) -> Result<(), DiagnosticsError> {
        self.repo.clear(&p.user_id).await.map_err(backend)
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
    struct Memory(RefCell<Vec<ClientError>>);

    impl DiagnosticsRepository for Memory {
        async fn insert(&self, e: &ClientError) -> Result<(), String> {
            self.0.borrow_mut().push(e.clone());
            Ok(())
        }
        async fn prune(&self, owner_id: &str, keep: u32) -> Result<(), String> {
            let mut all = self.0.borrow_mut();
            let mut mine: Vec<i64> = all
                .iter()
                .filter(|e| e.owner_id == owner_id)
                .map(|e| e.at)
                .collect();
            mine.sort_unstable_by(|a, b| b.cmp(a));
            if let Some(&cutoff) = mine.get(keep as usize - 1) {
                all.retain(|e| e.owner_id != owner_id || e.at >= cutoff);
            }
            Ok(())
        }
        async fn list(&self, owner_id: &str, limit: u32) -> Result<Vec<ClientError>, String> {
            let mut v: Vec<ClientError> = self
                .0
                .borrow()
                .iter()
                .filter(|e| e.owner_id == owner_id)
                .cloned()
                .collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.at));
            v.truncate(limit as usize);
            Ok(v)
        }
        async fn clear(&self, owner_id: &str) -> Result<(), String> {
            self.0.borrow_mut().retain(|e| e.owner_id != owner_id);
            Ok(())
        }
    }

    fn me() -> Principal {
        Principal {
            user_id: "u1".into(),
            method: AuthMethod::Session,
        }
    }

    #[test]
    fn reports_are_cleaned_bounded_and_listed_newest_first() {
        let repo = Memory::default();
        let e = CountingEntropy::default();
        let report = |at: i64, message: &str| {
            block_on(DiagnosticsService::new(&repo, &e, at).record(
                &me(),
                NewClientError {
                    message: message.into(),
                    module: Some("notes".into()),
                    stack: Some("x".repeat(MAX_STACK + 50)),
                    ..Default::default()
                },
                "Edge\u{0007}",
            ))
        };
        let first = report(1, "  TypeError: boom\u{0000}  ").unwrap();
        assert_eq!(first.message, "TypeError: boom");
        assert_eq!(first.stack.chars().count(), MAX_STACK);
        assert_eq!(first.user_agent, "Edge");
        assert_eq!(
            report(2, "   ").unwrap_err(),
            DiagnosticsError::Invalid("message", "required")
        );

        for at in 2..=(KEEP as i64 + 5) {
            report(at, &format!("error {at}")).unwrap();
        }
        assert_eq!(repo.0.borrow().len(), KEEP as usize, "oldest pruned");
        let svc = DiagnosticsService::new(&repo, &e, 0);
        let listed = block_on(svc.list(&me())).unwrap();
        assert_eq!(listed.len(), LIST_LIMIT as usize);
        assert_eq!(listed[0].message, format!("error {}", KEEP + 5));
        block_on(svc.clear(&me())).unwrap();
        assert!(block_on(svc.list(&me())).unwrap().is_empty());
    }
}
