//! D1 implementation of `IdentityRepository` (tables from migration 0002).

use super::{changes, err, int, opt_int, opt_text, stmt, text};
use directory_core::encoding::{b64url_decode, b64url_encode};
use directory_core::identity::{
    ApiToken, Challenge, ChallengePurpose, IdentityRepository, Passkey, Session, User,
};
use serde::Deserialize;
use worker::D1Database;

pub struct D1IdentityRepository {
    db: D1Database,
}

impl D1IdentityRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

// Row types: D1 returns numbers as JS doubles.
#[derive(Deserialize)]
struct UserRow {
    id: String,
    display_name: String,
    created_at: f64,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        Self {
            id: r.id,
            display_name: r.display_name,
            created_at: r.created_at as i64,
        }
    }
}

#[derive(Deserialize)]
struct PasskeyRow {
    id: String,
    user_id: String,
    public_key: String,
    algorithm: f64,
    sign_count: f64,
    name: String,
    created_at: f64,
    last_used_at: Option<f64>,
}

impl TryFrom<PasskeyRow> for Passkey {
    type Error = String;
    fn try_from(r: PasskeyRow) -> Result<Self, String> {
        Ok(Self {
            id: r.id,
            user_id: r.user_id,
            public_key: b64url_decode(&r.public_key)
                .map_err(|_| "corrupt public key".to_string())?,
            algorithm: r.algorithm as i64,
            sign_count: r.sign_count as u32,
            name: r.name,
            created_at: r.created_at as i64,
            last_used_at: r.last_used_at.map(|t| t as i64),
        })
    }
}

#[derive(Deserialize)]
struct ChallengeRow {
    id: String,
    challenge: String,
    purpose: String,
    user_id: Option<String>,
    display_name: Option<String>,
    expires_at: f64,
}

impl TryFrom<ChallengeRow> for Challenge {
    type Error = String;
    fn try_from(r: ChallengeRow) -> Result<Self, String> {
        let missing = || "corrupt challenge".to_string();
        let purpose = match r.purpose.as_str() {
            "setup" => ChallengePurpose::Setup {
                user_id: r.user_id.ok_or_else(missing)?,
                display_name: r.display_name.ok_or_else(missing)?,
            },
            "login" => ChallengePurpose::Login,
            "add_passkey" => ChallengePurpose::AddPasskey {
                user_id: r.user_id.ok_or_else(missing)?,
            },
            _ => return Err(missing()),
        };
        Ok(Self {
            id: r.id,
            challenge: b64url_decode(&r.challenge).map_err(|_| missing())?,
            purpose,
            expires_at: r.expires_at as i64,
        })
    }
}

#[derive(Deserialize)]
struct SessionRow {
    token_hash: String,
    user_id: String,
    created_at: f64,
    expires_at: f64,
    last_seen_at: f64,
}

impl From<SessionRow> for Session {
    fn from(r: SessionRow) -> Self {
        Self {
            token_hash: r.token_hash,
            user_id: r.user_id,
            created_at: r.created_at as i64,
            expires_at: r.expires_at as i64,
            last_seen_at: r.last_seen_at as i64,
        }
    }
}

#[derive(Deserialize)]
struct ApiTokenRow {
    id: String,
    user_id: String,
    name: String,
    token_hash: String,
    created_at: f64,
    last_used_at: Option<f64>,
}

impl From<ApiTokenRow> for ApiToken {
    fn from(r: ApiTokenRow) -> Self {
        Self {
            id: r.id,
            user_id: r.user_id,
            name: r.name,
            token_hash: r.token_hash,
            created_at: r.created_at as i64,
            last_used_at: r.last_used_at.map(|t| t as i64),
        }
    }
}

const PASSKEY_COLUMNS: &str =
    "id, user_id, public_key, algorithm, sign_count, name, created_at, last_used_at";

