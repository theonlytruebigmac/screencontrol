//! Audit log API — paginated query and insert helpers.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::middleware::AuthUser;
use crate::AppState;
use sc_common::AppResult;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(list_audit_log))
        .with_state(state)
}

// ─── Types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AuditQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    action: Option<String>,
    user_id: Option<Uuid>,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditEntryRow {
    id: Uuid,
    user_id: Option<Uuid>,
    action: String,
    target_type: Option<String>,
    target_id: Option<Uuid>,
    ip_address: Option<String>,
    metadata: Option<serde_json::Value>,
    created_at: chrono::DateTime<Utc>,
}

// ─── Handlers ────────────────────────────────────────────────

async fn list_audit_log(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<AuditQuery>,
) -> AppResult<Json<Vec<AuditEntryRow>>> {
    let limit = q.limit.min(200).max(1);
    let offset = q.offset.max(0);

    let entries: Vec<AuditEntryRow> = if let Some(action) = &q.action {
        sqlx::query_as(
            r#"SELECT id, user_id, action, target_type, target_id,
                      host(ip_address)::text AS ip_address, metadata, created_at
               FROM audit_log
               WHERE action = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(action)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    } else if let Some(uid) = q.user_id {
        sqlx::query_as(
            r#"SELECT id, user_id, action, target_type, target_id,
                      host(ip_address)::text AS ip_address, metadata, created_at
               FROM audit_log
               WHERE user_id = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(uid)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as(
            r#"SELECT id, user_id, action, target_type, target_id,
                      host(ip_address)::text AS ip_address, metadata, created_at
               FROM audit_log
               ORDER BY created_at DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(entries))
}

// ─── Insert helpers (used by other modules) ──────────────────

/// Insert an audit entry. This is called from other API handlers.
pub async fn record(
    db: &sqlx::PgPool,
    user_id: Option<Uuid>,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    ip_address: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO audit_log (user_id, action, target_type, target_id, ip_address, metadata)
           VALUES ($1, $2, $3, $4, $5::inet, $6)"#,
    )
    .bind(user_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(ip_address)
    .bind(metadata.unwrap_or(serde_json::json!({})))
    .execute(db)
    .await?;

    Ok(())
}
