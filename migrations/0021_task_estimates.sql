-- Migration 0021: optional time estimate per task, for the planner (D-041).

ALTER TABLE tasks ADD COLUMN estimate_minutes INTEGER
    CHECK (estimate_minutes IS NULL OR estimate_minutes BETWEEN 1 AND 1440);

UPDATE system_meta
   SET value = '21', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
