//! WebSocket connection to the ScreenControl server.
//!
//! Connects to `/console/{session_id}`, sends/receives protobuf `Envelope` messages.
//! Includes automatic reconnection with exponential backoff.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use prost::Message;
use sc_protocol::proto::{
    envelope, ClipboardData, Envelope, InputEvent, MonitorSwitch, Ping, QualitySettings,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite};
use tracing::{error, info, warn};

/// Messages the viewer sends to the server
#[derive(Debug)]
pub enum OutgoingMessage {
    Input(InputEvent),
    Quality(QualitySettings),
    MonitorSwitch(MonitorSwitch),
    Clipboard(ClipboardData),
    Ping(u64),
}

/// Messages received from the server for the viewer
#[derive(Debug)]
#[allow(dead_code)]
pub enum IncomingMessage {
    DesktopFrame {
        width: u32,
        height: u32,
        data: Vec<u8>,
        sequence: u32,
        codec: u32, // 0=JPEG, 1=H264
        is_keyframe: bool,
    },
    ScreenInfo {
        monitors: Vec<MonitorInfo>,
        active_monitor: u32,
    },
    Clipboard {
        text: String,
    },
    Pong {
        timestamp: u64,
    },
    SessionEnd {
        reason: String,
    },
    CursorData {
        cursor_id: u64,
        width: u32,
        height: u32,
        hotspot_x: u32,
        hotspot_y: u32,
        data: Vec<u8>,
    },
    CursorPosition {
        x: f64,
        y: f64,
        cursor_id: u64,
        visible: bool,
    },
    AudioFrame {
        data: Vec<u8>,
    },
    /// Connection state changes for UI feedback
    ConnectionStateChanged(ConnectionState),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MonitorInfo {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

/// Connection lifecycle states reported to the UI
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    /// Successfully connected / reconnected
    Connected,
    /// Connection lost, attempting to reconnect
    Reconnecting {
        attempt: u32,
        /// Seconds until next retry
        next_retry_secs: u64,
    },
    /// Gave up reconnecting after max attempts
    Disconnected { reason: String },
}

/// Maximum reconnection attempts before giving up
const MAX_RECONNECT_ATTEMPTS: u32 = 20;
/// Initial backoff delay (seconds)
const INITIAL_BACKOFF_SECS: u64 = 1;
/// Maximum backoff delay (seconds)
const MAX_BACKOFF_SECS: u64 = 30;

/// Connect to the server with automatic reconnection.
/// Returns channels for sending and receiving messages.
///
/// The connection manager spawns a background task that:
/// 1. Connects the WebSocket
/// 2. Runs reader/writer loops
/// 3. On disconnect, retries with exponential backoff
/// 4. Reports `ConnectionStateChanged` events to the incoming channel
pub async fn connect(
    server_url: &str,
    session_id: &str,
    auth_token: &str,
) -> Result<(
    mpsc::Sender<OutgoingMessage>,
    mpsc::Receiver<IncomingMessage>,
)> {
    // Channels for communication with the main thread
    let (outgoing_tx, outgoing_rx) = mpsc::channel::<OutgoingMessage>(64);
    let (incoming_tx, incoming_rx) = mpsc::channel::<IncomingMessage>(64);

    // Share the outgoing receiver with the connection manager
    let outgoing_rx = Arc::new(tokio::sync::Mutex::new(outgoing_rx));

    let url = server_url.to_string();
    let sid = session_id.to_string();
    let token = auth_token.to_string();

    // Do the initial connection synchronously so we can fail fast
    let ws_url = format!("{}/ws/console/{}", url, sid);
    info!("Connecting to server: {}", ws_url);

    let request = build_ws_request(&ws_url, &token)?;

    let (ws_stream, _response) = connect_async(request)
        .await
        .context("Failed to connect to WebSocket")?;

    info!("WebSocket connected to {}", ws_url);

    // Notify connected
    let _ = incoming_tx
        .send(IncomingMessage::ConnectionStateChanged(
            ConnectionState::Connected,
        ))
        .await;

    // Spawn the connection manager that handles reconnection
    tokio::spawn(connection_manager(
        url,
        sid,
        token,
        ws_stream,
        outgoing_rx,
        incoming_tx,
    ));

    Ok((outgoing_tx, incoming_rx))
}

/// Build the WebSocket HTTP upgrade request with auth headers.
fn build_ws_request(ws_url: &str, auth_token: &str) -> Result<tungstenite::http::Request<()>> {
    let url = url::Url::parse(ws_url).context("Invalid WebSocket URL")?;

    tungstenite::http::Request::builder()
        .uri(ws_url)
        .header("Authorization", format!("Bearer {}", auth_token))
        .header("Host", url.host_str().unwrap_or("localhost"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .context("Failed to build WebSocket request")
}

/// Long-lived connection manager that handles reading, writing, and reconnection.
async fn connection_manager(
    server_url: String,
    session_id: String,
    auth_token: String,
    initial_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    outgoing_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<OutgoingMessage>>>,
    incoming_tx: mpsc::Sender<IncomingMessage>,
) {
    // Run the initial connection
    let disconnect_reason = run_connection(initial_stream, &outgoing_rx, &incoming_tx).await;

    // If the session ended cleanly, don't reconnect
    if disconnect_reason == "session_ended" {
        info!("Session ended cleanly, not reconnecting");
        return;
    }

    // Reconnection loop with exponential backoff
    let mut attempt: u32 = 0;
    let mut backoff_secs = INITIAL_BACKOFF_SECS;

    loop {
        attempt += 1;
        if attempt > MAX_RECONNECT_ATTEMPTS {
            warn!(
                "Max reconnection attempts ({}) reached, giving up",
                MAX_RECONNECT_ATTEMPTS
            );
            let _ = incoming_tx
                .send(IncomingMessage::ConnectionStateChanged(
                    ConnectionState::Disconnected {
                        reason: format!("Failed after {} attempts", MAX_RECONNECT_ATTEMPTS),
                    },
                ))
                .await;
            break;
        }

        info!(
            "Reconnecting (attempt {}/{}, retry in {}s)...",
            attempt, MAX_RECONNECT_ATTEMPTS, backoff_secs
        );

        // Notify UI about reconnection state
        let _ = incoming_tx
            .send(IncomingMessage::ConnectionStateChanged(
                ConnectionState::Reconnecting {
                    attempt,
                    next_retry_secs: backoff_secs,
                },
            ))
            .await;

        // Wait before retry
        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;

        // Attempt reconnection
        let ws_url = format!("{}/ws/console/{}", server_url, session_id);
        let request = match build_ws_request(&ws_url, &auth_token) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to build reconnect request: {}", e);
                backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
                continue;
            }
        };

        match connect_async(request).await {
            Ok((ws_stream, _)) => {
                info!("Reconnected to {} (attempt {})", ws_url, attempt);

                // Reset backoff on success
                attempt = 0;
                backoff_secs = INITIAL_BACKOFF_SECS;

                // Notify UI connected
                let _ = incoming_tx
                    .send(IncomingMessage::ConnectionStateChanged(
                        ConnectionState::Connected,
                    ))
                    .await;

                // Run connection until it drops again
                let reason = run_connection(ws_stream, &outgoing_rx, &incoming_tx).await;

                if reason == "session_ended" {
                    info!("Session ended cleanly after reconnect");
                    return;
                }

                // Connection dropped again, loop back to retry
                info!("Connection lost again: {}", reason);
            }
            Err(e) => {
                warn!("Reconnection attempt {} failed: {}", attempt, e);
                backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
            }
        }
    }
}

/// Run a single WebSocket connection, handling both reading and writing.
/// Returns a reason string when the connection ends.
async fn run_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    outgoing_rx: &Arc<tokio::sync::Mutex<mpsc::Receiver<OutgoingMessage>>>,
    incoming_tx: &mpsc::Sender<IncomingMessage>,
) -> String {
    let (mut ws_write, mut ws_read) = ws_stream.split();
    let mut outgoing_rx = outgoing_rx.lock().await;

    loop {
        tokio::select! {
            // Handle outgoing messages
            Some(msg) = outgoing_rx.recv() => {
                let envelope = match msg {
                    OutgoingMessage::Input(input) => Envelope {
                        payload: Some(envelope::Payload::InputEvent(input)),
                        ..Default::default()
                    },
                    OutgoingMessage::Quality(qs) => Envelope {
                        payload: Some(envelope::Payload::QualitySettings(qs)),
                        ..Default::default()
                    },
                    OutgoingMessage::MonitorSwitch(ms) => Envelope {
                        payload: Some(envelope::Payload::MonitorSwitch(ms)),
                        ..Default::default()
                    },
                    OutgoingMessage::Clipboard(cb) => Envelope {
                        payload: Some(envelope::Payload::ClipboardData(cb)),
                        ..Default::default()
                    },
                    OutgoingMessage::Ping(ts) => Envelope {
                        payload: Some(envelope::Payload::Ping(Ping { timestamp: ts })),
                        ..Default::default()
                    },
                };

                let mut buf = Vec::with_capacity(envelope.encoded_len());
                if let Err(e) = envelope.encode(&mut buf) {
                    error!("Failed to encode protobuf: {}", e);
                    continue;
                }

                if let Err(e) = ws_write
                    .send(tungstenite::Message::Binary(buf.into()))
                    .await
                {
                    error!("WebSocket write error: {}", e);
                    return format!("write_error: {}", e);
                }
            }

            // Handle incoming messages
            Some(msg_result) = ws_read.next() => {
                let msg = match msg_result {
                    Ok(m) => m,
                    Err(e) => {
                        error!("WebSocket read error: {}", e);
                        return format!("read_error: {}", e);
                    }
                };

                let data = match msg {
                    tungstenite::Message::Binary(data) => data,
                    tungstenite::Message::Close(_) => {
                        info!("WebSocket closed by server");
                        let _ = incoming_tx
                            .send(IncomingMessage::SessionEnd {
                                reason: "server_closed".to_string(),
                            })
                            .await;
                        return "session_ended".to_string();
                    }
                    tungstenite::Message::Ping(data) => {
                        let _ = ws_write
                            .send(tungstenite::Message::Pong(data))
                            .await;
                        continue;
                    }
                    _ => continue,
                };

                let envelope = match Envelope::decode(data.as_ref()) {
                    Ok(e) => e,
                    Err(e) => {
                        warn!("Failed to decode protobuf: {}", e);
                        continue;
                    }
                };

                let payload = match envelope.payload {
                    Some(p) => p,
                    None => continue,
                };

                let incoming = match payload {
                    envelope::Payload::DesktopFrame(frame) => IncomingMessage::DesktopFrame {
                        width: frame.width,
                        height: frame.height,
                        data: frame.data,
                        sequence: frame.sequence,
                        codec: frame.codec as u32,
                        is_keyframe: frame.is_keyframe,
                    },
                    envelope::Payload::ScreenInfo(info) => IncomingMessage::ScreenInfo {
                        monitors: info
                            .monitors
                            .into_iter()
                            .map(|m| MonitorInfo {
                                index: m.index,
                                name: m.name,
                                width: m.width,
                                height: m.height,
                                primary: m.primary,
                            })
                            .collect(),
                        active_monitor: info.active_monitor,
                    },
                    envelope::Payload::ClipboardData(cb) => {
                        IncomingMessage::Clipboard { text: cb.text }
                    }
                    envelope::Payload::Pong(pong) => IncomingMessage::Pong {
                        timestamp: pong.timestamp,
                    },
                    envelope::Payload::SessionEnd(end) => {
                        let reason = end.reason.clone();
                        let _ = incoming_tx
                            .send(IncomingMessage::SessionEnd { reason: end.reason })
                            .await;
                        return if reason.is_empty() {
                            "session_ended".to_string()
                        } else {
                            format!("session_ended: {}", reason)
                        };
                    }
                    envelope::Payload::CursorData(cd) => IncomingMessage::CursorData {
                        cursor_id: cd.cursor_id,
                        width: cd.width,
                        height: cd.height,
                        hotspot_x: cd.hotspot_x,
                        hotspot_y: cd.hotspot_y,
                        data: cd.data,
                    },
                    envelope::Payload::CursorPosition(cp) => IncomingMessage::CursorPosition {
                        x: cp.x,
                        y: cp.y,
                        cursor_id: cp.cursor_id,
                        visible: cp.visible,
                    },
                    envelope::Payload::AudioFrame(af) => {
                        IncomingMessage::AudioFrame { data: af.data }
                    }
                    _ => continue,
                };

                if incoming_tx.send(incoming).await.is_err() {
                    return "receiver_dropped".to_string();
                }
            }

            // Both channels closed
            else => {
                return "channels_closed".to_string();
            }
        }
    }
}
