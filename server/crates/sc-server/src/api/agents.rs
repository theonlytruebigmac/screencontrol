//! Agent management API — registration, listing, details.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, patch, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::middleware::AuthUser;
use crate::AppState;
use sc_common::{AppError, AppResult};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(list_agents))
        .route("/register", post(register_agent))
        .route("/{id}", get(get_agent).patch(update_agent))
        .route("/{id}/thumbnail", get(get_agent_thumbnail))
        .route("/{id}/thumbnail/upload", put(upload_agent_thumbnail))
        .route_layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10 MB for thumbnail uploads
        .route("/{id}/chat", get(get_agent_chat))
        .with_state(state)
}

// ─── Types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RegisterAgentRequest {
    machine_name: String,
    os: String,
    os_version: String,
    arch: String,
    agent_version: String,
    tenant_token: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct AgentRow {
    id: Uuid,
    machine_name: String,
    os: String,
    os_version: String,
    arch: String,
    agent_version: String,
    status: String,
    last_seen: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
    tags: serde_json::Value,
    admin_notes: String,
    group_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct AgentResponse {
    id: Uuid,
    machine_name: String,
    os: String,
    os_version: String,
    arch: String,
    agent_version: String,
    status: String,
    last_seen: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
    tags: serde_json::Value,
    admin_notes: String,
    // Live metrics (None when agent is offline)
    cpu_usage: Option<f64>,
    memory_used: Option<u64>,
    memory_total: Option<u64>,
    disk_used: Option<u64>,
    disk_total: Option<u64>,
    uptime_secs: Option<u32>,
    ip_address: Option<String>,
    logged_in_user: Option<String>,
    cpu_model: Option<String>,
    group_name: Option<String>,
}

impl AgentResponse {
    fn from_row(row: AgentRow, registry: &crate::ws::registry::ConnectionRegistry) -> Self {
        let metrics = registry.get_agent_metrics(&row.id);
        Self {
            id: row.id,
            machine_name: row.machine_name,
            os: row.os,
            os_version: row.os_version,
            arch: row.arch,
            agent_version: row.agent_version,
            status: row.status,
            last_seen: row.last_seen,
            created_at: row.created_at,
            tags: row.tags,
            admin_notes: row.admin_notes,
            cpu_usage: metrics.as_ref().map(|m| m.cpu_usage),
            memory_used: metrics.as_ref().map(|m| m.memory_used),
            memory_total: metrics.as_ref().map(|m| m.memory_total),
            disk_used: metrics.as_ref().map(|m| m.disk_used),
            disk_total: metrics.as_ref().map(|m| m.disk_total),
            uptime_secs: metrics.as_ref().map(|m| m.uptime_secs),
            ip_address: metrics.as_ref().map(|m| m.ip_address.clone()),
            logged_in_user: metrics
                .as_ref()
                .map(|m| m.logged_in_user.clone())
                .filter(|u| !u.is_empty()),
            cpu_model: metrics.map(|m| m.cpu_model).filter(|c| !c.is_empty()),
            group_name: row.group_name,
        }
    }
}

#[derive(Debug, Deserialize)]
struct UpdateAgentRequest {
    tags: Option<serde_json::Value>,
    admin_notes: Option<String>,
}

// ─── Handlers ────────────────────────────────────────────────

async fn register_agent(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterAgentRequest>,
) -> AppResult<Json<AgentResponse>> {
    // Validate tenant token and get tenant_id
    let tenant_id: (Uuid,) = sqlx::query_as("SELECT id FROM tenants WHERE enrollment_token = $1")
        .bind(&payload.tenant_token)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid tenant token".into()))?;

    let now = Utc::now();
    let id = Uuid::new_v4();

    let agent: AgentRow = sqlx::query_as(
        r#"
        INSERT INTO agents (id, tenant_id, machine_name, os, os_version, arch, agent_version, status, last_seen, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'online', $8, $8)
        ON CONFLICT (tenant_id, machine_name) DO UPDATE SET
            os_version = EXCLUDED.os_version,
            arch = EXCLUDED.arch,
            agent_version = EXCLUDED.agent_version,
            status = 'online',
            last_seen = EXCLUDED.last_seen
        RETURNING id, machine_name, os, os_version, arch, agent_version, status, last_seen, created_at, tags, admin_notes, NULL::VARCHAR as group_name
        "#,
    )
    .bind(id)
    .bind(tenant_id.0)
    .bind(&payload.machine_name)
    .bind(&payload.os)
    .bind(&payload.os_version)
    .bind(&payload.arch)
    .bind(&payload.agent_version)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(AgentResponse::from_row(agent, &state.registry)))
}

