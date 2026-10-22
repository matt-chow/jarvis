-- Migration 0013: reading progress for library documents (PDF/ePub files).

CREATE TABLE reading_progress (
    owner_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    file_id      TEXT NOT NULL REFERENCES files (id) ON DELETE CASCADE,
    status       TEXT NOT NULL CHECK (status IN ('want', 'reading', 'finished')),
    current_page INTEGER,
    total_pages  INTEGER,
    position     TEXT,
    percent      INTEGER NOT NULL DEFAULT 0 CHECK (percent BETWEEN 0 AND 100),
    started_at   INTEGER,
    finished_at  INTEGER,
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (owner_id, file_id)
);

UPDATE system_meta
   SET value = '13', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
