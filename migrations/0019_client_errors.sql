-- Migration 0019: errors reported by the owner's clients (newest 200 kept per owner).

CREATE TABLE client_errors (
    id          TEXT PRIMARY KEY NOT NULL,
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    at          INTEGER NOT NULL,
    module      TEXT NOT NULL DEFAULT '',
    message     TEXT NOT NULL,
    stack       TEXT NOT NULL DEFAULT '',
    user_agent  TEXT NOT NULL DEFAULT '',
    app_version TEXT NOT NULL DEFAULT ''
);
CREATE INDEX client_errors_owner_at ON client_errors (owner_id, at);

UPDATE system_meta
   SET value = '19', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
