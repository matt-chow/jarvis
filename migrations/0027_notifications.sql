-- Migration 0027: reminder notifications (DECISIONS D-049). Additive only.
-- push_subscriptions: one row per device (Web Push endpoint; no keys are
-- stored because pushes carry no payload). notification_log: one row per
-- logical alert, unique per reminder occurrence and slot, written in the
-- same batch as the scheduler's reminder update. notification_deliveries:
-- one row per alert per device, with retry state.

CREATE TABLE push_subscriptions (
    id         TEXT PRIMARY KEY NOT NULL,
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    endpoint   TEXT NOT NULL UNIQUE,
    label      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_ok_at INTEGER,
    last_error TEXT,
    failures   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX push_subscriptions_owner ON push_subscriptions (owner_id);

CREATE TABLE notification_log (
    id          TEXT PRIMARY KEY NOT NULL,
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    reminder_id TEXT NOT NULL,              -- '' for a test notification
    occurrence  TEXT NOT NULL,
    slot        TEXT NOT NULL,
    due_at      INTEGER NOT NULL,
    state       TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    UNIQUE (owner_id, reminder_id, occurrence, slot)
);
CREATE INDEX notification_log_pending ON notification_log (created_at) WHERE state = 'pending';
CREATE INDEX notification_log_owner ON notification_log (owner_id, created_at);
CREATE INDEX notification_log_reminder ON notification_log (reminder_id);

CREATE TABLE notification_deliveries (
    id              TEXT PRIMARY KEY NOT NULL,
    owner_id        TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    notice_id       TEXT NOT NULL,
    subscription_id TEXT NOT NULL,
    state           TEXT NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    next_try        INTEGER,
    status          INTEGER,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    UNIQUE (notice_id, subscription_id)
);
CREATE INDEX notification_deliveries_due ON notification_deliveries (next_try)
    WHERE state IN ('queued', 'retry');
CREATE INDEX notification_deliveries_device ON notification_deliveries (subscription_id, state);

UPDATE system_meta
   SET value = '27', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
