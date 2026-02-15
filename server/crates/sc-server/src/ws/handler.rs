//! WebSocket handlers for agent and console connections.
//!
//! Decodes protobuf `Envelope` messages, dispatches by payload type,
//! and routes data through the `ConnectionRegistry`.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use prost::Message as ProstMessage;
use tokio::sync::mpsc;
use uuid::Uuid;

use sc_protocol::{
    envelope, AgentRegistrationAck, Envelope, HeartbeatAck, SessionEnd,
    DEFAULT_HEARTBEAT_INTERVAL_SECS,
};

use crate::AppState;

// ─── Upgrade handlers ────────────────────────────────────────

/// Agent WebSocket upgrade.
pub async fn agent_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_agent_socket(socket, state))
}

/// Console WebSocket upgrade (requires session_id in URL).
pub async fn console_ws_handler(
    ws: WebSocketUpgrade,
    Path(session_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_console_socket(socket, session_id, state))
}

// ─── Agent socket ────────────────────────────────────────────

async fn handle_agent_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Create an mpsc channel so we can send messages from multiple tasks
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Forward channel → WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let mut agent_id: Option<Uuid> = None;

    tracing::info!("Agent WebSocket connected, awaiting registration");

    // Receive loop
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                tracing::info!(bytes = data.len(), "Agent binary message received");
                match Envelope::decode(data.as_ref()) {
                    Ok(envelope) => {
                        let session_id = envelope.session_id.clone();
                        match envelope.payload {
                            // ── Registration ─────────────────────
                            Some(envelope::Payload::AgentRegistration(reg)) => {
                                agent_id = handle_agent_registration(&state, &tx, &reg).await;
                            }

                            // ── Heartbeat ────────────────────────
                            Some(envelope::Payload::Heartbeat(hb)) => {
                                handle_heartbeat(&state, &tx, &hb).await;
                            }

                            // ── Session signaling (relay to console) ──
                            Some(envelope::Payload::SessionOffer(_))
                            | Some(envelope::Payload::SessionAnswer(_))
                            | Some(envelope::Payload::IceCandidate(_)) => {
                                if let Ok(sid) = Uuid::parse_str(&session_id) {
                                    let mut buf = Vec::new();
                                    if Envelope::decode(data.as_ref())
                                        .ok()
                                        .and_then(|e| e.encode(&mut buf).ok())
                                        .is_some()
                                    {
                                        state.registry.send_to_console(&sid, data.to_vec());
                                    }
                                }
                            }

                            // ── Terminal data (relay to console) ──
                            Some(envelope::Payload::TerminalData(_)) => {
                                if let Ok(sid) = Uuid::parse_str(&session_id) {
                                    state.registry.send_to_console(&sid, data.to_vec());
                                }
                            }

                            // ── Screen info (relay to console) ──
                            Some(envelope::Payload::ScreenInfo(_)) => {
                                if let Ok(sid) = Uuid::parse_str(&session_id) {
                                    state.registry.send_to_console(&sid, data.to_vec());
                                }
                            }

                            // ── Desktop frame (relay to console) ──
                            Some(envelope::Payload::DesktopFrame(ref frame)) => {
                                if let Ok(sid) = Uuid::parse_str(&session_id) {
                                    let ok = state.registry.send_to_console(&sid, data.to_vec());
                                    tracing::info!(
                                        %session_id,
                                        frame_seq = frame.sequence,
                                        frame_bytes = data.len(),
                                        relay_ok = ok,
                                        "DesktopFrame relay"
                                    );
                                }
                            }

                            // ── File transfer ack/chunks (relay) ──
                            Some(envelope::Payload::FileTransferAck(_))
                            | Some(envelope::Payload::FileChunk(_))
                            | Some(envelope::Payload::FileList(_)) => {
                                if let Ok(sid) = Uuid::parse_str(&session_id) {
                                    state.registry.send_to_console(&sid, data.to_vec());
                                }
                            }

                            // ── Command response (relay) ─────────
                            Some(envelope::Payload::CommandResponse(_)) => {
                                if let Ok(sid) = Uuid::parse_str(&session_id) {
                                    state.registry.send_to_console(&sid, data.to_vec());
                                }
                            }

                            // ── Chat (relay + persist) ─────────────────────
                            Some(envelope::Payload::ChatMessage(ref msg)) => {
                                if let Ok(sid) = Uuid::parse_str(&session_id) {
                                    // Persist to DB
                                    let _ = sqlx::query(
                                        "INSERT INTO chat_messages (session_id, agent_id, sender_type, sender_name, content)
                                         VALUES ($1, $2, $3, $4, $5)"
                                    )
                                    .bind(sid)
                                    .bind(agent_id)
                                    .bind(&msg.sender_id) // 'agent' or 'agent-user'
                                    .bind(&msg.sender_name)
                                    .bind(&msg.content)
                                    .execute(&state.db)
                                    .await;

                                    state.registry.send_to_console(&sid, data.to_vec());
                                }
                            }

                            // ── Agent info (store logged_in_user) ─
                            Some(envelope::Payload::AgentInfo(ref info)) => {
                                if let Ok(aid) = Uuid::parse_str(&info.agent_id) {
                                    tracing::info!(
                                        agent_id = %aid,
                                        logged_in_user = %info.logged_in_user,
                                        "AgentInfo received"
                                    );
                                    // Update logged_in_user and cpu_model in existing metrics
                                    if let Some(mut existing) =
                                        state.registry.get_agent_metrics(&aid)
                                    {
                                        existing.logged_in_user = info.logged_in_user.clone();
                                        existing.cpu_model = info.cpu_model.clone();
                                        state.registry.update_agent_metrics(&aid, existing);
                                    }
                                }
                            }

                            Some(_) => {
                                tracing::debug!("Unhandled agent payload type");
                            }
                            None => {
                                tracing::warn!("Agent sent envelope with no payload");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to decode agent protobuf: {}", e);
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                let _ = tx.send(Message::Pong(data));
            }
            Ok(Message::Close(_)) => {
                tracing::info!("Agent WebSocket closed");
                break;
            }
            Err(e) => {
                tracing::error!("Agent WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // ── Cleanup ──────────────────────────────────────────────
    send_task.abort();
    if let Some(aid) = agent_id {
        state.registry.unregister_agent(&aid);
        // Mark offline in DB
        let _ = sqlx::query("UPDATE agents SET status = 'offline' WHERE id = $1")
            .bind(aid)
            .execute(&state.db)
            .await;
        tracing::info!(%aid, "Agent disconnected and marked offline");
    }
}

// ─── Console socket ──────────────────────────────────────────

async fn handle_console_socket(socket: WebSocket, session_id: Uuid, state: Arc<AppState>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Attach console to session in registry
    if !state.registry.attach_console(&session_id, tx.clone()) {
        tracing::warn!(%session_id, "Console connected but session not bound — rejecting");
        let _ = ws_sender.send(Message::Close(None)).await;
        return;
    }

    // Forward channel → WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    tracing::info!(%session_id, "Console WebSocket connected");

    // Update session status in DB
    let _ = sqlx::query("UPDATE sessions SET status = 'active' WHERE id = $1")
        .bind(session_id)
        .execute(&state.db)
        .await;

    // Trigger an immediate thumbnail capture when someone connects.
    // Send a HeartbeatAck with the HTTP upload path (agent resolves against its server URL).
    if let Some(binding) = state.registry.sessions.get(&session_id) {
        let agent_id = binding.agent_id;
        let upload_path = format!("/api/agents/{}/thumbnail/upload", agent_id);
        state.registry.mark_thumbnail_sent(&agent_id);
        let ack = Envelope {
            id: Uuid::new_v4().to_string(),
            session_id: String::new(),
            timestamp: None,
            payload: Some(envelope::Payload::HeartbeatAck(HeartbeatAck {
                interval_secs: DEFAULT_HEARTBEAT_INTERVAL_SECS,
                thumbnail_upload_url: upload_path,
            })),
        };
        let mut buf = Vec::new();
        if ack.encode(&mut buf).is_ok() {
            state.registry.send_to_agent(&agent_id, buf);
        }
    }

    // Receive loop
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                match Envelope::decode(data.as_ref()) {
                    Ok(envelope) => {
                        match envelope.payload {
                            // ── Session signaling (relay to agent) ──
                            Some(envelope::Payload::SessionRequest(_))
                            | Some(envelope::Payload::SessionOffer(_))
                            | Some(envelope::Payload::SessionAnswer(_))
                            | Some(envelope::Payload::IceCandidate(_)) => {
                                state
                                    .registry
                                    .send_to_session_agent(&session_id, data.to_vec());
                            }

                            // ── Terminal data/resize (relay to agent) ──
                            Some(envelope::Payload::TerminalData(_))
                            | Some(envelope::Payload::TerminalResize(_)) => {
                                state
                                    .registry
                                    .send_to_session_agent(&session_id, data.to_vec());
                            }

                            // ── Input events (relay to agent) ──
                            Some(envelope::Payload::InputEvent(_)) => {
                                state
                                    .registry
                                    .send_to_session_agent(&session_id, data.to_vec());
                            }

                            // ── File transfer request (intercept to add presigned URL) ──
                            Some(envelope::Payload::FileTransferRequest(ref req)) => {
                                let transfer_id = req.transfer_id.clone();
                                let file_name = req.file_name.clone();
                                let upload = req.upload; // true = console→agent
                                let s3_key = format!(
                                    "transfers/{}/{}",
                                    session_id,
                                    if transfer_id.is_empty() {
                                        Uuid::new_v4().to_string()
                                    } else {
                                        transfer_id.clone()
                                    }
                                );

                                // Generate presigned URLs:
                                //   upload=true  (console→agent): console PUTs (public), agent GETs (internal)
                                //   upload=false (agent→console): agent PUTs (internal), console GETs (public)

                                // Console always gets a public URL; agent always gets an internal URL
                                let console_url_result = if upload {
                                    // Console uploads → console gets public PUT URL
                                    crate::services::s3::presigned_upload_url_public(
                                        &state.s3_public,
                                        &state.config.s3.bucket,
                                        &s3_key,
                                        3600,
                                    )
                                    .await
                                } else {
                                    // Console downloads → console gets public GET URL
                                    crate::services::s3::presigned_download_url_public(
                                        &state.s3_public,
                                        &state.config.s3.bucket,
                                        &s3_key,
                                        3600,
                                    )
                                    .await
                                };

                                let agent_url_result = if upload {
                                    // Agent downloads → agent gets internal GET URL
                                    crate::services::s3::presigned_download_url(
                                        &state.s3,
                                        &state.config.s3.bucket,
                                        &s3_key,
                                        3600,
                                    )
                                    .await
                                } else {
                                    // Agent uploads → agent gets internal PUT URL
                                    crate::services::s3::presigned_upload_url(
                                        &state.s3,
                                        &state.config.s3.bucket,
                                        &s3_key,
                                        3600,
                                    )
                                    .await
                                };

                                match (console_url_result, agent_url_result) {
                                    (Ok(console_url), Ok(agent_url)) => {
                                        // Build ack for console side
                                        let console_ack = Envelope {
                                            id: Uuid::new_v4().to_string(),
                                            session_id: session_id.to_string(),
                                            timestamp: None,
                                            payload: Some(envelope::Payload::FileTransferAck(
                                                sc_protocol::FileTransferAck {
                                                    transfer_id: transfer_id.clone(),
                                                    accepted: true,
                                                    presigned_url: console_url,
                                                    message: format!(
                                                        "Transfer ready: {}",
                                                        file_name
                                                    ),
                                                },
                                            )),
                                        };
                                        let mut ack_buf = Vec::new();
                                        if console_ack.encode(&mut ack_buf).is_ok() {
                                            let _ = tx.send(Message::Binary(ack_buf.into()));
                                        }

                                        // Build enriched request for agent side (include the presigned URL)
                                        let agent_ack = Envelope {
                                            id: Uuid::new_v4().to_string(),
                                            session_id: session_id.to_string(),
                                            timestamp: None,
                                            payload: Some(envelope::Payload::FileTransferAck(
                                                sc_protocol::FileTransferAck {
                                                    transfer_id: transfer_id.clone(),
                                                    accepted: true,
                                                    presigned_url: agent_url,
                                                    message: String::new(),
                                                },
                                            )),
                                        };
                                        let mut agent_ack_buf = Vec::new();
                                        if agent_ack.encode(&mut agent_ack_buf).is_ok() {
                                            state
                                                .registry
                                                .send_to_session_agent(&session_id, agent_ack_buf);
                                        }

                                        // Also forward the original request so agent knows the file details
                                        state
                                            .registry
                                            .send_to_session_agent(&session_id, data.to_vec());

                                        tracing::info!(
                                            %session_id,
                                            %transfer_id,
                                            %file_name,
                                            upload,
                                            "File transfer URLs generated and sent"
                                        );
                                    }
                                    (Err(e), _) | (_, Err(e)) => {
                                        tracing::error!("Failed to generate presigned URL: {}", e);
                                        // Send rejection ack to console
                                        let nak = Envelope {
                                            id: Uuid::new_v4().to_string(),
                                            session_id: session_id.to_string(),
                                            timestamp: None,
                                            payload: Some(envelope::Payload::FileTransferAck(
                                                sc_protocol::FileTransferAck {
                                                    transfer_id,
                                                    accepted: false,
                                                    presigned_url: String::new(),
                                                    message: "Failed to generate transfer URL"
                                                        .to_string(),
                                                },
                                            )),
                                        };
                                        let mut nak_buf = Vec::new();
                                        if nak.encode(&mut nak_buf).is_ok() {
                                            let _ = tx.send(Message::Binary(nak_buf.into()));
                                        }
                                    }
                                }
                            }

                            // ── File list / chunks (relay to agent) ──
                            Some(envelope::Payload::FileChunk(_))
                            | Some(envelope::Payload::FileListRequest(_)) => {
                                state
                                    .registry
                                    .send_to_session_agent(&session_id, data.to_vec());
                            }

                            // ── Command (relay to agent) ──
                            Some(envelope::Payload::CommandRequest(_)) => {
                                state
                                    .registry
                                    .send_to_session_agent(&session_id, data.to_vec());
                            }

                            // ── Chat (persist + relay to agent) ──
                            Some(envelope::Payload::ChatMessage(ref msg)) => {
                                // Look up agent_id from session binding
                                let agent_id_opt =
                                    state.registry.sessions.get(&session_id).map(|b| b.agent_id);

                                if let Some(agent_id) = agent_id_opt {
                                    let _ = sqlx::query(
                                        "INSERT INTO chat_messages (session_id, agent_id, sender_type, sender_name, content)
                                         VALUES ($1, $2, 'tech', $3, $4)"
                                    )
                                    .bind(session_id)
                                    .bind(agent_id)
                                    .bind(&msg.sender_name)
                                    .bind(&msg.content)
                                    .execute(&state.db)
                                    .await;
                                }

                                state
                                    .registry
                                    .send_to_session_agent(&session_id, data.to_vec());
                            }

                            // ── Session end ──
                            Some(envelope::Payload::SessionEnd(_)) => {
                                // Relay to agent, then clean up
                                state
                                    .registry
                                    .send_to_session_agent(&session_id, data.to_vec());
                                let now = Utc::now();
                                let _ = sqlx::query(
                                    "UPDATE sessions SET status = 'ended', ended_at = $2 WHERE id = $1",
                                )
                                .bind(session_id)
                                .bind(now)
                                .execute(&state.db)
                                .await;
                                state.registry.unbind_session(&session_id);
                                break;
                            }

                            Some(_) => {
                                tracing::debug!("Unhandled console payload type");
                            }
                            None => {
                                tracing::warn!("Console sent envelope with no payload");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to decode console protobuf: {}", e);
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                let _ = tx.send(Message::Pong(data));
            }
            Ok(Message::Close(_)) => {
                tracing::info!(%session_id, "Console WebSocket closed");
                break;
            }
            Err(e) => {
                tracing::error!(%session_id, "Console WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    send_task.abort();
    let now = Utc::now();
    let _ = sqlx::query(
        "UPDATE sessions SET status = 'ended', ended_at = $2 WHERE id = $1 AND status = 'active'",
    )
    .bind(session_id)
    .bind(now)
    .execute(&state.db)
    .await;

    // Send SessionEnd to the agent BEFORE unbinding the session
    // Get agent_id from the session binding while it still exists
    let agent_id = state.registry.sessions.get(&session_id).map(|b| b.agent_id);
    if let Some(agent_id) = agent_id {
        let end_envelope = Envelope {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            timestamp: None,
            payload: Some(envelope::Payload::SessionEnd(SessionEnd {
                reason: "console_disconnected".to_string(),
            })),
        };
        let mut end_buf = Vec::new();
        if end_envelope.encode(&mut end_buf).is_ok() {
            let sent = state.registry.send_to_agent(&agent_id, end_buf);
            tracing::info!(%session_id, %agent_id, sent, "SessionEnd sent to agent on console disconnect");
        }
    } else {
        tracing::warn!(%session_id, "No session binding found, cannot send SessionEnd to agent");
    }

    state.registry.unbind_session(&session_id);
    tracing::info!(%session_id, "Console disconnected, session cleaned up");
}

// ─── Sub-handlers ────────────────────────────────────────────

/// Process agent registration, upsert in DB, register in connection registry.
async fn handle_agent_registration(
    state: &AppState,
    tx: &mpsc::UnboundedSender<Message>,
    reg: &sc_protocol::AgentRegistration,
) -> Option<Uuid> {
    // Validate tenant token
    let tenant_row: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM tenants WHERE enrollment_token = $1")
            .bind(&reg.tenant_token)
            .fetch_optional(&state.db)
            .await
            .ok()?;

    let tenant_id = match tenant_row {
        Some((tid,)) => tid,
        None => {
            tracing::warn!("Agent registration failed — invalid tenant token");
            send_registration_ack(tx, false, "Invalid tenant token", "");
            return None;
        }
    };

    // Determine agent_id: use the one provided or generate a new one
    let agent_id = if reg.agent_id.is_empty() {
        Uuid::new_v4()
    } else {
        Uuid::parse_str(&reg.agent_id).unwrap_or_else(|_| Uuid::new_v4())
    };

    let now = Utc::now();

    // Upsert agent in database
    let result = sqlx::query(
        r#"
        INSERT INTO agents (id, tenant_id, machine_name, os, os_version, arch, agent_version, status, last_seen, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'online', $8, $8)
        ON CONFLICT (tenant_id, machine_name) DO UPDATE SET
            os_version = EXCLUDED.os_version,
            arch = EXCLUDED.arch,
            agent_version = EXCLUDED.agent_version,
            status = 'online',
            last_seen = EXCLUDED.last_seen
        "#,
    )
    .bind(agent_id)
    .bind(tenant_id)
    .bind(&reg.machine_name)
    .bind(&reg.os)
    .bind(&reg.os_version)
    .bind(&reg.arch)
    .bind(&reg.agent_version)
    .bind(now)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => {
            // Register in connection registry
            state
                .registry
                .register_agent(agent_id, reg.machine_name.clone(), tx.clone());
            send_registration_ack(tx, true, "Registered successfully", &agent_id.to_string());
            tracing::info!(%agent_id, machine_name = %reg.machine_name, "Agent registered");

            // Auto-assign to group if group_name was provided
            if !reg.group_name.is_empty() {
                assign_agent_to_group(&state.db, tenant_id, agent_id, &reg.group_name).await;
            }

            Some(agent_id)
        }
        Err(e) => {
            tracing::error!("Agent registration DB error: {}", e);
            send_registration_ack(tx, false, "Internal server error", "");
            None
        }
    }
}

/// Look up a group by name (or create it) and add the agent as a member.
async fn assign_agent_to_group(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    agent_id: Uuid,
    group_name: &str,
) {
    // Find or create the group
    let group_id: Option<(Uuid,)> = sqlx::query_as(
        r#"
        INSERT INTO agent_groups (tenant_id, name)
        VALUES ($1, $2)
        ON CONFLICT (tenant_id, name) DO UPDATE SET updated_at = NOW()
        RETURNING id
        "#,
    )
    .bind(tenant_id)
    .bind(group_name)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    let Some((gid,)) = group_id else {
        tracing::warn!("Failed to find/create group '{}'", group_name);
        return;
    };

    // Add agent to group (ignore if already a member)
    let _ = sqlx::query(
        "INSERT INTO agent_group_members (group_id, agent_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
    )
    .bind(gid)
    .bind(agent_id)
    .execute(db)
    .await;

    tracing::info!(%agent_id, group = %group_name, "Agent assigned to group");
}

/// Process heartbeat — update DB, store metrics in-memory, and reply.
async fn handle_heartbeat(
    state: &AppState,
    tx: &mpsc::UnboundedSender<Message>,
    hb: &sc_protocol::Heartbeat,
) {
    if let Ok(agent_id) = Uuid::parse_str(&hb.agent_id) {
        let now = Utc::now();
        let _ = sqlx::query("UPDATE agents SET last_seen = $2, status = 'online' WHERE id = $1")
            .bind(agent_id)
            .bind(now)
            .execute(&state.db)
            .await;

        // Store real-time metrics in the connection registry
        use crate::ws::registry::AgentMetrics;
        // Preserve logged_in_user and cpu_model from AgentInfo (not part of heartbeat)
        let existing = state.registry.get_agent_metrics(&agent_id);
        let existing_user = existing
            .as_ref()
            .map(|m| m.logged_in_user.clone())
            .unwrap_or_default();
        let existing_cpu_model = existing.map(|m| m.cpu_model.clone()).unwrap_or_default();
        state.registry.update_agent_metrics(
            &agent_id,
            AgentMetrics {
                cpu_usage: hb.cpu_usage,
                memory_used: hb.memory_used,
                memory_total: hb.memory_total,
                disk_used: hb.disk_used,
                disk_total: hb.disk_total,
                uptime_secs: hb.uptime_secs,
                ip_address: hb.ip_address.clone(),
                logged_in_user: existing_user,
                cpu_model: existing_cpu_model,
            },
        );

        // Generate an HTTP upload path for the agent's desktop thumbnail,
        // but only once per hour to avoid excessive screenshot captures.
        // The agent resolves this against its known server URL.
        let thumbnail_url = if state.registry.should_capture_thumbnail(&agent_id, 3600) {
            format!("/api/agents/{}/thumbnail/upload", agent_id)
        } else {
            String::new()
        };

        // Reply with HeartbeatAck
        let ack = Envelope {
            id: Uuid::new_v4().to_string(),
            session_id: String::new(),
            timestamp: None,
            payload: Some(envelope::Payload::HeartbeatAck(HeartbeatAck {
                interval_secs: DEFAULT_HEARTBEAT_INTERVAL_SECS,
                thumbnail_upload_url: thumbnail_url,
            })),
        };
        let mut buf = Vec::new();
        if ack.encode(&mut buf).is_ok() {
            let _ = tx.send(Message::Binary(buf.into()));
        }
    }
}

/// Helper to send an AgentRegistrationAck back over the channel.
fn send_registration_ack(
    tx: &mpsc::UnboundedSender<Message>,
    success: bool,
    message: &str,
    assigned_id: &str,
) {
    let ack = Envelope {
        id: Uuid::new_v4().to_string(),
        session_id: String::new(),
        timestamp: None,
        payload: Some(envelope::Payload::AgentRegistrationAck(
            AgentRegistrationAck {
                success,
                message: message.to_string(),
                assigned_id: assigned_id.to_string(),
            },
        )),
    };
    let mut buf = Vec::new();
    if ack.encode(&mut buf).is_ok() {
        let _ = tx.send(Message::Binary(buf.into()));
    }
}
