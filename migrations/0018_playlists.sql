-- Migration 0018: music playlists (ordered audio file ids as a JSON array).

CREATE TABLE playlists (
    id          TEXT PRIMARY KEY NOT NULL,
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    tracks_json TEXT NOT NULL DEFAULT '[]', -- file ids; deleted files are skipped on read
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX playlists_owner_created ON playlists (owner_id, created_at);

UPDATE system_meta
   SET value = '18', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
