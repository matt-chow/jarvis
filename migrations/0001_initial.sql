-- Migration 0001: system metadata.
-- Applied by `wrangler d1 migrations apply DB` (tracked in d1_migrations).
-- Migrations are append-only: never edit an applied file; add a new one.

CREATE TABLE IF NOT EXISTS system_meta (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Must match directory_core::system::EXPECTED_SCHEMA_VERSION.
INSERT INTO system_meta (key, value) VALUES ('schema_version', '1')
    ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at;