impl IdentityRepository for D1IdentityRepository {
    async fn owner_exists(&self) -> Result<bool, String> {
        let row = stmt(&self.db, "SELECT COUNT(*) AS n FROM users", &[])?
            .first::<f64>(Some("n"))
            .await
            .map_err(err)?;
        Ok(row.unwrap_or(0.0) > 0.0)
    }

    async fn get_user(&self, id: &str) -> Result<Option<User>, String> {
        let row = stmt(
            &self.db,
            "SELECT id, display_name, created_at FROM users WHERE id = ?1",
            &[text(id)],
        )?
        .first::<UserRow>(None)
        .await
        .map_err(err)?;
        Ok(row.map(User::from))
    }

    async fn create_owner(&self, user: &User, passkey: &Passkey) -> Result<bool, String> {
        // One transaction: the user insert is skipped if any owner exists, and the
        // passkey insert only succeeds if that user row was just created.
        let results = self
            .db
            .batch(vec![
                stmt(
                    &self.db,
                    "INSERT INTO users (id, display_name, created_at)
                     SELECT ?1, ?2, ?3 WHERE NOT EXISTS (SELECT 1 FROM users)",
                    &[text(&user.id), text(&user.display_name), int(user.created_at)],
                )?,
                stmt(
                    &self.db,
                    "INSERT INTO passkeys (id, user_id, public_key, algorithm, sign_count, name, created_at)
                     SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7 WHERE EXISTS (SELECT 1 FROM users WHERE id = ?2)",
                    &[
                        text(&passkey.id),
                        text(&passkey.user_id),
                        text(&b64url_encode(&passkey.public_key)),
                        int(passkey.algorithm),
                        int(passkey.sign_count.into()),
                        text(&passkey.name),
                        int(passkey.created_at),
                    ],
                )?,
            ])
            .await
            .map_err(err)?;
        Ok(results.first().is_some_and(|r| changes(r) == 1))
    }

