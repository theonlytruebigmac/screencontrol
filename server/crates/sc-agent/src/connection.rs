//! WebSocket connection management for the agent.
//!
//! Connects to the server, sends protobuf-encoded registration and
//! heartbeats, dispatches incoming commands, and manages terminal sessions.

use futures_util::{SinkExt, StreamExt};
use prost::Message as ProstMessage;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use uuid::Uuid;

use sc_protocol::{
    envelope, AgentInfo, AgentRegistration, ChatMessage, ConsentResponse, Envelope, Heartbeat,
    SessionEnd, SessionOffer, DEFAULT_HEARTBEAT_INTERVAL_SECS,
};

use crate::chat::ChatHandle;
use crate::command;
use crate::consent;
use crate::filebrowser::FileBrowser;
use crate::input::InputInjector;
use crate::screen::DesktopCapturer;
use crate::sysinfo_collector::{self, SystemInfo};
use crate::terminal::TerminalManager;

/// Connect to the server and run the agent message loop.
pub async fn connect_and_run(
    server_url: &str,
    tenant_token: &str,
    group_name: &str,
    sys_info: &SystemInfo,
    chat_handle: ChatHandle,
    reply_rx: std::sync::Arc<
        std::sync::Mutex<std::sync::mpsc::Receiver<crate::chat::OutgoingReply>>,
    >,
) -> anyhow::Result<()> {
    let (ws_stream, _) = connect_async(server_url).await?;
    tracing::info!("WebSocket connected to server");

    let (mut ws_write, mut ws_read) = ws_stream.split();

    // Internal channel for writing to the WS from multiple tasks
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // Session managers
    let mut terminal_mgr = TerminalManager::new();
    let mut desktop_capturer = DesktopCapturer::new();
    let mut input_injector: Option<InputInjector> = None;

    // Derive HTTP base URL from WS URL (e.g. ws://host:8080/ws/agent → http://host:8080)
    let http_base = server_url
        .replace("wss://", "https://")
        .replace("ws://", "http://")
        .split("/ws/")
        .next()
        .unwrap_or("http://localhost:8080")
        .to_string();
    let mut file_browser = FileBrowser::new(&http_base, tenant_token);

    // Shared System handle for real-time metrics in heartbeat
    let sys = Arc::new(Mutex::new(sysinfo::System::new_all()));

    // ── Send registration ────────────────────────────────────

    let agent_id = load_or_create_agent_id();

    let reg_envelope = Envelope {
        id: Uuid::new_v4().to_string(),
        session_id: String::new(),
        timestamp: None,
        payload: Some(envelope::Payload::AgentRegistration(AgentRegistration {
            agent_id: agent_id.to_string(),
            machine_name: sys_info.machine_name.clone(),
            os: sys_info.os.clone(),
            os_version: sys_info.os_version.clone(),
            arch: sys_info.arch.clone(),
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            tenant_token: tenant_token.to_string(),
            group_name: group_name.to_string(),
            instance_id: String::new(), // populated in Phase 3
        })),
    };

    let mut buf = Vec::new();
    reg_envelope.encode(&mut buf)?;
    ws_write.send(Message::Binary(buf)).await?;
    tracing::info!("Sent protobuf registration for agent {}", agent_id);

    // ── Heartbeat task ───────────────────────────────────────

    let heartbeat_interval = Arc::new(AtomicU32::new(DEFAULT_HEARTBEAT_INTERVAL_SECS));
    let hb_interval_clone = Arc::clone(&heartbeat_interval);

    let heartbeat_tx = tx.clone();
    let hb_agent_id = agent_id.to_string();
    let hb_sys = Arc::clone(&sys);
    let heartbeat_handle = tokio::spawn(async move {
        loop {
            let secs = hb_interval_clone.load(Ordering::Relaxed) as u64;
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;

            // Sample real-time metrics
            let metrics = {
                let mut sys_lock = hb_sys.lock().await;
                sysinfo_collector::collect_realtime_metrics(&mut sys_lock)
            };

            let hb_envelope = Envelope {
                id: Uuid::new_v4().to_string(),
                session_id: String::new(),
                timestamp: None,
                payload: Some(envelope::Payload::Heartbeat(Heartbeat {
                    agent_id: hb_agent_id.clone(),
                    cpu_usage: metrics.cpu_usage,
                    memory_used: metrics.memory_used,
                    memory_total: metrics.memory_total,
                    disk_used: metrics.disk_used,
                    disk_total: metrics.disk_total,
                    uptime_secs: metrics.uptime_secs,
                    ip_address: metrics.ip_address,
                })),
            };

            let mut buf = Vec::new();
            if hb_envelope.encode(&mut buf).is_ok() {
                if heartbeat_tx.send(buf).is_err() {
                    break;
                }
                tracing::debug!(
                    "Heartbeat sent (cpu={:.1}%, mem={}/{})",
                    metrics.cpu_usage,
                    metrics.memory_used,
                    metrics.memory_total
                );
            }
        }
    });

    // ── Writer task (drains tx channel → WS) ─────────────────

    let writer_handle = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            if ws_write.send(Message::Binary(data)).await.is_err() {
                break;
            }
        }
    });

    // Shared session ID for chat replies — updated when we receive a ChatMessage
    let active_chat_session: Arc<std::sync::Mutex<Option<String>>> =
        Arc::new(std::sync::Mutex::new(None));

    // ── Chat window (embedded WebView) ───────────────────────
    // Messages are sent to the WebView via ChatHandle.show_message().
    // Replies from the WebView are forwarded back as protobuf ChatMessages.
    {
        let tx_reply = tx.clone();
        let machine_name = sys_info.machine_name.clone();
        let reply_rx = reply_rx.clone();
        let chat_session = active_chat_session.clone();
        tokio::spawn(async move {
            loop {
                // Non-blocking poll with a small sleep to avoid busy-waiting
                let maybe_reply = {
                    let rx = reply_rx.lock().unwrap();
                    rx.try_recv().ok()
                };
                if let Some(reply) = maybe_reply {
                    let sid = chat_session.lock().unwrap().clone().unwrap_or_default();
                    tracing::info!("Sending reply to session {}: {}", sid, reply.content);
                    let reply_env = Envelope {
                        id: Uuid::new_v4().to_string(),
                        session_id: sid,
                        timestamp: None,
                        payload: Some(envelope::Payload::ChatMessage(ChatMessage {
                            sender_id: "agent-user".to_string(),
                            sender_name: machine_name.clone(),
                            content: reply.content,
                            timestamp: String::new(),
                        })),
                    };
                    let mut buf = Vec::new();
                    if reply_env.encode(&mut buf).is_ok() {
                        if tx_reply.send(buf).is_err() {
                            break; // Channel closed, connection dropped
                        }
                    }
                } else {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        });
    }
    // ── Reader loop ──────────────────────────────────────────

    while let Some(msg) = ws_read.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                match Envelope::decode(data.as_slice()) {
                    Ok(envelope) => {
                        let session_id = envelope.session_id.clone();
                        match envelope.payload {
                            Some(envelope::Payload::AgentRegistrationAck(ack)) => {
                                if ack.success {
                                    tracing::info!(
                                        "Registration confirmed. Assigned ID: {}",
                                        ack.assigned_id
                                    );

                                    // Send AgentInfo with logged-in user and system details
                                    let logged_in_user = std::env::var("USER")
                                        .or_else(|_| std::env::var("LOGNAME"))
                                        .unwrap_or_else(|_| "unknown".to_string());

                                    // Read CPU model name
                                    let cpu_model = read_cpu_model();

                                    let info_envelope = Envelope {
                                        id: Uuid::new_v4().to_string(),
                                        session_id: String::new(),
                                        timestamp: None,
                                        payload: Some(envelope::Payload::AgentInfo(AgentInfo {
                                            agent_id: agent_id.to_string(),
                                            machine_name: sys_info.machine_name.clone(),
                                            os: sys_info.os.clone(),
                                            os_version: sys_info.os_version.clone(),
                                            arch: sys_info.arch.clone(),
                                            agent_version: env!("CARGO_PKG_VERSION").to_string(),
                                            monitors: Vec::new(),
                                            network_interfaces: Vec::new(),
                                            logged_in_user,
                                            domain: String::new(),
                                            cpu_model,
                                        })),
                                    };
                                    let mut info_buf = Vec::new();
                                    if info_envelope.encode(&mut info_buf).is_ok() {
                                        let _ = tx.send(info_buf);
                                        tracing::info!("Sent AgentInfo with system details");
                                    }
                                } else {
                                    tracing::error!("Registration rejected: {}", ack.message);
                                    break;
                                }
                            }
                            Some(envelope::Payload::HeartbeatAck(ack)) => {
                                let prev =
                                    heartbeat_interval.swap(ack.interval_secs, Ordering::Relaxed);
                                if prev != ack.interval_secs {
                                    tracing::info!(
                                        "Heartbeat interval changed: {}s → {}s",
                                        prev,
                                        ack.interval_secs
                                    );
                                } else {
                                    tracing::debug!(
                                        "HeartbeatAck — interval: {}s",
                                        ack.interval_secs
                                    );
                                }

                                // Upload desktop thumbnail if server provided an upload URL/path
                                if !ack.thumbnail_upload_url.is_empty() {
                                    // Server may send a relative path like /api/agents/{id}/thumbnail/upload
                                    let url = if ack.thumbnail_upload_url.starts_with('/') {
                                        format!("{}{}", http_base, ack.thumbnail_upload_url)
                                    } else {
                                        ack.thumbnail_upload_url.clone()
                                    };
                                    tokio::spawn(async move {
                                        if let Err(e) = capture_and_upload_thumbnail(&url).await {
                                            tracing::warn!("Thumbnail upload skipped: {}", e);
                                        }
                                    });
                                }

                                // If the server signals an update is available, trigger immediate update
                                if ack.update_available && !ack.update_download_url.is_empty() {
                                    let base = http_base.clone();
                                    let download_url = if ack.update_download_url.starts_with('/') {
                                        format!("{}{}", base, ack.update_download_url)
                                    } else {
                                        ack.update_download_url.clone()
                                    };
                                    let sha256 = ack.update_sha256.clone();
                                    let version = ack.update_version.clone();
                                    tracing::info!(
                                        "Server signaled update available: v{} — triggering immediate update",
                                        version
                                    );
                                    tokio::spawn(async move {
                                        if let Err(e) = crate::updater::apply_update_from_hint(
                                            &download_url,
                                            &sha256,
                                            &version,
                                        )
                                        .await
                                        {
                                            tracing::error!("Immediate update failed: {}", e);
                                        }
                                    });
                                }
                            }
                            Some(envelope::Payload::SessionRequest(req)) => {
                                tracing::info!(
                                    "Session requested by user {} (type: {})",
                                    req.user_id,
                                    req.session_type
                                );

                                // Spawn a terminal session
                                if req.session_type == 2
                                /* SESSION_TYPE_TERMINAL */
                                {
                                    match terminal_mgr.spawn_session(
                                        &session_id,
                                        80, // default cols
                                        24, // default rows
                                        tx.clone(),
                                    ) {
                                        Ok(()) => {
                                            let offer = Envelope {
                                                id: Uuid::new_v4().to_string(),
                                                session_id: session_id.clone(),
                                                timestamp: None,
                                                payload: Some(envelope::Payload::SessionOffer(
                                                    SessionOffer {
                                                        sdp: "terminal-ready".to_string(),
                                                        session_type: 2,
                                                    },
                                                )),
                                            };
                                            let mut buf = Vec::new();
                                            if offer.encode(&mut buf).is_ok() {
                                                let _ = tx.send(buf);
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to spawn terminal: {}", e);
                                        }
                                    }
                                } else if req.session_type == 1
                                /* SESSION_TYPE_DESKTOP */
                                {
                                    // ── Consent check ──────────────────────────
                                    let requester = if req.user_id.is_empty() {
                                        "Unknown user".to_string()
                                    } else {
                                        req.user_id.clone()
                                    };

                                    let consent_result = consent::prompt_user(
                                        &requester,
                                        "desktop",
                                        std::time::Duration::from_secs(30),
                                    )
                                    .await;

                                    match &consent_result {
                                        consent::ConsentResult::Granted => {
                                            tracing::info!("Consent granted by local user");
                                        }
                                        consent::ConsentResult::NoDisplay => {
                                            tracing::info!(
                                                "No display — auto-granting (headless mode)"
                                            );
                                        }
                                        consent::ConsentResult::Denied => {
                                            tracing::info!("Consent denied by local user");
                                            // Send ConsentResponse + SessionEnd
                                            let deny_env = Envelope {
                                                id: Uuid::new_v4().to_string(),
                                                session_id: session_id.clone(),
                                                timestamp: None,
                                                payload: Some(envelope::Payload::ConsentResponse(
                                                    ConsentResponse {
                                                        granted: false,
                                                        reason: "User denied access".to_string(),
                                                    },
                                                )),
                                            };
                                            let mut buf = Vec::new();
                                            if deny_env.encode(&mut buf).is_ok() {
                                                let _ = tx.send(buf);
                                            }
                                            let end_env = Envelope {
                                                id: Uuid::new_v4().to_string(),
                                                session_id: session_id.clone(),
                                                timestamp: None,
                                                payload: Some(envelope::Payload::SessionEnd(
                                                    SessionEnd {
                                                        reason: "Access denied by user".to_string(),
                                                    },
                                                )),
                                            };
                                            let mut buf = Vec::new();
                                            if end_env.encode(&mut buf).is_ok() {
                                                let _ = tx.send(buf);
                                            }
                                            continue;
                                        }
                                        consent::ConsentResult::TimedOut => {
                                            tracing::info!("Consent timed out — denying");
                                            let deny_env = Envelope {
                                                id: Uuid::new_v4().to_string(),
                                                session_id: session_id.clone(),
                                                timestamp: None,
                                                payload: Some(envelope::Payload::ConsentResponse(
                                                    ConsentResponse {
                                                        granted: false,
                                                        reason: "Request timed out".to_string(),
                                                    },
                                                )),
                                            };
                                            let mut buf = Vec::new();
                                            if deny_env.encode(&mut buf).is_ok() {
                                                let _ = tx.send(buf);
                                            }
                                            let end_env = Envelope {
                                                id: Uuid::new_v4().to_string(),
                                                session_id: session_id.clone(),
                                                timestamp: None,
                                                payload: Some(envelope::Payload::SessionEnd(
                                                    SessionEnd {
                                                        reason: "Consent timed out".to_string(),
                                                    },
                                                )),
                                            };
                                            let mut buf = Vec::new();
                                            if end_env.encode(&mut buf).is_ok() {
                                                let _ = tx.send(buf);
                                            }
                                            continue;
                                        }
                                    }

                                    // Send screen info first
                                    if let Some(info_buf) =
                                        DesktopCapturer::get_screen_info(&session_id)
                                    {
                                        let _ = tx.send(info_buf);
                                    }

                                    // Start screen capture (returns D-Bus input handle on Linux)
                                    let input_handle_rx = desktop_capturer.start_capture(
                                        &session_id,
                                        req.monitor_index,
                                        tx.clone(),
                                    );

                                    // Create input injector — prefer Mutter D-Bus on Linux/Wayland
                                    #[cfg(target_os = "linux")]
                                    {
                                        if let Some(rx) = input_handle_rx {
                                            // Wait for the Mutter RemoteDesktop session to be ready
                                            match tokio::time::timeout(
                                                std::time::Duration::from_secs(5),
                                                rx,
                                            )
                                            .await
                                            {
                                                Ok(Ok(handle)) => {
                                                    let inj = crate::input::MutterInputInjector::from_handle(handle);
                                                    input_injector =
                                                        Some(InputInjector::Mutter(inj));
                                                    tracing::info!("MutterInputInjector (D-Bus) created — no consent dialog needed");
                                                }
                                                Ok(Err(_)) => {
                                                    tracing::warn!("Mutter input handle channel dropped, falling back to enigo");
                                                    let (sw, sh) =
                                                        crate::screen::get_total_screen_size();
                                                    if let Ok(inj) =
                                                        crate::input::EnigoInputInjector::new(
                                                            sw, sh,
                                                        )
                                                    {
                                                        input_injector =
                                                            Some(InputInjector::Enigo(inj));
                                                    }
                                                }
                                                Err(_) => {
                                                    tracing::warn!("Timed out waiting for Mutter input handle, falling back to enigo");
                                                    let (sw, sh) =
                                                        crate::screen::get_total_screen_size();
                                                    if let Ok(inj) =
                                                        crate::input::EnigoInputInjector::new(
                                                            sw, sh,
                                                        )
                                                    {
                                                        input_injector =
                                                            Some(InputInjector::Enigo(inj));
                                                    }
                                                }
                                            }
                                        } else {
                                            let (sw, sh) = crate::screen::get_total_screen_size();
                                            if let Ok(inj) =
                                                crate::input::EnigoInputInjector::new(sw, sh)
                                            {
                                                input_injector = Some(InputInjector::Enigo(inj));
                                            }
                                        }
                                    }

                                    #[cfg(not(target_os = "linux"))]
                                    {
                                        let _ = input_handle_rx; // suppress unused warning
                                        let (sw, sh) = crate::screen::get_total_screen_size();
                                        if let Ok(inj) =
                                            crate::input::EnigoInputInjector::new(sw, sh)
                                        {
                                            input_injector = Some(InputInjector::Enigo(inj));
                                        }
                                    }

                                    let offer = Envelope {
                                        id: Uuid::new_v4().to_string(),
                                        session_id: session_id.clone(),
                                        timestamp: None,
                                        payload: Some(envelope::Payload::SessionOffer(
                                            SessionOffer {
                                                sdp: "desktop-ready".to_string(),
                                                session_type: 1,
                                            },
                                        )),
                                    };
                                    let mut buf = Vec::new();
                                    if offer.encode(&mut buf).is_ok() {
                                        let _ = tx.send(buf);
                                    }
                                } else if req.session_type == 3
                                /* SESSION_TYPE_FILE_TRANSFER */
                                {
                                    tracing::info!("File transfer session ready");
                                    let offer = Envelope {
                                        id: Uuid::new_v4().to_string(),
                                        session_id: session_id.clone(),
                                        timestamp: None,
                                        payload: Some(envelope::Payload::SessionOffer(
                                            SessionOffer {
                                                sdp: "file-transfer-ready".to_string(),
                                                session_type: 3,
                                            },
                                        )),
                                    };
                                    let mut buf = Vec::new();
                                    if offer.encode(&mut buf).is_ok() {
                                        let _ = tx.send(buf);
                                    }
                                }
                            }
                            Some(envelope::Payload::TerminalData(td)) => {
                                terminal_mgr.write_to_session(&session_id, td.data);
                            }
                            Some(envelope::Payload::TerminalResize(resize)) => {
                                tracing::debug!("Terminal resize: {}x{}", resize.cols, resize.rows);
                                terminal_mgr.resize_session(
                                    &session_id,
                                    resize.cols as u16,
                                    resize.rows as u16,
                                );
                            }
                            Some(envelope::Payload::SessionEnd(end)) => {
                                tracing::info!("Session ended: {}", end.reason);
                                terminal_mgr.close_session(&session_id);
                                desktop_capturer.stop_capture(&session_id);
                                input_injector = None;
                                tracing::info!(
                                    "Input injector cleared for session: {}",
                                    session_id
                                );
                            }
                            Some(envelope::Payload::InputEvent(evt)) => {
                                if let Some(ref mut injector) = input_injector {
                                    tracing::debug!("InputEvent received, injecting");
                                    injector.handle_event(&evt);
                                } else {
                                    tracing::warn!(
                                        "InputEvent received but input_injector is None!"
                                    );
                                }
                            }
                            Some(envelope::Payload::CommandRequest(cmd)) => {
                                tracing::info!("Command requested: {} {:?}", cmd.command, cmd.args);
                                let cmd_tx = tx.clone();
                                let cmd_session = session_id.clone();
                                tokio::spawn(async move {
                                    command::execute_command(
                                        &cmd_session,
                                        &cmd.command,
                                        &cmd.args,
                                        &cmd.working_dir,
                                        cmd.timeout_secs,
                                        &cmd_tx,
                                    )
                                    .await;
                                });
                            }
                            Some(envelope::Payload::FileTransferRequest(req)) => {
                                tracing::info!("File transfer requested: {}", req.file_name);
                                file_browser.handle_transfer_request(&session_id, &req, &tx);
                            }
                            Some(envelope::Payload::FileTransferAck(ack)) => {
                                tracing::info!(
                                    "File transfer ack received: {} (accepted: {})",
                                    ack.transfer_id,
                                    ack.accepted
                                );
                                file_browser.handle_transfer_ack(&session_id, &ack, &tx);
                            }
                            Some(envelope::Payload::FileListRequest(req)) => {
                                tracing::info!("File list requested for: {}", req.path);
                                file_browser.handle_file_list(&session_id, &req, &tx);
                            }
                            Some(envelope::Payload::ChatMessage(msg)) => {
                                tracing::info!("Chat from {}: {}", msg.sender_name, msg.content);

                                // Track the session so replies go to the right place
                                *active_chat_session.lock().unwrap() = Some(session_id.clone());

                                // Send to the embedded WebView chat window
                                chat_handle
                                    .show_message(msg.sender_name.clone(), msg.content.clone());
                            }
                            Some(_) => {
                                tracing::debug!("Unhandled payload type");
                            }
                            None => {
                                tracing::warn!("Received envelope with no payload");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to decode protobuf: {}", e);
                    }
                }
            }
            Ok(Message::Ping(data)) => {
                let _ = tx.send(data);
            }
            Ok(Message::Close(_)) => {
                tracing::info!("Server closed connection");
                break;
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    terminal_mgr.close_all();
    desktop_capturer.stop_all();
    heartbeat_handle.abort();
    writer_handle.abort();
    Ok(())
}

/// Load agent ID from a local file, or create one on first run.
fn load_or_create_agent_id() -> Uuid {
    let id_path = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("screencontrol")
        .join("agent_id");

    if let Ok(contents) = std::fs::read_to_string(&id_path) {
        if let Ok(id) = Uuid::parse_str(contents.trim()) {
            return id;
        }
    }

    let id = Uuid::new_v4();
    if let Some(parent) = id_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&id_path, id.to_string());
    tracing::info!("Generated new agent ID: {}", id);
    id
}

/// Read the CPU model name from the system.
fn read_cpu_model() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in contents.lines() {
                if line.starts_with("model name") {
                    if let Some(value) = line.split(':').nth(1) {
                        return value.trim().to_string();
                    }
                }
            }
        }
        "Unknown".to_string()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Unknown".to_string())
    }
    #[cfg(target_os = "windows")]
    {
        // Read from registry or WMI; fall back to arch
        std::process::Command::new("wmic")
            .args(["cpu", "get", "name", "/value"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("Name="))
                    .map(|l| l.trim_start_matches("Name=").trim().to_string())
            })
            .unwrap_or_else(|| "Unknown".to_string())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "Unknown".to_string()
    }
}

