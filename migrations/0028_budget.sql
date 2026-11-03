-- Migration 0028: budget (DECISIONS D-050). Additive only: new tables.
-- Amounts are integer minor units (cents) in the owner's one currency.

CREATE TABLE budget_settings (
    owner_id   TEXT PRIMARY KEY NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    currency   TEXT NOT NULL DEFAULT 'CAD',
    updated_at INTEGER NOT NULL
);

CREATE TABLE budget_accounts (
    id              TEXT PRIMARY KEY NOT NULL,
    owner_id        TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL,     -- cash | chequing | savings | credit | other
    opening_balance INTEGER NOT NULL DEFAULT 0,
    archived        INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE INDEX budget_accounts_owner ON budget_accounts (owner_id);

CREATE TABLE budget_categories (
    id             TEXT PRIMARY KEY NOT NULL,
    owner_id       TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    kind           TEXT NOT NULL,      -- expense | income
    color          TEXT NOT NULL,      -- #rrggbb
    monthly_budget INTEGER,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);
CREATE INDEX budget_categories_owner ON budget_categories (owner_id);

CREATE TABLE budget_transactions (
    id            TEXT PRIMARY KEY NOT NULL,
    owner_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL,       -- expense | income | transfer
    account_id    TEXT NOT NULL,
    to_account_id TEXT,
    category_id   TEXT,
    amount        INTEGER NOT NULL CHECK (amount > 0),
    date          TEXT NOT NULL,       -- YYYY-MM-DD
    payee         TEXT NOT NULL DEFAULT '',
    note          TEXT NOT NULL DEFAULT '',
    template_id   TEXT,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);
CREATE INDEX budget_transactions_owner_date ON budget_transactions (owner_id, date);

CREATE TABLE budget_templates (
    id            TEXT PRIMARY KEY NOT NULL,
    owner_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL,
    account_id    TEXT NOT NULL,
    to_account_id TEXT,
    category_id   TEXT,
    amount        INTEGER NOT NULL CHECK (amount > 0),
    payee         TEXT NOT NULL DEFAULT '',
    cadence       TEXT NOT NULL,       -- weekly | biweekly | monthly | yearly
    next_date     TEXT NOT NULL,
    anchor_day    INTEGER NOT NULL,
    active        INTEGER NOT NULL DEFAULT 1,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);
CREATE INDEX budget_templates_owner ON budget_templates (owner_id);

UPDATE system_meta
   SET value = '28', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
