-- Migration 0008: universal inbox (quick capture, triaged into other modules).

CREATE TABLE inbox_items (
    id         TEXT PRIMARY KEY NOT NULL,
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    text       TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX inbox_owner_created ON inbox_items (owner_id, created_at);

UPDATE system_meta
   SET value = '8', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
