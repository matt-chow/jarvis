-- Migration 0022: device token scopes (D-042). Existing tokens keep full access;
-- 'jarvis' tokens may call only the JARVIS ask endpoint (Apple Shortcuts).

ALTER TABLE api_tokens ADD COLUMN scope TEXT NOT NULL DEFAULT 'full'
    CHECK (scope IN ('full', 'jarvis'));

UPDATE system_meta
   SET value = '22', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
 WHERE key = 'schema_version';
