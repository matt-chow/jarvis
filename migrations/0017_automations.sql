-- Migration 0017: automations (owner rules that run a JARVIS command on an event).

CREATE TABLE automations (
    id           TEXT PRIMARY KEY NOT NULL,
    owner_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    trigger_json TEXT NOT NULL,            -- {"kind": "task_completed" | "habit_checked", ...filter}
    command      TEXT NOT NULL,            -- JARVIS template, validated by core
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    last_run_at  INTEGER,
    last_ok      INTEGER,
    last_message TEXT
);
CREATE INDEX automations_owner_created ON automations (owner_id, created_at);

UPDATE system_meta
   SET value = '17', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
