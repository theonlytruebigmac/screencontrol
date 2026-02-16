//! Agent Update Policy API — controls how/when agents receive updates.
//!
//! GET  /api/admin/update-policy  — read current policy
//! PUT  /api/admin/update-policy  — update policy
//!
//! The policy is stored in `server_config` (server-wide, not per-tenant)
//! because updates apply to the entire fleet in single-tenant mode.

use axum::{extract::State, http::StatusCode, routing, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

/// Agent update policy controlling when and how updates are pushed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePolicy {
    /// `"automatic"` — push via heartbeat hints as soon as available.
    /// `"manual"` — admin must explicitly trigger updates.
    pub mode: String,

    /// Optional maintenance window start (HH:MM, 24-hour, server-local time).
    /// If set, automatic updates are only pushed within this window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maintenance_window_start: Option<String>,

    /// Optional maintenance window end (HH:MM, 24-hour, server-local time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maintenance_window_end: Option<String>,

    /// Percentage of agents to update per heartbeat cycle (1–100).
    pub rollout_percentage: u32,

    /// Master switch. If false, no update hints are ever sent.
    pub auto_update_enabled: bool,
}

impl Default for UpdatePolicy {
    fn default() -> Self {
        Self {
            mode: "automatic".to_string(),
            maintenance_window_start: None,
            maintenance_window_end: None,
            rollout_percentage: 100,
            auto_update_enabled: true,
        }
    }
}

impl UpdatePolicy {
    /// Should the server send update hints right now?
    pub fn should_push_updates(&self) -> bool {
        if !self.auto_update_enabled || self.mode != "automatic" {
            return false;
        }

        // Check maintenance window if configured
        if let (Some(start), Some(end)) =
            (&self.maintenance_window_start, &self.maintenance_window_end)
        {
            let now = chrono::Local::now().format("%H:%M").to_string();
            if start <= end {
                // Normal window (e.g. 02:00–06:00)
                return now >= *start && now <= *end;
            } else {
                // Overnight window (e.g. 22:00–06:00)
                return now >= *start || now <= *end;
            }
        }

        true
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/update-policy", routing::get(get_policy).put(set_policy))
        .with_state(state)
}

/// Load the current update policy from the database.
pub async fn load_policy(db: &sqlx::PgPool) -> UpdatePolicy {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT value FROM server_config WHERE key = 'update_policy'")
            .fetch_optional(db)
            .await
            .ok()
            .flatten();

    match row {
        Some((json,)) => serde_json::from_str(&json).unwrap_or_default(),
        None => UpdatePolicy::default(),
    }
}

// ─── Handlers ────────────────────────────────────────────────

async fn get_policy(State(state): State<Arc<AppState>>) -> Result<Json<UpdatePolicy>, StatusCode> {
    let policy = load_policy(&state.db).await;
    Ok(Json(policy))
}

async fn set_policy(
    State(state): State<Arc<AppState>>,
    Json(policy): Json<UpdatePolicy>,
) -> Result<Json<UpdatePolicy>, StatusCode> {
    // Validate
    if policy.rollout_percentage == 0 || policy.rollout_percentage > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !["automatic", "manual"].contains(&policy.mode.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let json = serde_json::to_string(&policy).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query(
        "INSERT INTO server_config (key, value, updated_at) \
         VALUES ('update_policy', $1, NOW()) \
         ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()",
    )
    .bind(&json)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(mode = %policy.mode, rollout = policy.rollout_percentage, "Update policy saved");
    Ok(Json(policy))
}
