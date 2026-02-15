//! Scheduled Tasks API — CRUD for task scheduling.

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
        .route("/", get(list_tasks).post(create_task))
        .route("/{id}", patch(update_task).delete(delete_task))
        .with_state(state)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TaskRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub task_type: String,
    pub target_type: String,
    pub target_value: Option<String>,
    pub schedule: String,
    pub next_run: Option<chrono::DateTime<chrono::Utc>>,
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub config: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub name: String,
    pub description: Option<String>,
    pub task_type: Option<String>,
    pub target_type: Option<String>,
    pub target_value: Option<String>,
    pub schedule: Option<String>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub task_type: Option<String>,
    pub target_type: Option<String>,
    pub target_value: Option<String>,
    pub schedule: Option<String>,
    pub status: Option<String>,
    pub config: Option<serde_json::Value>,
}

async fn list_tasks(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<TaskRow>>> {
    let tenant_id = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();

    let tasks: Vec<TaskRow> = sqlx::query_as(
        r#"SELECT id, name, description, task_type, target_type, target_value,
                  schedule, next_run, last_run, status, config, created_at, updated_at
           FROM scheduled_tasks
           WHERE tenant_id = $1
           ORDER BY created_at DESC"#,
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(tasks))
}

async fn create_task(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTaskRequest>,
) -> AppResult<Json<TaskRow>> {
    let tenant_id = Uuid::parse_str(&auth.0.tenant_id)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid tenant ID")))?;

    let task: TaskRow = sqlx::query_as(
        r#"INSERT INTO scheduled_tasks (tenant_id, name, description, task_type, target_type, target_value, schedule, config)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING id, name, description, task_type, target_type, target_value,
                     schedule, next_run, last_run, status, config, created_at, updated_at"#,
    )
    .bind(tenant_id)
    .bind(&payload.name)
    .bind(payload.description.as_deref().unwrap_or(""))
    .bind(payload.task_type.as_deref().unwrap_or("script"))
    .bind(payload.target_type.as_deref().unwrap_or("group"))
    .bind(payload.target_value.as_deref().unwrap_or(""))
    .bind(payload.schedule.as_deref().unwrap_or("0 * * * *"))
    .bind(payload.config.as_ref().unwrap_or(&serde_json::json!({})))
    .fetch_one(&state.db)
    .await?;

    Ok(Json(task))
}

async fn update_task(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateTaskRequest>,
) -> AppResult<Json<TaskRow>> {
    let task: TaskRow = sqlx::query_as(
        r#"UPDATE scheduled_tasks SET
               name = COALESCE($2, name),
               description = COALESCE($3, description),
               task_type = COALESCE($4, task_type),
               target_type = COALESCE($5, target_type),
               target_value = COALESCE($6, target_value),
               schedule = COALESCE($7, schedule),
               status = COALESCE($8, status),
               config = COALESCE($9, config),
               updated_at = NOW()
           WHERE id = $1
           RETURNING id, name, description, task_type, target_type, target_value,
                     schedule, next_run, last_run, status, config, created_at, updated_at"#,
    )
    .bind(id)
    .bind(payload.name)
    .bind(payload.description)
    .bind(payload.task_type)
    .bind(payload.target_type)
    .bind(payload.target_value)
    .bind(payload.schedule)
    .bind(payload.status)
    .bind(payload.config)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Task not found".into()))?;

    Ok(Json(task))
}

async fn delete_task(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Task not found".into()));
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}
