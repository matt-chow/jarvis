-- Migration 0004: tasks with one level of subtasks.

CREATE TABLE tasks (
    id           TEXT PRIMARY KEY NOT NULL,
    owner_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    parent_id    TEXT REFERENCES tasks (id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    notes        TEXT NOT NULL DEFAULT '',
    status       TEXT NOT NULL CHECK (status IN ('todo', 'in_progress', 'done', 'cancelled')),
    priority     TEXT NOT NULL CHECK (priority IN ('none', 'low', 'medium', 'high')),
    due_date     TEXT,                -- YYYY-MM-DD
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    completed_at INTEGER
);
CREATE INDEX tasks_owner_status ON tasks (owner_id, status);
CREATE INDEX tasks_parent ON tasks (parent_id);

UPDATE system_meta
   SET value = '4', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
