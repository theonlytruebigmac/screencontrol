//! WebSocket handler for UI event subscriptions.
//!
//! UI clients connect to `/ws/events` to receive real-time JSON events
//! such as agent status changes, session updates, etc.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::AppState;

/// Events WebSocket upgrade.
pub async fn events_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_events_socket(socket, state))
}

async fn handle_events_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let sub_id = Uuid::new_v4();

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Register as event subscriber
    state.registry.add_event_sub(sub_id, tx);

    tracing::info!(%sub_id, "Events WebSocket subscriber connected");

    // Forward channel → WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Keep alive — just listen for pings or close
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Ping(data)) => {
                // Can't send pong directly since sender is moved; subscriber pattern
                // handles this via the send task. We'll just continue.
                let _ = data;
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    // Cleanup
    send_task.abort();
    state.registry.remove_event_sub(&sub_id);
    tracing::info!(%sub_id, "Events WebSocket subscriber disconnected");
}
