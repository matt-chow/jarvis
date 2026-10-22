-- Migration 0002: owner identity (passkeys, sessions, device API tokens).
-- Times are Unix epoch milliseconds (INTEGER). Secrets are stored only as SHA-256 hex.

CREATE TABLE users (
    id           TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);

CREATE TABLE passkeys (
    id           TEXT PRIMARY KEY NOT NULL,          -- base64url credential id
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    public_key   TEXT NOT NULL,                      -- base64url SPKI DER
    algorithm    INTEGER NOT NULL,                   -- COSE alg (-7 ES256, -257 RS256)
    sign_count   INTEGER NOT NULL DEFAULT 0,
    name         TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER
);
CREATE INDEX passkeys_user ON passkeys (user_id);

CREATE TABLE auth_challenges (
    id           TEXT PRIMARY KEY NOT NULL,
    challenge    TEXT NOT NULL,                      -- base64url
    purpose      TEXT NOT NULL CHECK (purpose IN ('setup', 'login', 'add_passkey')),
    user_id      TEXT,                               -- setup: new user id; add_passkey: owner
    display_name TEXT,                               -- setup only
    expires_at   INTEGER NOT NULL
);
CREATE INDEX auth_challenges_expiry ON auth_challenges (expires_at);

CREATE TABLE sessions (
    token_hash   TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);

CREATE TABLE api_tokens (
    id           TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER
);
CREATE INDEX api_tokens_user ON api_tokens (user_id);

UPDATE system_meta
   SET value = '2', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
