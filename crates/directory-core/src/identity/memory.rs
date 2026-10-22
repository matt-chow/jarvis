//! In-memory `IdentityRepository` for tests.

use super::*;
use std::cell::RefCell;

#[derive(Default)]
pub struct MemoryIdentity {
    pub users: RefCell<Vec<User>>,
    pub passkeys: RefCell<Vec<Passkey>>,
    pub challenges: RefCell<Vec<Challenge>>,
    pub sessions: RefCell<Vec<Session>>,
    pub tokens: RefCell<Vec<ApiToken>>,
}

impl IdentityRepository for MemoryIdentity {
    async fn owner_exists(&self) -> Result<bool, String> {
        Ok(!self.users.borrow().is_empty())
    }
    async fn get_user(&self, id: &str) -> Result<Option<User>, String> {
        Ok(self.users.borrow().iter().find(|u| u.id == id).cloned())
    }
    async fn create_owner(&self, user: &User, passkey: &Passkey) -> Result<bool, String> {
        if !self.users.borrow().is_empty() {
            return Ok(false);
        }
        self.users.borrow_mut().push(user.clone());
        self.passkeys.borrow_mut().push(passkey.clone());
        Ok(true)
    }
    async fn add_passkey(&self, passkey: &Passkey) -> Result<(), String> {
        if self.passkeys.borrow().iter().any(|p| p.id == passkey.id) {
            return Err("duplicate credential".into());
        }
        self.passkeys.borrow_mut().push(passkey.clone());
        Ok(())
    }
    async fn find_passkey(&self, id: &str) -> Result<Option<Passkey>, String> {
        Ok(self.passkeys.borrow().iter().find(|p| p.id == id).cloned())
    }
    async fn list_passkeys(&self, user_id: &str) -> Result<Vec<Passkey>, String> {
        Ok(self
            .passkeys
            .borrow()
            .iter()
            .filter(|p| p.user_id == user_id)
            .cloned()
            .collect())
    }
    async fn record_passkey_use(&self, id: &str, sign_count: u32, at: i64) -> Result<(), String> {
        for p in self.passkeys.borrow_mut().iter_mut().filter(|p| p.id == id) {
            p.sign_count = sign_count;
            p.last_used_at = Some(at);
        }
        Ok(())
    }
    async fn delete_passkey(&self, user_id: &str, id: &str) -> Result<bool, String> {
        let mut list = self.passkeys.borrow_mut();
        let before = list.len();
        list.retain(|p| !(p.user_id == user_id && p.id == id));
        Ok(list.len() < before)
    }
    async fn save_challenge(&self, challenge: &Challenge, now: i64) -> Result<(), String> {
        let mut list = self.challenges.borrow_mut();
        list.retain(|c| c.expires_at > now);
        list.push(challenge.clone());
        Ok(())
    }
    async fn take_challenge(&self, id: &str) -> Result<Option<Challenge>, String> {
        let mut list = self.challenges.borrow_mut();
        let pos = list.iter().position(|c| c.id == id);
        Ok(pos.map(|i| list.remove(i)))
    }
    async fn create_session(&self, session: &Session) -> Result<(), String> {
        self.sessions.borrow_mut().push(session.clone());
        Ok(())
    }
    async fn find_session(&self, token_hash: &str) -> Result<Option<Session>, String> {
        Ok(self
            .sessions
            .borrow()
            .iter()
            .find(|s| s.token_hash == token_hash)
            .cloned())
    }
    async fn touch_session(
        &self,
        token_hash: &str,
        last_seen_at: i64,
        expires_at: i64,
    ) -> Result<(), String> {
        for s in self
            .sessions
            .borrow_mut()
            .iter_mut()
            .filter(|s| s.token_hash == token_hash)
        {
            s.last_seen_at = last_seen_at;
            s.expires_at = expires_at;
        }
        Ok(())
    }
    async fn delete_session(&self, token_hash: &str) -> Result<(), String> {
        self.sessions
            .borrow_mut()
            .retain(|s| s.token_hash != token_hash);
        Ok(())
    }
    async fn create_api_token(&self, token: &ApiToken) -> Result<(), String> {
        self.tokens.borrow_mut().push(token.clone());
        Ok(())
    }
    async fn find_api_token(&self, token_hash: &str) -> Result<Option<ApiToken>, String> {
        Ok(self
            .tokens
            .borrow()
            .iter()
            .find(|t| t.token_hash == token_hash)
            .cloned())
    }
    async fn touch_api_token(&self, id: &str, at: i64) -> Result<(), String> {
        for t in self.tokens.borrow_mut().iter_mut().filter(|t| t.id == id) {
            t.last_used_at = Some(at);
        }
        Ok(())
    }
    async fn list_api_tokens(&self, user_id: &str) -> Result<Vec<ApiToken>, String> {
        Ok(self
            .tokens
            .borrow()
            .iter()
            .filter(|t| t.user_id == user_id)
            .cloned()
            .collect())
    }
    async fn delete_api_token(&self, user_id: &str, id: &str) -> Result<bool, String> {
        let mut list = self.tokens.borrow_mut();
        let before = list.len();
        list.retain(|t| !(t.user_id == user_id && t.id == id));
        Ok(list.len() < before)
    }
}

/// Deterministic entropy: an incrementing byte stream.
#[derive(Default)]
pub struct CountingEntropy(pub std::cell::Cell<u8>);

impl Entropy for CountingEntropy {
    fn fill(&self, buf: &mut [u8]) {
        for b in buf {
            let next = self.0.get().wrapping_add(1);
            self.0.set(next);
            *b = next;
        }
    }
}