async fn list_agents(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<AgentResponse>>> {
    let rows: Vec<AgentRow> = sqlx::query_as(
        "SELECT a.id, a.machine_name, a.os, a.os_version, a.arch, a.agent_version, a.status, a.last_seen, a.created_at, a.tags, a.admin_notes, \
         (SELECT g.name FROM agent_groups g JOIN agent_group_members m ON m.group_id = g.id WHERE m.agent_id = a.id LIMIT 1) as group_name \
         FROM agents a ORDER BY a.last_seen DESC NULLS LAST"
    )
    .fetch_all(&state.db)
    .await?;

    let agents: Vec<AgentResponse> = rows
        .into_iter()
        .map(|row| AgentResponse::from_row(row, &state.registry))
        .collect();

    Ok(Json(agents))
}

async fn get_agent(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<AgentResponse>> {
    let row: AgentRow = sqlx::query_as(
        "SELECT a.id, a.machine_name, a.os, a.os_version, a.arch, a.agent_version, a.status, a.last_seen, a.created_at, a.tags, a.admin_notes, \
         (SELECT g.name FROM agent_groups g JOIN agent_group_members m ON m.group_id = g.id WHERE m.agent_id = a.id LIMIT 1) as group_name \
         FROM agents a WHERE a.id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Agent not found".into()))?;

    Ok(Json(AgentResponse::from_row(row, &state.registry)))
}

async fn update_agent(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateAgentRequest>,
) -> AppResult<Json<AgentResponse>> {
    // Build dynamic update
    let mut sets = Vec::new();
    let mut idx = 2u32; // $1 = id
    if payload.tags.is_some() {
        sets.push(format!("tags = ${}", idx));
        idx += 1;
    }
    if payload.admin_notes.is_some() {
        sets.push(format!("admin_notes = ${}", idx));
    }
    if sets.is_empty() {
        let row: AgentRow = sqlx::query_as(
            "SELECT a.id, a.machine_name, a.os, a.os_version, a.arch, a.agent_version, a.status, a.last_seen, a.created_at, a.tags, a.admin_notes, \
             (SELECT g.name FROM agent_groups g JOIN agent_group_members m ON m.group_id = g.id WHERE m.agent_id = a.id LIMIT 1) as group_name \
             FROM agents a WHERE a.id = $1"
        )
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Agent not found".into()))?;
        return Ok(Json(AgentResponse::from_row(row, &state.registry)));
    }

    let sql = format!(
        "UPDATE agents SET {}, updated_at = NOW() WHERE id = $1 RETURNING id, machine_name, os, os_version, arch, agent_version, status, last_seen, created_at, tags, admin_notes, NULL::VARCHAR as group_name",
        sets.join(", ")
    );

    let mut query = sqlx::query_as::<_, AgentRow>(&sql).bind(id);
    if let Some(ref tags) = payload.tags {
        query = query.bind(tags);
    }
    if let Some(ref notes) = payload.admin_notes {
        query = query.bind(notes);
    }

    let row = query
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Agent not found".into()))?;

    Ok(Json(AgentResponse::from_row(row, &state.registry)))
}

/// GET /agents/{id}/thumbnail — serve the desktop thumbnail image directly
async fn get_agent_thumbnail(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    _user: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let path = thumbnail_path(&id);
    match tokio::fs::read(&path).await {
        Ok(data) => Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=60"),
            ],
            data,
        )
            .into_response()),
        Err(_) => Err(AppError::NotFound("No thumbnail available".into())),
    }
}

/// PUT /agents/{id}/thumbnail/upload — accept a JPEG screenshot and save to disk.
///
/// The agent calls this endpoint directly (no S3 required).
async fn upload_agent_thumbnail(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    if body.is_empty() {
        return Err(AppError::BadRequest("Empty body".into()));
    }
    let dir = thumbnail_dir();
    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        AppError::Internal(anyhow::anyhow!("Failed to create thumbnail dir: {}", e))
    })?;
    let path = thumbnail_path(&id);
    tokio::fs::write(&path, &body)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to write thumbnail: {}", e)))?;
    tracing::info!(agent_id = %id, bytes = body.len(), "Thumbnail saved");
    Ok(StatusCode::OK)
}

/// Directory for locally-stored agent thumbnails.
fn thumbnail_dir() -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp/sc-thumbnails")
}

/// Full path for an agent's thumbnail file.
fn thumbnail_path(agent_id: &Uuid) -> std::path::PathBuf {
    thumbnail_dir().join(format!("{}.jpg", agent_id))
}

// ─── Chat ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatHistoryQuery {
    #[serde(default = "default_chat_limit")]
    limit: i64,
}

fn default_chat_limit() -> i64 {
    50
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct ChatMessageRow {
    id: Uuid,
    session_id: Uuid,
    agent_id: Uuid,
    sender_type: String,
    sender_name: String,
    content: String,
    created_at: DateTime<Utc>,
}

/// GET /agents/{id}/chat — returns recent chat messages for the agent
async fn get_agent_chat(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ChatHistoryQuery>,
) -> AppResult<Json<Vec<ChatMessageRow>>> {
    let rows = sqlx::query_as::<_, ChatMessageRow>(
        "SELECT id, session_id, agent_id, sender_type, sender_name, content, created_at
         FROM chat_messages
         WHERE agent_id = $1
         ORDER BY created_at DESC
         LIMIT $2",
    )
    .bind(id)
    .bind(query.limit)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(rows))
}
