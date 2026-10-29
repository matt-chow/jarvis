-- Migration 0023: the unified planner (DECISIONS D-047). Additive only:
-- existing events stay fixed commitments, existing tasks don't repeat.

-- Events: fixed or flexible, the zone their wall-clock times are in, and
-- richer repeats (every N periods, an occurrence limit, skipped dates).
ALTER TABLE events ADD COLUMN fixed INTEGER NOT NULL DEFAULT 1 CHECK (fixed IN (0, 1));
ALTER TABLE events ADD COLUMN timezone TEXT;
ALTER TABLE events ADD COLUMN repeat_interval INTEGER NOT NULL DEFAULT 1
    CHECK (repeat_interval BETWEEN 1 AND 99);
ALTER TABLE events ADD COLUMN repeat_count INTEGER
    CHECK (repeat_count IS NULL OR repeat_count BETWEEN 1 AND 500);
ALTER TABLE events ADD COLUMN skip_dates TEXT NOT NULL DEFAULT '[]';   -- JSON array of YYYY-MM-DD

-- Recurring tasks: completing one creates the next (deterministic id, so never twice).
ALTER TABLE tasks ADD COLUMN repeat TEXT NOT NULL DEFAULT 'none'
    CHECK (repeat IN ('none', 'daily', 'weekly', 'monthly', 'yearly'));
ALTER TABLE tasks ADD COLUMN repeat_until TEXT;

-- Time blocks: scheduled work sessions, optionally for a task (many per task).
CREATE TABLE time_blocks (
    id             TEXT PRIMARY KEY NOT NULL,
    owner_id       TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    task_id        TEXT REFERENCES tasks (id) ON DELETE CASCADE,
    title          TEXT NOT NULL DEFAULT '',
    date           TEXT NOT NULL,             -- YYYY-MM-DD, local wall-clock
    start_time     TEXT NOT NULL,             -- HH:MM
    end_time       TEXT NOT NULL,             -- HH:MM, after start_time (24:00 = midnight)
    timezone       TEXT,
    fixed          INTEGER NOT NULL DEFAULT 0 CHECK (fixed IN (0, 1)),
    status         TEXT NOT NULL DEFAULT 'planned' CHECK (status IN ('planned', 'done', 'skipped')),
    actual_minutes INTEGER CHECK (actual_minutes IS NULL OR actual_minutes BETWEEN 0 AND 1440),
    notes          TEXT NOT NULL DEFAULT '',
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);
CREATE INDEX time_blocks_owner_date ON time_blocks (owner_id, date);
CREATE INDEX time_blocks_task ON time_blocks (task_id);

UPDATE system_meta
   SET value = '23', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
