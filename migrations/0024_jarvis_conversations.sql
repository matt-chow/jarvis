-- Migration 0024: JARVIS 2.0 (DECISIONS D-048).
-- Plans record which plan undid them and which pending plan they replaced;
-- conversations hold short-lived, bounded follow-up state (no prompts, no
-- model output beyond the last few turns' text).

ALTER TABLE jarvis_plans ADD COLUMN reversed_by TEXT;
ALTER TABLE jarvis_plans ADD COLUMN revises TEXT;

CREATE TABLE jarvis_conversations (
    id          TEXT PRIMARY KEY NOT NULL,
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    state       TEXT NOT NULL,        -- JSON: conversation::State
    updated_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL
);
CREATE INDEX jarvis_conversations_owner ON jarvis_conversations (owner_id, updated_at);

UPDATE system_meta
   SET value = '24', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
