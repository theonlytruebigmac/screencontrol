-- ─── Settings (key-value per tenant) ─────────────────────────

CREATE TABLE IF NOT EXISTS settings (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    category TEXT NOT NULL,
    key TEXT NOT NULL,
    value JSONB NOT NULL DEFAULT '{}',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, category, key)
);

CREATE INDEX idx_settings_tenant_category ON settings(tenant_id, category);

-- ─── API Keys ────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    key_prefix VARCHAR(12) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    revoked BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX idx_api_keys_tenant ON api_keys(tenant_id);

-- ─── Agent tags + notes columns ──────────────────────────────

ALTER TABLE agents ADD COLUMN IF NOT EXISTS tags JSONB NOT NULL DEFAULT '[]';
ALTER TABLE agents ADD COLUMN IF NOT EXISTS admin_notes TEXT NOT NULL DEFAULT '';
