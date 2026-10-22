//! Owner identity: passkey (WebAuthn) sign-in, sessions, and device API tokens.
//!
//! Policy (DECISIONS D-009):
//! - One owner. The first passkey is registered with the one-time setup token
//!   while no owner exists; later passkeys require a signed-in session.
//! - Browsers hold an opaque session token in an HttpOnly cookie; native
//!   clients use revocable `dir_…` bearer tokens. Only SHA-256 hashes are stored.

pub mod webauthn;

use crate::encoding::{b64url_decode, b64url_encode, constant_time_eq, hex, sha256_hex};
use serde::{Deserialize, Serialize};
use webauthn::{Assertion, Registration, RelyingParty, WebAuthnError};

pub const SESSION_TTL_MS: i64 = 30 * DAY_MS;
pub const SESSION_MAX_AGE_MS: i64 = 90 * DAY_MS;
const SESSION_REFRESH_AFTER_MS: i64 = DAY_MS;
pub const CHALLENGE_TTL_MS: i64 = 5 * 60 * 1000;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
pub const API_TOKEN_PREFIX: &str = "dir_";
/// Shortest setup secret accepted; weaker values disable setup (fail closed).
pub const MIN_SETUP_TOKEN_LEN: usize = 32;
const MAX_NAME_LEN: usize = 64;

