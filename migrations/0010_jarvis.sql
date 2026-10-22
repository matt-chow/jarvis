-- Migration 0010: JARVIS plans (preview → approve → execute) and their history.

CREATE TABLE jarvis_plans (
    id                TEXT PRIMARY KEY NOT NULL,
    owner_id          TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    input             TEXT NOT NULL,
    actions           TEXT NOT NULL,       -- JSON: PlannedAction[]
    status            TEXT NOT NULL CHECK (status IN ('pending', 'running', 'executed', 'failed', 'cancelled', 'expired')),
    requires_approval INTEGER NOT NULL,    -- 0/1
    results           TEXT NOT NULL DEFAULT '[]', -- JSON: ActionResult[]
    created_at        INTEGER NOT NULL,
    expires_at        INTEGER NOT NULL
);
CREATE INDEX jarvis_plans_owner_created ON jarvis_plans (owner_id, created_at);

UPDATE system_meta
   SET value = '10', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
