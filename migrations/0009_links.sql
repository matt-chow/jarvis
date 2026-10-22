-- Migration 0009: links between entities (normalized pair: a < b).

CREATE TABLE links (
    id         TEXT PRIMARY KEY NOT NULL,
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    a_kind     TEXT NOT NULL CHECK (a_kind IN ('journal', 'note', 'project', 'task')),
    a_id       TEXT NOT NULL,
    b_kind     TEXT NOT NULL CHECK (b_kind IN ('journal', 'note', 'project', 'task')),
    b_id       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (owner_id, a_kind, a_id, b_kind, b_id)
);
CREATE INDEX links_a ON links (owner_id, a_kind, a_id);
CREATE INDEX links_b ON links (owner_id, b_kind, b_id);

UPDATE system_meta
   SET value = '9', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
