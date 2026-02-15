-- Agent Groups — logical grouping of agents for bulk operations
CREATE TABLE agent_groups (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT DEFAULT '',
    color VARCHAR(20) DEFAULT '#e05246',
    icon VARCHAR(10) DEFAULT '📁',
    filter_criteria JSONB DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_agent_groups_tenant ON agent_groups(tenant_id);

-- Many-to-many: an agent can belong to multiple groups
CREATE TABLE agent_group_members (
    group_id UUID NOT NULL REFERENCES agent_groups(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (group_id, agent_id)
);
