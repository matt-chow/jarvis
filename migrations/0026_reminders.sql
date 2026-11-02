-- Migration 0026: reminders (DECISIONS D-049). Additive only.
-- A reminder's due_date is its current occurrence; resolved occurrences
-- live in reminder_occurrences, and every action in reminder_history.
-- Links to tasks, projects, and lists are plain ids: deleting one of those
-- clears the link in the same batch (no reliance on cascades).

CREATE TABLE reminder_lists (
    id         TEXT PRIMARY KEY NOT NULL,
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX reminder_lists_owner_name ON reminder_lists (owner_id, name COLLATE NOCASE);

CREATE TABLE reminders (
    id                 TEXT PRIMARY KEY NOT NULL,
    owner_id           TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title              TEXT NOT NULL,
    notes              TEXT NOT NULL DEFAULT '',
    links              TEXT NOT NULL DEFAULT '[]',     -- JSON array of URLs
    due_date           TEXT,                           -- YYYY-MM-DD, local
    due_time           TEXT,                           -- HH:MM, local
    timezone           TEXT NOT NULL DEFAULT 'UTC',
    list_id            TEXT,
    priority           TEXT NOT NULL DEFAULT 'none',
    flagged            INTEGER NOT NULL DEFAULT 0,
    project_id         TEXT,
    task_id            TEXT,
    complete_with_task INTEGER NOT NULL DEFAULT 0,
    alerts             TEXT NOT NULL DEFAULT '[]',     -- JSON
    recurrence         TEXT,                           -- JSON rule
    advance            TEXT NOT NULL DEFAULT 'calendar',
    missed             TEXT NOT NULL DEFAULT 'one',
    checklist          TEXT NOT NULL DEFAULT '[]',     -- JSON
    snoozed_until      INTEGER,
    completed_ahead    TEXT NOT NULL DEFAULT '[]',     -- JSON dates
    backlog            INTEGER NOT NULL DEFAULT 0,
    completed_at       INTEGER,
    deleted_at         INTEGER,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    version            INTEGER NOT NULL DEFAULT 1,
    field_versions     TEXT NOT NULL DEFAULT '{}',     -- JSON field -> version
    due_at             INTEGER,
    notified_through   INTEGER NOT NULL DEFAULT 0,
    wake_at            INTEGER,
    write_id           TEXT NOT NULL DEFAULT ''        -- guards a batch's dependent rows
);
CREATE INDEX reminders_owner ON reminders (owner_id, due_at);
CREATE INDEX reminders_wake ON reminders (wake_at) WHERE wake_at IS NOT NULL;
CREATE INDEX reminders_task ON reminders (owner_id, task_id) WHERE task_id IS NOT NULL;

CREATE TABLE reminder_categories (
    reminder_id TEXT NOT NULL REFERENCES reminders (id) ON DELETE CASCADE,
    category_id TEXT NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (reminder_id, category_id)
);
CREATE INDEX reminder_categories_owner ON reminder_categories (owner_id, category_id);

CREATE TABLE reminder_occurrences (
    reminder_id TEXT NOT NULL REFERENCES reminders (id) ON DELETE CASCADE,
    occurrence  TEXT NOT NULL,                         -- date, or '-'
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    state       TEXT NOT NULL,                         -- completed | missed
    at          INTEGER NOT NULL,
    PRIMARY KEY (reminder_id, occurrence)
);
CREATE INDEX reminder_occurrences_owner ON reminder_occurrences (owner_id, occurrence);

CREATE TABLE reminder_history (
    id          TEXT PRIMARY KEY NOT NULL,
    reminder_id TEXT NOT NULL REFERENCES reminders (id) ON DELETE CASCADE,
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    occurrence  TEXT NOT NULL,
    action      TEXT NOT NULL,                         -- completed | uncompleted | snoozed | missed
    at          INTEGER NOT NULL,
    until       INTEGER
);
CREATE INDEX reminder_history_reminder ON reminder_history (reminder_id, at);

UPDATE system_meta
   SET value = '26', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
