-- Add unique constraint for group name per tenant (needed for ON CONFLICT in auto-assign)
ALTER TABLE agent_groups ADD CONSTRAINT uq_agent_groups_tenant_name UNIQUE (tenant_id, name);