    async fn add_passkey(&self, p: &Passkey) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO passkeys (id, user_id, public_key, algorithm, sign_count, name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[
                text(&p.id),
                text(&p.user_id),
                text(&b64url_encode(&p.public_key)),
                int(p.algorithm),
                int(p.sign_count.into()),
                text(&p.name),
                int(p.created_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn find_passkey(&self, id: &str) -> Result<Option<Passkey>, String> {
        let sql = format!("SELECT {PASSKEY_COLUMNS} FROM passkeys WHERE id = ?1");
        stmt(&self.db, &sql, &[text(id)])?
            .first::<PasskeyRow>(None)
            .await
            .map_err(err)?
            .map(Passkey::try_from)
            .transpose()
    }

    async fn list_passkeys(&self, user_id: &str) -> Result<Vec<Passkey>, String> {
        let sql = format!(
            "SELECT {PASSKEY_COLUMNS} FROM passkeys WHERE user_id = ?1 ORDER BY created_at"
        );
        stmt(&self.db, &sql, &[text(user_id)])?
            .all()
            .await
            .map_err(err)?
            .results::<PasskeyRow>()
            .map_err(err)?
            .into_iter()
            .map(Passkey::try_from)
            .collect()
    }

    async fn record_passkey_use(&self, id: &str, sign_count: u32, at: i64) -> Result<(), String> {
        stmt(
            &self.db,
            "UPDATE passkeys SET sign_count = ?2, last_used_at = ?3 WHERE id = ?1",
            &[text(id), int(sign_count.into()), int(at)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn delete_passkey(&self, user_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM passkeys WHERE user_id = ?1 AND id = ?2",
            &[text(user_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn save_challenge(&self, c: &Challenge, now: i64) -> Result<(), String> {
        let (purpose, user_id, display_name) = match &c.purpose {
            ChallengePurpose::Setup {
                user_id,
                display_name,
            } => ("setup", Some(user_id.as_str()), Some(display_name.as_str())),
            ChallengePurpose::Login => ("login", None, None),
            ChallengePurpose::AddPasskey { user_id } => {
                ("add_passkey", Some(user_id.as_str()), None)
            }
        };
        self.db
            .batch(vec![
                stmt(&self.db, "DELETE FROM auth_challenges WHERE expires_at <= ?1", &[int(now)])?,
                stmt(
                    &self.db,
                    "INSERT INTO auth_challenges (id, challenge, purpose, user_id, display_name, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    &[
                        text(&c.id),
                        text(&b64url_encode(&c.challenge)),
                        text(purpose),
                        opt_text(user_id),
                        opt_text(display_name),
                        int(c.expires_at),
                    ],
                )?,
            ])
            .await
            .map_err(err)?;
        Ok(())
    }

    async fn take_challenge(&self, id: &str) -> Result<Option<Challenge>, String> {
        stmt(
            &self.db,
            "DELETE FROM auth_challenges WHERE id = ?1
             RETURNING id, challenge, purpose, user_id, display_name, expires_at",
            &[text(id)],
        )?
        .first::<ChallengeRow>(None)
        .await
        .map_err(err)?
        .map(Challenge::try_from)
        .transpose()
    }

    async fn create_session(&self, s: &Session) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            &[text(&s.token_hash), text(&s.user_id), int(s.created_at), int(s.expires_at), int(s.last_seen_at)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn find_session(&self, token_hash: &str) -> Result<Option<Session>, String> {
        let row = stmt(
            &self.db,
            "SELECT token_hash, user_id, created_at, expires_at, last_seen_at FROM sessions WHERE token_hash = ?1",
            &[text(token_hash)],
        )?
        .first::<SessionRow>(None)
        .await
        .map_err(err)?;
        Ok(row.map(Session::from))
    }

    async fn touch_session(
        &self,
        token_hash: &str,
        last_seen_at: i64,
        expires_at: i64,
    ) -> Result<(), String> {
        stmt(
            &self.db,
            "UPDATE sessions SET last_seen_at = ?2, expires_at = ?3 WHERE token_hash = ?1",
            &[text(token_hash), int(last_seen_at), int(expires_at)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn delete_session(&self, token_hash: &str) -> Result<(), String> {
        stmt(
            &self.db,
            "DELETE FROM sessions WHERE token_hash = ?1",
            &[text(token_hash)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn create_api_token(&self, t: &ApiToken) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO api_tokens (id, user_id, name, token_hash, created_at, last_used_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[text(&t.id), text(&t.user_id), text(&t.name), text(&t.token_hash), int(t.created_at), opt_int(t.last_used_at)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn find_api_token(&self, token_hash: &str) -> Result<Option<ApiToken>, String> {
        let row = stmt(
            &self.db,
            "SELECT id, user_id, name, token_hash, created_at, last_used_at FROM api_tokens WHERE token_hash = ?1",
            &[text(token_hash)],
        )?
        .first::<ApiTokenRow>(None)
        .await
        .map_err(err)?;
        Ok(row.map(ApiToken::from))
    }

    async fn touch_api_token(&self, id: &str, at: i64) -> Result<(), String> {
        stmt(
            &self.db,
            "UPDATE api_tokens SET last_used_at = ?2 WHERE id = ?1",
            &[text(id), int(at)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn list_api_tokens(&self, user_id: &str) -> Result<Vec<ApiToken>, String> {
        Ok(stmt(
            &self.db,
            "SELECT id, user_id, name, token_hash, created_at, last_used_at FROM api_tokens WHERE user_id = ?1 ORDER BY created_at",
            &[text(user_id)],
        )?
        .all()
        .await
        .map_err(err)?
        .results::<ApiTokenRow>()
        .map_err(err)?
        .into_iter()
        .map(ApiToken::from)
        .collect())
    }

    async fn delete_api_token(&self, user_id: &str, id: &str) -> Result<bool, String> {
        let r = stmt(
            &self.db,
            "DELETE FROM api_tokens WHERE user_id = ?1 AND id = ?2",
            &[text(user_id), text(id)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&r) > 0)
    }
}
