//! API routes module.

mod agents;
mod api_keys;
pub mod audit;
pub mod auth;
mod downloads;
mod groups;
mod health;
mod installer;
pub mod middleware;
mod schedules;
mod scripts;
mod sessions;
mod settings;
mod stats;
mod users;

use crate::AppState;
use axum::Router;
use std::sync::Arc;

/// Build the API router with all sub-routes.
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/auth", auth::router(state.clone()))
        .nest("/agents", agents::router(state.clone()))
        .nest("/sessions", sessions::router(state.clone()))
        .nest("/stats", stats::router(state.clone()))
        .nest("/audit", audit::router(state.clone()))
        .nest("/users", users::router(state.clone()))
        .nest("/groups", groups::router(state.clone()))
        .nest("/scripts", scripts::router(state.clone()))
        .nest("/schedules", schedules::router(state.clone()))
        .nest("/settings", settings::router(state.clone()))
        .nest("/api-keys", api_keys::router(state.clone()))
        .nest("/downloads", downloads::router())
        .nest("/installer", installer::router(state.clone()))
        .merge(health::router())
}