/// Capture a desktop screenshot and upload it to the pre-signed S3 URL.
///
/// Uses the Mutter ScreenCast D-Bus API (same as remote desktop streaming)
/// to get a PipeWire node, then captures a single JPEG frame via GStreamer.
/// This is the only reliable method on GNOME 46+ Wayland where external
/// screenshot CLI tools are blocked or deprecated.
#[cfg(target_os = "linux")]
async fn capture_and_upload_thumbnail(upload_url: &str) -> anyhow::Result<()> {
    use zbus::Connection;

    let tmp_path = "/tmp/sc_thumbnail.jpg";

    // Clean up any previous screenshot
    let _ = tokio::fs::remove_file(tmp_path).await;

    // Connect to the session D-Bus
    let connection = Connection::session().await?;

    // Create a Mutter ScreenCast session
    // is-recording=false tells GNOME this is a one-shot capture, suppressing
    // the "screen recording" indicator in the system tray.
    let mut session_props = std::collections::HashMap::<String, zbus::zvariant::Value>::new();
    session_props.insert(
        "disable-animations".to_string(),
        zbus::zvariant::Value::Bool(true),
    );
    session_props.insert(
        "is-recording".to_string(),
        zbus::zvariant::Value::Bool(false),
    );

    let session_path: zbus::zvariant::OwnedObjectPath = connection
        .call_method(
            Some("org.gnome.Mutter.ScreenCast"),
            "/org/gnome/Mutter/ScreenCast",
            Some("org.gnome.Mutter.ScreenCast"),
            "CreateSession",
            &(session_props,),
        )
        .await?
        .body()
        .deserialize()?;

    // Create a stream for the primary monitor (no cursor)
    let mut stream_props = std::collections::HashMap::<String, zbus::zvariant::Value>::new();
    stream_props.insert(
        "cursor-mode".to_string(),
        zbus::zvariant::Value::U32(0), // 0 = hidden for thumbnail
    );

    let _stream_path: zbus::zvariant::OwnedObjectPath = connection
        .call_method(
            Some("org.gnome.Mutter.ScreenCast"),
            session_path.as_ref(),
            Some("org.gnome.Mutter.ScreenCast.Session"),
            "RecordMonitor",
            &("", stream_props),
        )
        .await?
        .body()
        .deserialize()?;

    // Listen for the PipeWireStreamAdded signal to get node_id
    let (node_tx, node_rx) = tokio::sync::oneshot::channel::<u32>();

    let signal_conn = connection.clone();
    let signal_task = tokio::spawn(async move {
        use futures_util::StreamExt;

        let mut stream = zbus::MessageStream::from(&signal_conn);
        while let Some(msg) = stream.next().await {
            if let Ok(msg) = msg {
                let header = msg.header();
                if header.member().map(|m| m.as_str()) == Some("PipeWireStreamAdded") {
                    if let Ok(node_id) = msg.body().deserialize::<u32>() {
                        let _ = node_tx.send(node_id);
                        return;
                    }
                }
            }
        }
    });

    // Start the session
    connection
        .call_method(
            Some("org.gnome.Mutter.ScreenCast"),
            session_path.as_ref(),
            Some("org.gnome.Mutter.ScreenCast.Session"),
            "Start",
            &(),
        )
        .await?;

    // Wait for the PipeWire node_id
    let node_id = tokio::time::timeout(std::time::Duration::from_secs(5), node_rx)
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for PipeWire node"))?
        .map_err(|_| anyhow::anyhow!("Signal task dropped"))?;

    signal_task.abort();

    tracing::debug!(node_id, "Thumbnail capture: PipeWire node ready");

    // Use GStreamer to capture a single JPEG frame from the PipeWire node
    let pw_src =
        format!("pipewiresrc path={node_id} do-timestamp=true keepalive-time=1000 num-buffers=1");
    let gst_args =
        format!("{pw_src} ! videoconvert ! jpegenc quality=75 ! filesink location={tmp_path}");

    let gst_result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new("gst-launch-1.0")
            .arg("-e")
            .args(gst_args.split_whitespace())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output(),
    )
    .await;

    // Stop the Mutter ScreenCast session
    let _ = connection
        .call_method(
            Some("org.gnome.Mutter.ScreenCast"),
            session_path.as_ref(),
            Some("org.gnome.Mutter.ScreenCast.Session"),
            "Stop",
            &(),
        )
        .await;

    // Check if GStreamer succeeded
    match gst_result {
        Ok(Ok(output)) if output.status.success() => {}
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "gst-launch failed: exit={}, stderr={}",
                output.status,
                stderr
            );
        }
        Ok(Err(e)) => anyhow::bail!("gst-launch spawn error: {}", e),
        Err(_) => anyhow::bail!("gst-launch timed out"),
    }

    // Read the captured frame
    let data = tokio::fs::read(tmp_path).await?;
    let _ = tokio::fs::remove_file(tmp_path).await;

    if data.is_empty() {
        anyhow::bail!("Screenshot file was empty");
    }

    tracing::debug!("Captured thumbnail: {} bytes", data.len());

    // Upload via pre-signed PUT URL
    let client = reqwest::Client::new();
    let resp = client
        .put(upload_url)
        .header("Content-Type", "image/jpeg")
        .body(data)
        .send()
        .await?;

    if resp.status().is_success() {
        tracing::debug!("Thumbnail uploaded successfully");
    } else {
        tracing::debug!("Thumbnail upload failed: HTTP {}", resp.status());
    }

    Ok(())
}

