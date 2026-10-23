-- Migration 0020: R2 objects whose delete failed, retried later (D-033).

CREATE TABLE blob_orphans (
    owner_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    key      TEXT NOT NULL,
    at       INTEGER NOT NULL,
    PRIMARY KEY (owner_id, key)
);

UPDATE system_meta
   SET value = '20', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
