-- ScreenControl Database Schema
-- Initial migration: core tables for tenants, users, agents, sessions, and audit.

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ─── Tenants ─────────────────────────────────────────────────

CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    enrollment_token VARCHAR(255) NOT NULL UNIQUE,
    branding JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ─── Users ───────────────────────────────────────────────────

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    email VARCHAR(255) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    role VARCHAR(50) NOT NULL DEFAULT 'technician',
    mfa_secret VARCHAR(255),
    mfa_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_login TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(tenant_id, email)
);

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_tenant ON users(tenant_id);

-- ─── Agents ──────────────────────────────────────────────────

CREATE TABLE agents (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    machine_name VARCHAR(255) NOT NULL,
    os VARCHAR(50) NOT NULL,
    os_version VARCHAR(100) DEFAULT '',
    arch VARCHAR(50) DEFAULT '',
    agent_version VARCHAR(50) DEFAULT '',
    status VARCHAR(20) NOT NULL DEFAULT 'offline',
    last_seen TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(tenant_id, machine_name)
);

CREATE INDEX idx_agents_tenant ON agents(tenant_id);
CREATE INDEX idx_agents_status ON agents(status);
CREATE INDEX idx_agents_last_seen ON agents(last_seen);

-- ─── Session Groups ──────────────────────────────────────────

CREATE TABLE session_groups (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT DEFAULT '',
    filter_criteria JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ─── Sessions ────────────────────────────────────────────────

CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    group_id UUID REFERENCES session_groups(id) ON DELETE SET NULL,
    session_type VARCHAR(50) NOT NULL DEFAULT 'desktop',
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    recording_url TEXT,
    notes TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,

    CONSTRAINT valid_session_type CHECK (session_type IN ('desktop', 'terminal', 'file_transfer', 'chat')),
    CONSTRAINT valid_status CHECK (status IN ('pending', 'active', 'paused', 'ended', 'error'))
);

CREATE INDEX idx_sessions_agent ON sessions(agent_id);
CREATE INDEX idx_sessions_user ON sessions(user_id);
CREATE INDEX idx_sessions_status ON sessions(status);
CREATE INDEX idx_sessions_started ON sessions(started_at DESC);

-- ─── Audit Log ───────────────────────────────────────────────

CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    action VARCHAR(100) NOT NULL,
    target_type VARCHAR(50),
    target_id UUID,
    ip_address INET,
    user_agent TEXT,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_tenant ON audit_log(tenant_id);
CREATE INDEX idx_audit_user ON audit_log(user_id);
CREATE INDEX idx_audit_action ON audit_log(action);
CREATE INDEX idx_audit_created ON audit_log(created_at DESC);

-- ─── API Keys ────────────────────────────────────────────────

CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    key_hash VARCHAR(255) NOT NULL,
    key_prefix VARCHAR(10) NOT NULL,
    scopes JSONB DEFAULT '["read"]',
    last_used TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_api_keys_hash ON api_keys(key_hash);

-- ─── Seed default tenant ─────────────────────────────────────

INSERT INTO tenants (name, enrollment_token) VALUES
    ('Default', 'sc-default-token-change-me');

-- Seed admin user (password: "admin" — CHANGE IN PRODUCTION)
-- Argon2 hash of "admin"
INSERT INTO users (tenant_id, email, password_hash, display_name, role) VALUES
    ((SELECT id FROM tenants WHERE name = 'Default'),
     'admin@screencontrol.local',
     '$argon2id$v=19$m=19456,t=2,p=1$vanphaR/4cr/BpoHhJel8w$l7pVSDttgWOc/mT4wHBK3KRDK31rvrxVo8y7bCtMX2g',
     'Administrator',
     'admin');
