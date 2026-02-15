//! Session cleanup — ends stale sessions automatically.
//!
//! Runs every 60 seconds. Ends sessions that have been in `pending` or
//! `active` status for longer than 5 minutes without an active WebSocket
//! binding in the connection registry.

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::AppState;

const CHECK_INTERVAL_SECS: u64 = 60;
const STALE_MINUTES: i64 = 5;

pub fn start(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(CHECK_INTERVAL_SECS));

        tracing::info!(
            "Session cleanup worker started (every {}s, stale after {}m)",
            CHECK_INTERVAL_SECS,
            STALE_MINUTES
        );

        loop {
            interval.tick().await;

            let ended: Vec<(uuid::Uuid,)> = match sqlx::query_as(
                r#"
                UPDATE sessions
                SET status = 'ended', ended_at = NOW()
                WHERE status IN ('pending', 'active')
                  AND started_at < NOW() - MAKE_INTERVAL(mins => $1)
                RETURNING id
                "#,
            )
            .bind(STALE_MINUTES)
            .fetch_all(&state.db)
            .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!("Session cleanup query failed: {}", e);
                    continue;
                }
            };

            if !ended.is_empty() {
                tracing::info!(count = ended.len(), "Session cleanup: ended stale sessions");

                for (id,) in &ended {
                    // Unbind the session from the registry
                    state.registry.unbind_session(id);

                    state.registry.broadcast_event(&serde_json::json!({
                        "type": "session.ended",
                        "session_id": id.to_string(),
                        "reason": "stale_cleanup"
                    }));
                }
            }
        }
    })
}
