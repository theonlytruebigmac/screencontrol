-- ─── Server Instance Identity ─────────────────────────────────
-- Stores server-wide configuration (single-tenant for now).

CREATE TABLE IF NOT EXISTS server_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed the server instance ID on first migration
INSERT INTO server_config (key, value)
VALUES ('instance_id', uuid_generate_v4()::TEXT)
ON CONFLICT (key) DO NOTHING;

-- ─── Agent Update Tracking ───────────────────────────────────

ALTER TABLE agents ADD COLUMN IF NOT EXISTS instance_id TEXT;
ALTER TABLE agents ADD COLUMN IF NOT EXISTS target_version VARCHAR(50);
ALTER TABLE agents ADD COLUMN IF NOT EXISTS update_status VARCHAR(20) DEFAULT 'current';
-- update_status values: 'current', 'pending', 'downloading', 'applied', 'failed'
ALTER TABLE agents ADD COLUMN IF NOT EXISTS last_update_attempt TIMESTAMPTZ;

-- ─── Group Enrollment ID ─────────────────────────────────────
-- Each manually-created group has its own ID (the UUID) that can be
-- passed as a parameter to the installer.  The `enrollment_id` is a
-- short, human-friendly alternative that admins can share.

ALTER TABLE agent_groups ADD COLUMN IF NOT EXISTS enrollment_id VARCHAR(32) UNIQUE;

-- Generate enrollment IDs for existing groups (8-char hex)
UPDATE agent_groups
   SET enrollment_id = LEFT(REPLACE(id::TEXT, '-', ''), 8)
 WHERE enrollment_id IS NULL;