/// Capture a desktop screenshot on macOS and upload it to the pre-signed S3 URL.
///
/// Uses CoreGraphics `CGDisplayCreateImage` directly via FFI, which works
/// from a root LaunchDaemon when Screen Recording TCC permission is granted.
/// Falls back to `screencapture` in interactive (non-root) mode.
#[cfg(target_os = "macos")]
async fn capture_and_upload_thumbnail(upload_url: &str) -> anyhow::Result<()> {
    let tmp_path = "/tmp/sc_thumbnail.jpg";

    // Clean up any previous thumbnail
    let _ = tokio::fs::remove_file(tmp_path).await;

    // Use CoreGraphics to capture the main display directly.
    // This works as root when Screen Recording TCC is granted.
    let _captured = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        capture_display_to_jpeg(tmp_path)
    })
    .await??;

    // Verify the file exists and is non-empty
    match tokio::fs::metadata(tmp_path).await {
        Ok(meta) if meta.len() > 0 => {}
        _ => anyhow::bail!("Screenshot capture produced no output file"),
    }

    upload_thumbnail_file(tmp_path, upload_url).await
}

/// Use CoreGraphics to capture the main display and save as JPEG.
#[cfg(target_os = "macos")]
fn capture_display_to_jpeg(output_path: &str) -> anyhow::Result<()> {
    use std::ffi::c_void;
    use std::ptr;

    // CoreGraphics / CoreFoundation / ImageIO type aliases
    type CGDirectDisplayID = u32;
    type CGImageRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFURLRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CGImageDestinationRef = *const c_void;
    type CFIndex = isize;
    type CFAllocatorRef = *const c_void;
    type CFNumberRef = *const c_void;
    type Boolean = u8;

    extern "C" {
        fn CGMainDisplayID() -> CGDirectDisplayID;
        fn CGDisplayCreateImage(display: CGDirectDisplayID) -> CGImageRef;
        fn CGImageRelease(image: CGImageRef);

        fn CFRelease(cf: CFTypeRef);

        // CFString
        static kCFAllocatorDefault: CFAllocatorRef;
        fn CFStringCreateWithBytes(
            alloc: CFAllocatorRef,
            bytes: *const u8,
            num_bytes: CFIndex,
            encoding: u32,
            is_external: Boolean,
        ) -> CFStringRef;

        // CFURL
        fn CFURLCreateWithFileSystemPath(
            alloc: CFAllocatorRef,
            file_path: CFStringRef,
            path_style: isize,
            is_directory: Boolean,
        ) -> CFURLRef;

        // CFNumber
        fn CFNumberCreate(
            alloc: CFAllocatorRef,
            the_type: CFIndex,
            value_ptr: *const c_void,
        ) -> CFNumberRef;

        // CFDictionary
        fn CFDictionaryCreate(
            alloc: CFAllocatorRef,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: CFIndex,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFDictionaryRef;

        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;

        // ImageIO
        fn CGImageDestinationCreateWithURL(
            url: CFURLRef,
            ty: CFStringRef,
            count: usize,
            options: CFDictionaryRef,
        ) -> CGImageDestinationRef;
        fn CGImageDestinationAddImage(
            dest: CGImageDestinationRef,
            image: CGImageRef,
            properties: CFDictionaryRef,
        );
        fn CGImageDestinationFinalize(dest: CGImageDestinationRef) -> Boolean;
    }

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;
    const K_CF_URL_POSIX_PATH_STYLE: isize = 0;
    const K_CF_NUMBER_FLOAT64_TYPE: CFIndex = 13; // kCFNumberFloat64Type

    unsafe {
        // Capture the main display
        let display_id = CGMainDisplayID();
        let image = CGDisplayCreateImage(display_id);
        if image.is_null() {
            anyhow::bail!("CGDisplayCreateImage returned null — Screen Recording permission may not be granted");
        }

        // Create CFURL for output path
        let path_cf = CFStringCreateWithBytes(
            kCFAllocatorDefault,
            output_path.as_ptr(),
            output_path.len() as CFIndex,
            K_CF_STRING_ENCODING_UTF8,
            0,
        );
        let url = CFURLCreateWithFileSystemPath(
            kCFAllocatorDefault,
            path_cf,
            K_CF_URL_POSIX_PATH_STYLE,
            0,
        );

        // Create "public.jpeg" UTI string
        let jpeg_uti_str = "public.jpeg";
        let jpeg_uti = CFStringCreateWithBytes(
            kCFAllocatorDefault,
            jpeg_uti_str.as_ptr(),
            jpeg_uti_str.len() as CFIndex,
            K_CF_STRING_ENCODING_UTF8,
            0,
        );

        // Create image destination and write
        let dest = CGImageDestinationCreateWithURL(url, jpeg_uti, 1, ptr::null());
        if dest.is_null() {
            CGImageRelease(image);
            CFRelease(path_cf);
            CFRelease(url);
            CFRelease(jpeg_uti);
            anyhow::bail!("Failed to create image destination");
        }

        // Build properties dictionary with JPEG compression quality = 0.4
        // kCGImageDestinationLossyCompressionQuality CFString key
        let quality_key_str = "kCGImageDestinationLossyCompressionQuality";
        let quality_key = CFStringCreateWithBytes(
            kCFAllocatorDefault,
            quality_key_str.as_ptr(),
            quality_key_str.len() as CFIndex,
            K_CF_STRING_ENCODING_UTF8,
            0,
        );
        let quality_value: f64 = 0.4;
        let quality_num = CFNumberCreate(
            kCFAllocatorDefault,
            K_CF_NUMBER_FLOAT64_TYPE,
            &quality_value as *const f64 as *const c_void,
        );
        let keys = [quality_key as *const c_void];
        let values = [quality_num as *const c_void];
        let props = CFDictionaryCreate(
            kCFAllocatorDefault,
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const c_void,
            &kCFTypeDictionaryValueCallBacks as *const c_void,
        );

        CGImageDestinationAddImage(dest, image, props);
        let ok = CGImageDestinationFinalize(dest);

        // Clean up
        CFRelease(props);
        CFRelease(quality_num);
        CFRelease(quality_key);
        CFRelease(dest);
        CGImageRelease(image);
        CFRelease(path_cf);
        CFRelease(url);
        CFRelease(jpeg_uti);

        if ok == 0 {
            anyhow::bail!("CGImageDestinationFinalize failed");
        }

        tracing::debug!("Captured display {} via CoreGraphics", display_id);
    }

    Ok(())
}

/// Capture a desktop screenshot on Windows and upload it to the pre-signed S3 URL.
///
/// Uses PowerShell with .NET System.Drawing to capture the screen.
#[cfg(target_os = "windows")]
async fn capture_and_upload_thumbnail(upload_url: &str) -> anyhow::Result<()> {
    let tmp_path = std::env::temp_dir().join("sc_thumbnail.jpg");
    let tmp_str = tmp_path.to_string_lossy().to_string();

    let ps_script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bitmap = New-Object System.Drawing.Bitmap($screen.Width, $screen.Height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
$bitmap.Save('{}', [System.Drawing.Imaging.ImageFormat]::Jpeg)
$graphics.Dispose()
$bitmap.Dispose()
"#,
        tmp_str.replace('\\', "\\\\")
    );

    let output = tokio::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "PowerShell screenshot failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    upload_thumbnail_file(&tmp_str, upload_url).await
}

/// Stub for platforms without thumbnail support.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
async fn capture_and_upload_thumbnail(_upload_url: &str) -> anyhow::Result<()> {
    tracing::debug!("Thumbnail capture not supported on this platform");
    Ok(())
}

/// Common helper: read a screenshot file, upload it, and clean up.
#[allow(dead_code)]
async fn upload_thumbnail_file(path: &str, upload_url: &str) -> anyhow::Result<()> {
    let data = tokio::fs::read(path).await?;
    let _ = tokio::fs::remove_file(path).await;

    if data.is_empty() {
        anyhow::bail!("Screenshot file was empty");
    }

    let client = reqwest::Client::new();
    let resp = client
        .put(upload_url)
        .header("Content-Type", "image/jpeg")
        .body(data)
        .send()
        .await?;

    if resp.status().is_success() {
        tracing::info!("Thumbnail uploaded successfully");
    } else {
        tracing::warn!("Thumbnail upload failed: HTTP {}", resp.status());
    }

    Ok(())
}
