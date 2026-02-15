//! Heartbeat monitor — marks agents offline if no heartbeat received.
//!
//! Runs every 30 seconds. Any agent with `status = 'online'` and
//! `last_seen` older than 90 seconds is marked `'offline'` and an
//! event is broadcast to UI subscribers.

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::AppState;

const CHECK_INTERVAL_SECS: u64 = 30;
const TIMEOUT_SECS: i64 = 90;

pub fn start(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(CHECK_INTERVAL_SECS));

        tracing::info!(
            "Heartbeat monitor started (every {}s, timeout {}s)",
            CHECK_INTERVAL_SECS,
            TIMEOUT_SECS
        );

        loop {
            interval.tick().await;

            // Find agents that should be marked offline
            let stale: Vec<(uuid::Uuid, String)> = match sqlx::query_as(
                r#"
                UPDATE agents
                SET status = 'offline', updated_at = NOW()
                WHERE status = 'online'
                  AND last_seen < NOW() - MAKE_INTERVAL(secs => $1)
                RETURNING id, machine_name
                "#,
            )
            .bind(TIMEOUT_SECS)
            .fetch_all(&state.db)
            .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!("Heartbeat monitor query failed: {}", e);
                    continue;
                }
            };

            if !stale.is_empty() {
                tracing::info!(
                    count = stale.len(),
                    "Heartbeat monitor: marked agents offline"
                );

                for (id, name) in &stale {
                    state.registry.broadcast_event(&serde_json::json!({
                        "type": "agent.status",
                        "agent_id": id.to_string(),
                        "machine_name": name,
                        "status": "offline",
                        "reason": "heartbeat_timeout"
                    }));
                }
            }
        }
    })
}
