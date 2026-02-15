//! Background workers — periodic tasks running alongside the server.

pub mod audit_pruner;
pub mod heartbeat_monitor;
pub mod session_cleanup;

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::AppState;

/// Start all background worker tasks. Returns handles that can be
/// used to abort them on shutdown.
pub fn start_all_workers(state: Arc<AppState>) -> Vec<JoinHandle<()>> {
    vec![
        heartbeat_monitor::start(state.clone()),
        session_cleanup::start(state.clone()),
        audit_pruner::start(state),
    ]
}
