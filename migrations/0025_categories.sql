-- Migration 0025: categories shared by tasks and reminders (DECISIONS D-049).
-- Additive only: tasks keep their columns; assignments live in a join table.

CREATE TABLE categories (
    id         TEXT PRIMARY KEY NOT NULL,
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL,        -- #rrggbb
    icon       TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX categories_owner_name ON categories (owner_id, name COLLATE NOCASE);

CREATE TABLE task_categories (
    task_id     TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    category_id TEXT NOT NULL REFERENCES categories (id) ON DELETE CASCADE,
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (task_id, category_id)
);
CREATE INDEX task_categories_owner ON task_categories (owner_id, category_id);

UPDATE system_meta
   SET value = '25', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
