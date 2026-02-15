//! Settings API — key-value config persistence per tenant.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::middleware::AuthUser;
use crate::AppState;
use sc_common::AppResult;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(list_settings).put(upsert_setting))
        .with_state(state)
}

// ─── Types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SettingsQuery {
    category: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct SettingRow {
    id: Uuid,
    category: String,
    key: String,
    value: serde_json::Value,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct UpsertSettingRequest {
    category: String,
    key: String,
    value: serde_json::Value,
}

// ─── Handlers ────────────────────────────────────────────────

async fn list_settings(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(params): Query<SettingsQuery>,
) -> AppResult<Json<Vec<SettingRow>>> {
    let tenant_id = auth.tenant_id();

    let rows: Vec<SettingRow> = if let Some(category) = params.category {
        sqlx::query_as(
            "SELECT id, category, key, value, updated_at FROM settings WHERE tenant_id = $1 AND category = $2 ORDER BY key"
        )
        .bind(tenant_id)
        .bind(&category)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, category, key, value, updated_at FROM settings WHERE tenant_id = $1 ORDER BY category, key"
        )
        .bind(tenant_id)
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(rows))
}

async fn upsert_setting(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpsertSettingRequest>,
) -> AppResult<Json<SettingRow>> {
    let tenant_id = auth.tenant_id();

    let row: SettingRow = sqlx::query_as(
        r#"
        INSERT INTO settings (tenant_id, category, key, value, updated_at)
        VALUES ($1, $2, $3, $4, NOW())
        ON CONFLICT (tenant_id, category, key) DO UPDATE SET
            value = EXCLUDED.value,
            updated_at = NOW()
        RETURNING id, category, key, value, updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(&payload.category)
    .bind(&payload.key)
    .bind(&payload.value)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(row))
}
