-- Migration 0014: habits (routines on chosen weekdays) and their daily check-ins.

CREATE TABLE habits (
    id         TEXT PRIMARY KEY NOT NULL,
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    days       INTEGER NOT NULL DEFAULT 127 CHECK (days BETWEEN 1 AND 127), -- bit 0 = Monday … bit 6 = Sunday
    archived   INTEGER NOT NULL DEFAULT 0,                                -- 0/1
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX habits_owner_created ON habits (owner_id, created_at);

CREATE TABLE habit_checkins (
    habit_id   TEXT NOT NULL REFERENCES habits (id) ON DELETE CASCADE,
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    date       TEXT NOT NULL, -- the owner's local YYYY-MM-DD
    created_at INTEGER NOT NULL,
    PRIMARY KEY (habit_id, date)
);
CREATE INDEX habit_checkins_owner_date ON habit_checkins (owner_id, date);

UPDATE system_meta
   SET value = '14', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
