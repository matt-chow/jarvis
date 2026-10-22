-- Migration 0011: saved JARVIS commands.

CREATE TABLE jarvis_presets (
    id         TEXT PRIMARY KEY NOT NULL,
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    input      TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX jarvis_presets_owner ON jarvis_presets (owner_id, created_at);

UPDATE system_meta
   SET value = '11', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
