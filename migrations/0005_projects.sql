-- Migration 0005: projects, and project assignment for top-level tasks.

CREATE TABLE projects (
    id          TEXT PRIMARY KEY NOT NULL,
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL CHECK (status IN ('active', 'archived')),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX projects_owner ON projects (owner_id, status);

ALTER TABLE tasks ADD COLUMN project_id TEXT REFERENCES projects (id) ON DELETE SET NULL;
CREATE INDEX tasks_project ON tasks (project_id);

UPDATE system_meta
   SET value = '5', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
