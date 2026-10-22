-- Migration 0006: server-backed notes (replacing the device-only Scratchpad).

CREATE TABLE notes (
    id         TEXT PRIMARY KEY NOT NULL,
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title      TEXT NOT NULL DEFAULT '',   -- empty: derived from the body
    body       TEXT NOT NULL DEFAULT '',
    pinned     INTEGER NOT NULL DEFAULT 0, -- 0/1
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL            -- also the optimistic-concurrency version
);
CREATE INDEX notes_owner_updated ON notes (owner_id, updated_at);

UPDATE system_meta
   SET value = '6', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
