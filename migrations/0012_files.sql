-- Migration 0012: file metadata (content lives in R2 at u/<owner>/f/<id>).

CREATE TABLE files (
    id           TEXT PRIMARY KEY NOT NULL,
    owner_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    folder       TEXT NOT NULL DEFAULT '',   -- '' = root; 'a/b' nested
    name         TEXT NOT NULL,
    size         INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    UNIQUE (owner_id, folder, name)
);
CREATE INDEX files_owner_folder ON files (owner_id, folder);

UPDATE system_meta
   SET value = '12', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
