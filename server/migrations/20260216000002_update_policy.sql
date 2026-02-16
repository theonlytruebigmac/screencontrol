-- ─── Update Policy Settings ──────────────────────────────────
-- Stored in server_config as JSON so we don't need a separate table.

INSERT INTO server_config (key, value)
VALUES ('update_policy', '{
    "mode": "automatic",
    "maintenance_window_start": null,
    "maintenance_window_end": null,
    "rollout_percentage": 100,
    "auto_update_enabled": true
}')
ON CONFLICT (key) DO NOTHING;
