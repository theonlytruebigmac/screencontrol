//! API Key management — create, list, revoke.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::api::middleware::AuthUser;
use crate::AppState;
use sc_common::AppResult;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(list_keys).post(create_key))
        .route("/{id}", delete(revoke_key))
        .with_state(state)
}

// ─── Types ───────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
struct ApiKeyRow {
    id: Uuid,
    name: String,
    key_prefix: String,
    created_at: chrono::DateTime<Utc>,
    last_used_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct CreateApiKeyResponse {
    id: Uuid,
    name: String,
    key: String, // full key — shown only once
    key_prefix: String,
    created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
}

// ─── Handlers ────────────────────────────────────────────────

async fn list_keys(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<ApiKeyRow>>> {
    let tenant_id = auth.tenant_id();

    let rows: Vec<ApiKeyRow> = sqlx::query_as(
        "SELECT id, name, key_prefix, created_at, last_used_at FROM api_keys WHERE tenant_id = $1 AND revoked = false ORDER BY created_at DESC"
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}

async fn create_key(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> AppResult<Json<CreateApiKeyResponse>> {
    let tenant_id = auth.tenant_id();
    let user_id = auth.user_id();

    // Generate a random API key: sc_<32 hex chars>
    let raw_key = format!("sc_{}", hex::encode(Uuid::new_v4().as_bytes()));
    let prefix = raw_key[..12].to_string();

    // Hash for storage
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    let id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO api_keys (id, tenant_id, user_id, name, key_hash, key_prefix, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(id)
    .bind(tenant_id)
    .bind(user_id)
    .bind(&payload.name)
    .bind(&key_hash)
    .bind(&prefix)
    .bind(now)
    .execute(&state.db)
    .await?;

    Ok(Json(CreateApiKeyResponse {
        id,
        name: payload.name,
        key: raw_key,
        key_prefix: prefix,
        created_at: now,
    }))
}

async fn revoke_key(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let tenant_id = auth.tenant_id();

    sqlx::query("UPDATE api_keys SET revoked = true WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(tenant_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "status": "revoked" })))
}
