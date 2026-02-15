//! Scripts API — CRUD for the Toolbox script library.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::middleware::AuthUser;
use crate::AppState;
use sc_common::{AppError, AppResult};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(list_scripts).post(create_script))
        .route("/{id}", patch(update_script).delete(delete_script))
        .with_state(state)
}

// ─── Types ──────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ScriptRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub language: String,
    pub code: String,
    pub folder: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub starred: Option<bool>,
    pub run_count: Option<i32>,
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateScriptRequest {
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub code: String,
    pub folder: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub starred: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScriptRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub code: Option<String>,
    pub folder: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub starred: Option<bool>,
    pub run_count: Option<i32>,
    pub last_run: Option<String>,
}

// ─── Handlers ───────────────────────────────────────────

async fn list_scripts(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<ScriptRow>>> {
    let tenant_id = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();

    let scripts: Vec<ScriptRow> = sqlx::query_as(
        r#"SELECT id, name, description, language, code, folder, tags,
                  starred, run_count, last_run, created_at, updated_at
           FROM scripts
           WHERE tenant_id = $1
           ORDER BY starred DESC, updated_at DESC"#,
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(scripts))
}

async fn create_script(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateScriptRequest>,
) -> AppResult<Json<ScriptRow>> {
    let tenant_id = Uuid::parse_str(&auth.0.tenant_id)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid tenant ID")))?;

    let script: ScriptRow = sqlx::query_as(
        r#"INSERT INTO scripts (tenant_id, name, description, language, code, folder, tags, starred)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING id, name, description, language, code, folder, tags, starred,
                     run_count, last_run, created_at, updated_at"#,
    )
    .bind(tenant_id)
    .bind(&payload.name)
    .bind(payload.description.as_deref().unwrap_or(""))
    .bind(payload.language.as_deref().unwrap_or("bash"))
    .bind(&payload.code)
    .bind(payload.folder.as_deref().unwrap_or("General"))
    .bind(payload.tags.as_ref().unwrap_or(&serde_json::json!([])))
    .bind(payload.starred.unwrap_or(false))
    .fetch_one(&state.db)
    .await?;

    Ok(Json(script))
}

async fn update_script(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateScriptRequest>,
) -> AppResult<Json<ScriptRow>> {
    let script: ScriptRow = sqlx::query_as(
        r#"UPDATE scripts SET
               name = COALESCE($2, name),
               description = COALESCE($3, description),
               language = COALESCE($4, language),
               code = COALESCE($5, code),
               folder = COALESCE($6, folder),
               tags = COALESCE($7, tags),
               starred = COALESCE($8, starred),
               run_count = COALESCE($9, run_count),
               updated_at = NOW()
           WHERE id = $1
           RETURNING id, name, description, language, code, folder, tags, starred,
                     run_count, last_run, created_at, updated_at"#,
    )
    .bind(id)
    .bind(payload.name)
    .bind(payload.description)
    .bind(payload.language)
    .bind(payload.code)
    .bind(payload.folder)
    .bind(payload.tags)
    .bind(payload.starred)
    .bind(payload.run_count)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Script not found".into()))?;

    Ok(Json(script))
}

async fn delete_script(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query("DELETE FROM scripts WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Script not found".into()));
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}
