-- Migration 0003: fixed-window counters for unauthenticated endpoints.

CREATE TABLE rate_limits (
    key          TEXT NOT NULL,       -- e.g. 'auth:<client ip>'
    window_start INTEGER NOT NULL,    -- epoch ms
    count        INTEGER NOT NULL,
    PRIMARY KEY (key, window_start)
);
CREATE INDEX rate_limits_window ON rate_limits (window_start);

UPDATE system_meta
   SET value = '3', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
