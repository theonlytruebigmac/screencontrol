//! Agent Groups API — CRUD for organizing agents into logical groups.

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
        .route("/", get(list_groups).post(create_group))
        .route("/{id}", patch(update_group).delete(delete_group))
        .route("/{id}/agents", get(list_group_agents))
        .with_state(state)
}

// ─── Types ──────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
struct GroupRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    filter_criteria: Option<serde_json::Value>,
    agent_count: Option<i64>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateGroupRequest {
    name: String,
    description: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    filter_criteria: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct UpdateGroupRequest {
    name: Option<String>,
    description: Option<String>,
    color: Option<String>,
    icon: Option<String>,
    filter_criteria: Option<serde_json::Value>,
}

// ─── Handlers ───────────────────────────────────────────

async fn list_groups(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<GroupRow>>> {
    let tenant_id = Uuid::parse_str(&auth.0.tenant_id).unwrap_or_default();

    let groups: Vec<GroupRow> = sqlx::query_as(
        r#"SELECT g.id, g.name, g.description, g.color, g.icon, g.filter_criteria,
                  g.created_at,
                  (SELECT COUNT(*) FROM agent_group_members m WHERE m.group_id = g.id) as agent_count
           FROM agent_groups g
           WHERE g.tenant_id = $1
           ORDER BY g.name ASC"#,
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(groups))
}

async fn create_group(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateGroupRequest>,
) -> AppResult<Json<GroupRow>> {
    let tenant_id = Uuid::parse_str(&auth.0.tenant_id)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid tenant ID")))?;

    let group: GroupRow = sqlx::query_as(
        r#"WITH inserted AS (
               INSERT INTO agent_groups (tenant_id, name, description, color, icon, filter_criteria)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING *
           )
           SELECT i.id, i.name, i.description, i.color, i.icon, i.filter_criteria, i.created_at,
                  0::bigint as agent_count
           FROM inserted i"#,
    )
    .bind(tenant_id)
    .bind(&payload.name)
    .bind(payload.description.as_deref().unwrap_or(""))
    .bind(payload.color.as_deref().unwrap_or("#e05246"))
    .bind(payload.icon.as_deref().unwrap_or("📁"))
    .bind(
        payload
            .filter_criteria
            .as_ref()
            .unwrap_or(&serde_json::json!([])),
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(group))
}

async fn update_group(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateGroupRequest>,
) -> AppResult<Json<GroupRow>> {
    let group: GroupRow = sqlx::query_as(
        r#"WITH updated AS (
               UPDATE agent_groups SET
                   name = COALESCE($2, name),
                   description = COALESCE($3, description),
                   color = COALESCE($4, color),
                   icon = COALESCE($5, icon),
                   filter_criteria = COALESCE($6, filter_criteria),
                   updated_at = NOW()
               WHERE id = $1
               RETURNING *
           )
           SELECT u.id, u.name, u.description, u.color, u.icon, u.filter_criteria, u.created_at,
                  (SELECT COUNT(*) FROM agent_group_members m WHERE m.group_id = u.id) as agent_count
           FROM updated u"#,
    )
    .bind(id)
    .bind(payload.name)
    .bind(payload.description)
    .bind(payload.color)
    .bind(payload.icon)
    .bind(payload.filter_criteria)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Group not found".into()))?;

    Ok(Json(group))
}

async fn delete_group(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let result = sqlx::query("DELETE FROM agent_groups WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Group not found".into()));
    }

    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn list_group_agents(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Vec<Uuid>>> {
    let agents: Vec<(Uuid,)> =
        sqlx::query_as("SELECT agent_id FROM agent_group_members WHERE group_id = $1")
            .bind(id)
            .fetch_all(&state.db)
            .await?;

    Ok(Json(agents.into_iter().map(|a| a.0).collect()))
}
