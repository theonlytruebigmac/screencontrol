//! Health check endpoint.

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn router() -> Router {
    Router::new().route("/health", get(health_check))
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "sc-server",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
