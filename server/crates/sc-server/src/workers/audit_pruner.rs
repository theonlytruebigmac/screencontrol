//! Audit log pruner — deletes old audit log entries.
//!
//! Runs once daily. Removes audit log entries older than 90 days.

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::AppState;

/// Run once per day (86400 seconds).
const PRUNE_INTERVAL_SECS: u64 = 86400;

/// Keep 90 days of audit logs.
const RETENTION_DAYS: i64 = 90;

pub fn start(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(PRUNE_INTERVAL_SECS));

        tracing::info!(
            "Audit pruner started (every {}h, retention {}d)",
            PRUNE_INTERVAL_SECS / 3600,
            RETENTION_DAYS
        );

        loop {
            interval.tick().await;

            match sqlx::query(
                "DELETE FROM audit_log WHERE created_at < NOW() - MAKE_INTERVAL(days => $1)",
            )
            .bind(RETENTION_DAYS)
            .execute(&state.db)
            .await
            {
                Ok(result) => {
                    let count = result.rows_affected();
                    if count > 0 {
                        tracing::info!(count, "Audit pruner: deleted old entries");
                    }
                }
                Err(e) => {
                    tracing::error!("Audit pruner query failed: {}", e);
                }
            }
        }
    })
}
