-- Migration 0015: calendar events. Dates and times are the owner's local
-- wall-clock values (YYYY-MM-DD, HH:MM); no time zone is stored.

CREATE TABLE events (
    id           TEXT PRIMARY KEY NOT NULL,
    owner_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    start_date   TEXT NOT NULL,
    end_date     TEXT NOT NULL,            -- = start_date for single-day events
    start_time   TEXT,                     -- NULL = all day
    end_time     TEXT,
    location     TEXT NOT NULL DEFAULT '',
    notes        TEXT NOT NULL DEFAULT '',
    repeat       TEXT NOT NULL DEFAULT 'none'
                 CHECK (repeat IN ('none', 'daily', 'weekly', 'monthly', 'yearly')),
    repeat_until TEXT,                     -- last possible occurrence start; NULL = forever
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);
CREATE INDEX events_owner_start ON events (owner_id, start_date);

UPDATE system_meta
   SET value = '15', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
