//! WebSocket gateway for real-time agent ↔ console communication.

mod events;
mod handler;
pub mod registry;

use crate::AppState;
use axum::Router;
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/agent", axum::routing::get(handler::agent_ws_handler))
        .route(
            "/console/{session_id}",
            axum::routing::get(handler::console_ws_handler),
        )
        .route("/events", axum::routing::get(events::events_ws_handler))
        .with_state(state)
}