/// Source of cryptographically secure random bytes (injected per runtime).
pub trait Entropy {
    fn fill(&self, buf: &mut [u8]);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct User {
    pub id: String,
    pub display_name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passkey {
    /// base64url credential id.
    pub id: String,
    pub user_id: String,
    pub public_key: Vec<u8>,
    pub algorithm: i64,
    pub sign_count: u32,
    pub name: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengePurpose {
    Setup {
        user_id: String,
        display_name: String,
    },
    Login,
    AddPasskey {
        user_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub id: String,
    pub challenge: Vec<u8>,
    pub purpose: ChallengePurpose,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub token_hash: String,
    pub user_id: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiToken {
    pub id: String,
    #[serde(skip)]
    pub user_id: String,
    pub name: String,
    #[serde(skip)]
    pub token_hash: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

/// Persistence for identity data. Errors are backend descriptions (logged, never shown).
pub trait IdentityRepository {
    async fn owner_exists(&self) -> Result<bool, String>;
    async fn get_user(&self, id: &str) -> Result<Option<User>, String>;
    /// Insert owner and first passkey atomically; `false` if an owner already exists.
    async fn create_owner(&self, user: &User, passkey: &Passkey) -> Result<bool, String>;
    async fn add_passkey(&self, passkey: &Passkey) -> Result<(), String>;
    async fn find_passkey(&self, id: &str) -> Result<Option<Passkey>, String>;
    async fn list_passkeys(&self, user_id: &str) -> Result<Vec<Passkey>, String>;
    async fn record_passkey_use(&self, id: &str, sign_count: u32, at: i64) -> Result<(), String>;
    async fn delete_passkey(&self, user_id: &str, id: &str) -> Result<bool, String>;
    /// Store a challenge and purge expired ones.
    async fn save_challenge(&self, challenge: &Challenge, now: i64) -> Result<(), String>;
    /// Remove and return a challenge (single use).
    async fn take_challenge(&self, id: &str) -> Result<Option<Challenge>, String>;
    async fn create_session(&self, session: &Session) -> Result<(), String>;
    async fn find_session(&self, token_hash: &str) -> Result<Option<Session>, String>;
    async fn touch_session(
        &self,
        token_hash: &str,
        last_seen_at: i64,
        expires_at: i64,
    ) -> Result<(), String>;
    async fn delete_session(&self, token_hash: &str) -> Result<(), String>;
    async fn create_api_token(&self, token: &ApiToken) -> Result<(), String>;
    async fn find_api_token(&self, token_hash: &str) -> Result<Option<ApiToken>, String>;
    async fn touch_api_token(&self, id: &str, at: i64) -> Result<(), String>;
    async fn list_api_tokens(&self, user_id: &str) -> Result<Vec<ApiToken>, String>;
    async fn delete_api_token(&self, user_id: &str, id: &str) -> Result<bool, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// Relying party origins not configured: auth fails closed.
    NotConfigured,
    SetupUnavailable,
    SetupTokenInvalid,
    InvalidInput(&'static str),
    ChallengeInvalid,
    Verification(WebAuthnError),
    UnknownCredential,
    LastPasskey,
    NotFound,
    Backend(String),
}

impl From<WebAuthnError> for IdentityError {
    fn from(e: WebAuthnError) -> Self {
        Self::Verification(e)
    }
}

fn backend(e: String) -> IdentityError {
    IdentityError::Backend(e)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Session,
    ApiToken,
}

/// The authenticated caller. Every private handler requires one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub user_id: String,
    pub method: AuthMethod,
}

pub struct IdentityConfig {
    pub rp: RelyingParty,
    pub setup_token: Option<String>,
}

impl IdentityConfig {
    fn require_rp(&self) -> Result<&RelyingParty, IdentityError> {
        if self.rp.id.is_empty() || self.rp.origins.is_empty() {
            Err(IdentityError::NotConfigured)
        } else {
            Ok(&self.rp)
        }
    }
}

// ---- Wire types (JSON shapes for the browser WebAuthn API) ----

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationOptions {
    pub challenge_id: String,
    pub public_key: PublicKeyCreation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyCreation {
    pub challenge: String,
    pub rp: RpEntity,
    pub user: UserEntity,
    pub pub_key_cred_params: Vec<CredParam>,
    pub authenticator_selection: AuthenticatorSelection,
    pub exclude_credentials: Vec<CredentialDescriptor>,
    pub attestation: &'static str,
    pub timeout: i64,
}

#[derive(Debug, Serialize)]
pub struct RpEntity {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserEntity {
    pub id: String,
    pub name: String,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
pub struct CredParam {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub alg: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatorSelection {
    pub resident_key: &'static str,
    pub require_resident_key: bool,
    pub user_verification: &'static str,
}

#[derive(Debug, Serialize)]
pub struct CredentialDescriptor {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestOptions {
    pub challenge_id: String,
    pub public_key: PublicKeyRequest,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyRequest {
    pub challenge: String,
    pub rp_id: String,
    pub allow_credentials: Vec<CredentialDescriptor>,
    pub user_verification: &'static str,
    pub timeout: i64,
}

/// Registration result as sent by the browser (binary fields base64url).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationInput {
    pub challenge_id: String,
    pub credential_id: String,
    pub client_data_json: String,
    pub authenticator_data: String,
    pub public_key: String,
    pub algorithm: i64,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertionInput {
    pub challenge_id: String,
    pub credential_id: String,
    pub client_data_json: String,
    pub authenticator_data: String,
    pub signature: String,
    #[serde(default)]
    pub user_handle: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PasskeyInfo {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

impl From<&Passkey> for PasskeyInfo {
    fn from(p: &Passkey) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            created_at: p.created_at,
            last_used_at: p.last_used_at,
        }
    }
}

/// A newly issued session. `token` goes into the cookie and is never stored.
#[derive(Debug)]
pub struct IssuedSession {
    pub user: User,
    pub token: String,
    pub expires_at: i64,
}

fn decode(field: &'static str, value: &str) -> Result<Vec<u8>, IdentityError> {
    b64url_decode(value).map_err(|_| IdentityError::InvalidInput(field))
}

/// Trim and bound a user-supplied label.
pub fn clean_name(raw: Option<&str>, default: &str) -> Result<String, IdentityError> {
    let name = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default);
    if name.chars().count() > MAX_NAME_LEN || name.chars().any(char::is_control) {
        return Err(IdentityError::InvalidInput("name"));
    }
    Ok(name.to_owned())
}

impl RegistrationInput {
    fn decode(&self) -> Result<Registration, IdentityError> {
        Ok(Registration {
            credential_id: decode("credentialId", &self.credential_id)?,
            client_data_json: decode("clientDataJson", &self.client_data_json)?,
            authenticator_data: decode("authenticatorData", &self.authenticator_data)?,
            public_key: decode("publicKey", &self.public_key)?,
            algorithm: self.algorithm,
        })
    }
}

impl AssertionInput {
    fn decode(&self) -> Result<Assertion, IdentityError> {
        Ok(Assertion {
            credential_id: decode("credentialId", &self.credential_id)?,
            client_data_json: decode("clientDataJson", &self.client_data_json)?,
            authenticator_data: decode("authenticatorData", &self.authenticator_data)?,
            signature: decode("signature", &self.signature)?,
            user_handle: self
                .user_handle
                .as_deref()
                .map(|h| decode("userHandle", h))
                .transpose()?,
        })
    }
}

/// Orchestrates identity flows for one request (`now` is fixed per request).
pub struct IdentityService<'a, R, E> {
    repo: &'a R,
    entropy: &'a E,
    config: &'a IdentityConfig,
    now: i64,
}

impl<'a, R: IdentityRepository, E: Entropy> IdentityService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, config: &'a IdentityConfig, now: i64) -> Self {
        Self {
            repo,
            entropy,
            config,
            now,
        }
    }

    fn random(&self, len: usize) -> Vec<u8> {
        let mut buf = vec![0u8; len];
        self.entropy.fill(&mut buf);
        buf
    }

    fn random_id(&self) -> String {
        hex(&self.random(16))
    }

    async fn new_challenge(&self, purpose: ChallengePurpose) -> Result<Challenge, IdentityError> {
        let challenge = Challenge {
            id: self.random_id(),
            challenge: self.random(32),
            purpose,
            expires_at: self.now + CHALLENGE_TTL_MS,
        };
        self.repo
            .save_challenge(&challenge, self.now)
            .await
            .map_err(backend)?;
        Ok(challenge)
    }

    async fn consume_challenge(&self, id: &str) -> Result<Challenge, IdentityError> {
        match self.repo.take_challenge(id).await.map_err(backend)? {
            Some(c) if c.expires_at > self.now => Ok(c),
            _ => Err(IdentityError::ChallengeInvalid),
        }
    }

    fn creation_options(
        &self,
        rp: &RelyingParty,
        challenge: &Challenge,
        user_id: &str,
        display_name: &str,
        exclude: &[Passkey],
    ) -> CreationOptions {
        CreationOptions {
            challenge_id: challenge.id.clone(),
            public_key: PublicKeyCreation {
                challenge: b64url_encode(&challenge.challenge),
                rp: RpEntity {
                    id: rp.id.clone(),
                    name: rp.name.clone(),
                },
                user: UserEntity {
                    id: b64url_encode(user_id.as_bytes()),
                    name: display_name.to_owned(),
                    display_name: display_name.to_owned(),
                },
                pub_key_cred_params: webauthn::SUPPORTED_ALGORITHMS
                    .iter()
                    .map(|&alg| CredParam {
                        kind: "public-key",
                        alg,
                    })
                    .collect(),
                authenticator_selection: AuthenticatorSelection {
                    resident_key: "required",
                    require_resident_key: true,
                    user_verification: "required",
                },
                exclude_credentials: exclude
                    .iter()
                    .map(|p| CredentialDescriptor {
                        kind: "public-key",
                        id: p.id.clone(),
                    })
                    .collect(),
                attestation: "none",
                timeout: CHALLENGE_TTL_MS,
            },
        }
    }

    async fn issue_session(&self, user: User) -> Result<IssuedSession, IdentityError> {
        let token = b64url_encode(&self.random(32));
        let session = Session {
            token_hash: sha256_hex(token.as_bytes()),
            user_id: user.id.clone(),
            created_at: self.now,
            expires_at: self.now + SESSION_TTL_MS,
            last_seen_at: self.now,
        };
        self.repo.create_session(&session).await.map_err(backend)?;
        Ok(IssuedSession {
            user,
            token,
            expires_at: session.expires_at,
        })
    }

    fn new_passkey(
        &self,
        reg: &Registration,
        user_id: &str,
        name: String,
        sign_count: u32,
    ) -> Passkey {
        Passkey {
            id: b64url_encode(&reg.credential_id),
            user_id: user_id.to_owned(),
            public_key: reg.public_key.clone(),
            algorithm: reg.algorithm,
            sign_count,
            name,
            created_at: self.now,
            last_used_at: None,
        }
    }

    /// Whether first-run setup is open (no owner yet).
    pub async fn setup_required(&self) -> Result<bool, IdentityError> {
        Ok(!self.repo.owner_exists().await.map_err(backend)?)
    }

    pub async fn begin_setup(
        &self,
        supplied_token: &str,
        display_name: Option<&str>,
    ) -> Result<CreationOptions, IdentityError> {
        let rp = self.config.require_rp()?;
        let configured = match self.config.setup_token.as_deref().map(str::trim) {
            Some(t) if t.len() >= MIN_SETUP_TOKEN_LEN => t,
            _ => return Err(IdentityError::NotConfigured),
        };
        if !self.setup_required().await? {
            return Err(IdentityError::SetupUnavailable);
        }
        if !constant_time_eq(supplied_token.trim().as_bytes(), configured.as_bytes()) {
            return Err(IdentityError::SetupTokenInvalid);
        }
        let display_name = clean_name(display_name, "Owner")?;
        let user_id = self.random_id();
        let challenge = self
            .new_challenge(ChallengePurpose::Setup {
                user_id: user_id.clone(),
                display_name: display_name.clone(),
            })
            .await?;
        Ok(self.creation_options(rp, &challenge, &user_id, &display_name, &[]))
    }

    pub async fn finish_setup(
        &self,
        input: &RegistrationInput,
    ) -> Result<IssuedSession, IdentityError> {
        let rp = self.config.require_rp()?;
        let reg = input.decode()?;
        let challenge = self.consume_challenge(&input.challenge_id).await?;
        let ChallengePurpose::Setup {
            user_id,
            display_name,
        } = challenge.purpose
        else {
            return Err(IdentityError::ChallengeInvalid);
        };
        let sign_count = webauthn::verify_registration(&reg, &challenge.challenge, rp)?;
        let user = User {
            id: user_id,
            display_name,
            created_at: self.now,
        };
        let name = clean_name(input.name.as_deref(), "Passkey")?;
        let passkey = self.new_passkey(&reg, &user.id, name, sign_count);
        if !self
            .repo
            .create_owner(&user, &passkey)
            .await
            .map_err(backend)?
        {
            return Err(IdentityError::SetupUnavailable);
        }
        self.issue_session(user).await
    }

    pub async fn begin_login(&self) -> Result<RequestOptions, IdentityError> {
        let rp = self.config.require_rp()?;
        let challenge = self.new_challenge(ChallengePurpose::Login).await?;
        Ok(RequestOptions {
            challenge_id: challenge.id.clone(),
            public_key: PublicKeyRequest {
                challenge: b64url_encode(&challenge.challenge),
                rp_id: rp.id.clone(),
                // Empty list: discoverable credentials (passkey picker).
                allow_credentials: vec![],
                user_verification: "required",
                timeout: CHALLENGE_TTL_MS,
            },
        })
    }

    pub async fn finish_login(
        &self,
        input: &AssertionInput,
    ) -> Result<IssuedSession, IdentityError> {
        let rp = self.config.require_rp()?;
        let assertion = input.decode()?;
        let challenge = self.consume_challenge(&input.challenge_id).await?;
        if challenge.purpose != ChallengePurpose::Login {
            return Err(IdentityError::ChallengeInvalid);
        }
        let passkey = self
            .repo
            .find_passkey(&b64url_encode(&assertion.credential_id))
            .await
            .map_err(backend)?
            .ok_or(IdentityError::UnknownCredential)?;
        if let Some(handle) = &assertion.user_handle {
            if handle.as_slice() != passkey.user_id.as_bytes() {
                return Err(IdentityError::UnknownCredential);
            }
        }
        let sign_count = webauthn::verify_assertion(
            &assertion,
            &challenge.challenge,
            rp,
            passkey.algorithm,
            &passkey.public_key,
            passkey.sign_count,
        )?;
        self.repo
            .record_passkey_use(&passkey.id, sign_count, self.now)
            .await
            .map_err(backend)?;
        let user = self
            .repo
            .get_user(&passkey.user_id)
            .await
            .map_err(backend)?
            .ok_or(IdentityError::UnknownCredential)?;
        self.issue_session(user).await
    }

    pub async fn begin_add_passkey(
        &self,
        principal: &Principal,
    ) -> Result<CreationOptions, IdentityError> {
        let rp = self.config.require_rp()?;
        let user = self
            .repo
            .get_user(&principal.user_id)
            .await
            .map_err(backend)?
            .ok_or(IdentityError::NotFound)?;
        let existing = self.repo.list_passkeys(&user.id).await.map_err(backend)?;
        let challenge = self
            .new_challenge(ChallengePurpose::AddPasskey {
                user_id: user.id.clone(),
            })
            .await?;
        Ok(self.creation_options(rp, &challenge, &user.id, &user.display_name, &existing))
    }

    pub async fn finish_add_passkey(
        &self,
        principal: &Principal,
        input: &RegistrationInput,
    ) -> Result<PasskeyInfo, IdentityError> {
        let rp = self.config.require_rp()?;
        let reg = input.decode()?;
        let challenge = self.consume_challenge(&input.challenge_id).await?;
        match &challenge.purpose {
            ChallengePurpose::AddPasskey { user_id } if *user_id == principal.user_id => {}
            _ => return Err(IdentityError::ChallengeInvalid),
        }
        let sign_count = webauthn::verify_registration(&reg, &challenge.challenge, rp)?;
        let name = clean_name(input.name.as_deref(), "Passkey")?;
        let passkey = self.new_passkey(&reg, &principal.user_id, name, sign_count);
        self.repo.add_passkey(&passkey).await.map_err(backend)?;
        Ok(PasskeyInfo::from(&passkey))
    }

    pub async fn list_passkeys(
        &self,
        principal: &Principal,
    ) -> Result<Vec<PasskeyInfo>, IdentityError> {
        let list = self
            .repo
            .list_passkeys(&principal.user_id)
            .await
            .map_err(backend)?;
        Ok(list.iter().map(PasskeyInfo::from).collect())
    }

    /// Delete a passkey; the last one cannot be removed (would lock the owner out).
    pub async fn delete_passkey(
        &self,
        principal: &Principal,
        id: &str,
    ) -> Result<(), IdentityError> {
        let list = self
            .repo
            .list_passkeys(&principal.user_id)
            .await
            .map_err(backend)?;
        if !list.iter().any(|p| p.id == id) {
            return Err(IdentityError::NotFound);
        }
        if list.len() <= 1 {
            return Err(IdentityError::LastPasskey);
        }
        self.repo
            .delete_passkey(&principal.user_id, id)
            .await
            .map_err(backend)?;
        Ok(())
    }

    /// Resolve a session cookie value; refreshes sliding expiry at most daily.
    pub async fn authenticate_session(
        &self,
        token: &str,
    ) -> Result<Option<Principal>, IdentityError> {
        let hash = sha256_hex(token.as_bytes());
        let Some(session) = self.repo.find_session(&hash).await.map_err(backend)? else {
            return Ok(None);
        };
        if session.expires_at <= self.now {
            self.repo.delete_session(&hash).await.map_err(backend)?;
            return Ok(None);
        }
        if self.now - session.last_seen_at >= SESSION_REFRESH_AFTER_MS {
            let expires = (self.now + SESSION_TTL_MS).min(session.created_at + SESSION_MAX_AGE_MS);
            self.repo
                .touch_session(&hash, self.now, expires)
                .await
                .map_err(backend)?;
        }
        Ok(Some(Principal {
            user_id: session.user_id,
            method: AuthMethod::Session,
        }))
    }

    pub async fn logout(&self, token: &str) -> Result<(), IdentityError> {
        self.repo
            .delete_session(&sha256_hex(token.as_bytes()))
            .await
            .map_err(backend)
    }

    pub async fn current_user(&self, principal: &Principal) -> Result<User, IdentityError> {
        self.repo
            .get_user(&principal.user_id)
            .await
            .map_err(backend)?
            .ok_or(IdentityError::NotFound)
    }

    /// Issue a device token. The secret is returned once and never stored.
    pub async fn create_api_token(
        &self,
        principal: &Principal,
        name: Option<&str>,
    ) -> Result<(ApiToken, String), IdentityError> {
        let secret = format!("{API_TOKEN_PREFIX}{}", b64url_encode(&self.random(32)));
        let token = ApiToken {
            id: self.random_id(),
            user_id: principal.user_id.clone(),
            name: clean_name(name, "Device")?,
            token_hash: sha256_hex(secret.as_bytes()),
            created_at: self.now,
            last_used_at: None,
        };
        self.repo.create_api_token(&token).await.map_err(backend)?;
        Ok((token, secret))
    }

    pub async fn authenticate_api_token(
        &self,
        secret: &str,
    ) -> Result<Option<Principal>, IdentityError> {
        if !secret.starts_with(API_TOKEN_PREFIX) {
            return Ok(None);
        }
        let Some(token) = self
            .repo
            .find_api_token(&sha256_hex(secret.as_bytes()))
            .await
            .map_err(backend)?
        else {
            return Ok(None);
        };
        if token
            .last_used_at
            .is_none_or(|t| self.now - t >= SESSION_REFRESH_AFTER_MS)
        {
            self.repo
                .touch_api_token(&token.id, self.now)
                .await
                .map_err(backend)?;
        }
        Ok(Some(Principal {
            user_id: token.user_id,
            method: AuthMethod::ApiToken,
        }))
    }

    pub async fn list_api_tokens(
        &self,
        principal: &Principal,
    ) -> Result<Vec<ApiToken>, IdentityError> {
        self.repo
            .list_api_tokens(&principal.user_id)
            .await
            .map_err(backend)
    }

    pub async fn delete_api_token(
        &self,
        principal: &Principal,
        id: &str,
    ) -> Result<(), IdentityError> {
        if self
            .repo
            .delete_api_token(&principal.user_id, id)
            .await
            .map_err(backend)?
        {
            Ok(())
        } else {
            Err(IdentityError::NotFound)
        }
    }
}

/// Extract the bearer credential from an `Authorization` header.
pub fn bearer_token(header: Option<&str>) -> Option<&str> {
    let h = header?;
    h.strip_prefix("Bearer ")
        .or_else(|| h.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod memory;
#[cfg(test)]
mod tests;
