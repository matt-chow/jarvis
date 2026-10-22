-- Migration 0007: journal, one entry per owner per local date.

CREATE TABLE journal_entries (
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    date       TEXT NOT NULL,              -- YYYY-MM-DD
    body       TEXT NOT NULL DEFAULT '',
    mood       INTEGER CHECK (mood BETWEEN 1 AND 5),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,           -- optimistic-concurrency version
    PRIMARY KEY (owner_id, date)
);

UPDATE system_meta
   SET value = '7', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
