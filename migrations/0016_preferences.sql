-- Migration 0016: per-owner preferences (one JSON document, validated by core).

CREATE TABLE preferences (
    owner_id   TEXT PRIMARY KEY NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    data       TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

UPDATE system_meta
   SET value = '16', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
