//! Sessions API — create, list, join sessions.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use prost::Message as ProstMessage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::middleware::AuthUser;
use crate::AppState;
use sc_common::{AppError, AppResult};
use sc_protocol::{envelope, Envelope, SessionEnd, SessionRequest};

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(list_sessions).post(create_session))
        .route("/{id}", get(get_session))
        .route("/{id}/end", post(end_session))
        .route("/{id}/recording-url", get(get_recording_url))
        .route("/{id}/recording-upload-url", post(get_recording_upload_url))
        .with_state(state)
}

// ─── Types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    agent_id: Uuid,
    session_type: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct SessionResponse {
    id: Uuid,
    agent_id: Uuid,
    user_id: Option<Uuid>,
    session_type: String,
    status: String,
    started_at: chrono::DateTime<Utc>,
    ended_at: Option<chrono::DateTime<Utc>>,
}

/// Map session_type string → proto enum value.
fn session_type_to_proto(st: &str) -> i32 {
    match st {
        "terminal" => 2,
        "desktop" => 1,
        "file_transfer" => 3,
        "chat" => 4,
        _ => 0,
    }
}

// ─── Handlers ────────────────────────────────────────────────

async fn create_session(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateSessionRequest>,
) -> AppResult<Json<SessionResponse>> {
    let id = Uuid::new_v4();
    let now = Utc::now();

    let user_id = Uuid::parse_str(&claims.sub).ok();

    let session: SessionResponse = sqlx::query_as(
        r#"
        INSERT INTO sessions (id, agent_id, user_id, session_type, status, started_at)
        VALUES ($1, $2, $3, $4, 'pending', $5)
        RETURNING id, agent_id, user_id, session_type, status, started_at, ended_at
        "#,
    )
    .bind(id)
    .bind(payload.agent_id)
    .bind(user_id)
    .bind(&payload.session_type)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    // ── Bind session in registry and notify the agent ────────
    if !state.registry.bind_session(id, payload.agent_id) {
        tracing::warn!(%id, %payload.agent_id, "Agent not connected — session created but not bound");
    } else {
        // Send SessionRequest to the agent to spawn the PTY / desktop
        let req_envelope = Envelope {
            id: Uuid::new_v4().to_string(),
            session_id: id.to_string(),
            timestamp: None,
            payload: Some(envelope::Payload::SessionRequest(SessionRequest {
                agent_id: payload.agent_id.to_string(),
                user_id: user_id.map(|u| u.to_string()).unwrap_or_default(),
                session_type: session_type_to_proto(&payload.session_type),
                monitor_index: 0,
            })),
        };
        let mut buf = Vec::new();
        if req_envelope.encode(&mut buf).is_ok() {
            state.registry.send_to_agent(&payload.agent_id, buf);
        }
        tracing::info!(%id, %payload.agent_id, session_type = %payload.session_type, "Session bound and SessionRequest sent");
    }

    // Record audit entry for session creation
    let _ = crate::api::audit::record(
        &state.db,
        user_id,
        "session.create",
        Some("session"),
        Some(id),
        None,
        Some(serde_json::json!({
            "agent_id": payload.agent_id.to_string(),
            "session_type": payload.session_type
        })),
    )
    .await;

    // Broadcast event to UI subscribers
    state.registry.broadcast_event(&serde_json::json!({
        "type": "session.create",
        "session_id": id.to_string(),
        "agent_id": payload.agent_id.to_string(),
        "session_type": payload.session_type
    }));

    Ok(Json(session))
}

async fn list_sessions(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<SessionResponse>>> {
    let sessions: Vec<SessionResponse> = sqlx::query_as(
        "SELECT id, agent_id, user_id, session_type, status, started_at, ended_at FROM sessions ORDER BY started_at DESC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(sessions))
}

async fn get_session(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<SessionResponse>> {
    let session: SessionResponse = sqlx::query_as(
        "SELECT id, agent_id, user_id, session_type, status, started_at, ended_at FROM sessions WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Session not found".into()))?;

    Ok(Json(session))
}

async fn end_session(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<SessionResponse>> {
    let now = Utc::now();

    let session: SessionResponse = sqlx::query_as(
        r#"
        UPDATE sessions SET status = 'ended', ended_at = $2
        WHERE id = $1
        RETURNING id, agent_id, user_id, session_type, status, started_at, ended_at
        "#,
    )
    .bind(id)
    .bind(now)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("Session not found".into()))?;

    // Record audit entry for session end
    let user_id = Uuid::parse_str(&_auth.0.sub).ok();
    let _ = crate::api::audit::record(
        &state.db,
        user_id,
        "session.end",
        Some("session"),
        Some(id),
        None,
        Some(serde_json::json!({
            "agent_id": session.agent_id.to_string(),
            "session_type": session.session_type
        })),
    )
    .await;
    // Send SessionEnd to the agent so it stops capture/input injection.
    // Use send_to_agent (not send_to_session_agent) because the session
    // binding may already be removed by the console WS disconnect.
    let end_envelope = Envelope {
        id: Uuid::new_v4().to_string(),
        session_id: id.to_string(),
        timestamp: None,
        payload: Some(envelope::Payload::SessionEnd(SessionEnd {
            reason: "user_disconnected".to_string(),
        })),
    };
    let mut end_buf = Vec::new();
    if end_envelope.encode(&mut end_buf).is_ok() {
        let sent = state.registry.send_to_agent(&session.agent_id, end_buf);
        tracing::info!(%id, agent_id = %session.agent_id, sent, "SessionEnd sent to agent");
    }

    // Clean up session binding from the registry
    state.registry.unbind_session(&id);

    // Broadcast event so UI updates
    state.registry.broadcast_event(&serde_json::json!({
        "type": "session.ended",
        "session_id": id.to_string(),
    }));

    Ok(Json(session))
}

// ─── Recording URLs ──────────────────────────────────────────

/// Get a pre-signed download URL for a session recording.
async fn get_recording_url(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let key = format!("recordings/{}/{}.webm", auth.tenant_id(), id);

    let url = crate::services::s3::presigned_download_url(
        &state.s3,
        &state.config.s3.bucket,
        &key,
        3600, // 1 hour
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(Json(serde_json::json!({ "url": url })))
}

/// Get a pre-signed upload URL so an agent can upload a session recording.
async fn get_recording_upload_url(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let key = format!("recordings/{}/{}.webm", auth.tenant_id(), id);

    let url = crate::services::s3::presigned_upload_url(
        &state.s3,
        &state.config.s3.bucket,
        &key,
        3600, // 1 hour
    )
    .await
    .map_err(AppError::Internal)?;

    // Update the session with the recording key
    let _ = sqlx::query("UPDATE sessions SET recording_url = $2 WHERE id = $1")
        .bind(id)
        .bind(&key)
        .execute(&state.db)
        .await;

    Ok(Json(serde_json::json!({ "url": url, "key": key })))
}
