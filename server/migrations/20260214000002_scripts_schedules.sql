-- Scripts / Toolbox — saved scripts for remote execution
CREATE TABLE scripts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT DEFAULT '',
    language VARCHAR(20) NOT NULL DEFAULT 'bash',
    code TEXT NOT NULL DEFAULT '',
    folder VARCHAR(100) DEFAULT 'General',
    tags JSONB DEFAULT '[]',
    starred BOOLEAN DEFAULT FALSE,
    run_count INT DEFAULT 0,
    last_run TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_language CHECK (language IN ('powershell', 'bash', 'python', 'batch'))
);

CREATE INDEX idx_scripts_tenant ON scripts(tenant_id);

-- Scheduled Tasks
CREATE TABLE scheduled_tasks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT DEFAULT '',
    task_type VARCHAR(50) NOT NULL DEFAULT 'script',
    target_type VARCHAR(50) NOT NULL DEFAULT 'group',
    target_value VARCHAR(255) DEFAULT '',
    schedule VARCHAR(100) NOT NULL DEFAULT '0 * * * *',
    next_run TIMESTAMPTZ,
    last_run TIMESTAMPTZ,
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    config JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_task_type CHECK (task_type IN ('script', 'patch', 'scan', 'backup', 'restart', 'report')),
    CONSTRAINT valid_task_status CHECK (status IN ('active', 'paused', 'completed', 'error'))
);

CREATE INDEX idx_scheduled_tasks_tenant ON scheduled_tasks(tenant_id);
