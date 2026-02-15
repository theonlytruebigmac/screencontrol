//! Redis pub/sub for cross-instance session signaling.
//!
//! When running multiple `sc-server` instances, an agent may be connected
//! to instance A while the console connects to instance B.  This module
//! publishes/subscribes to Redis channels so that messages always reach
//! the correct local WebSocket connection.

use std::sync::Arc;

use futures_util::StreamExt;
use redis::AsyncCommands;
use tokio::task::JoinHandle;

use crate::AppState;

/// Redis channel name for routing messages to a specific agent.
pub fn agent_channel(agent_id: &uuid::Uuid) -> String {
    format!("sc:agent:{}", agent_id)
}

/// Redis channel name for session-level events.
pub fn session_channel(session_id: &uuid::Uuid) -> String {
    format!("sc:session:{}", session_id)
}

/// Publish a protobuf-encoded message to a Redis channel.
pub async fn publish(
    redis: &mut redis::aio::ConnectionManager,
    channel: &str,
    data: &[u8],
) -> anyhow::Result<()> {
    redis.publish::<_, _, ()>(channel, data).await?;
    Ok(())
}

/// Start a subscriber task for a given channel pattern.
/// Received messages are dispatched through the `ConnectionRegistry`.
pub fn start_subscriber(state: Arc<AppState>, pattern: &str) -> JoinHandle<()> {
    let pattern = pattern.to_string();

    tokio::spawn(async move {
        let redis_url = state.config.redis.url.clone();
        let client = match redis::Client::open(redis_url.as_str()) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Redis subscriber client error: {}", e);
                return;
            }
        };

        let mut pubsub = match client.get_async_pubsub().await {
            Ok(ps) => ps,
            Err(e) => {
                tracing::error!("Redis pub/sub connection error: {}", e);
                return;
            }
        };

        if let Err(e) = pubsub.psubscribe(&pattern).await {
            tracing::error!("Redis psubscribe error: {}", e);
            return;
        }

        tracing::info!("Redis subscriber listening on pattern: {}", pattern);

        let mut msg_stream = pubsub.on_message();

        while let Some(msg) = StreamExt::next(&mut msg_stream).await {
            let channel: String = match msg.get_channel() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let payload: Vec<u8> = match msg.get_payload() {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Route based on channel prefix
            if channel.starts_with("sc:agent:") {
                let agent_id_str = channel.strip_prefix("sc:agent:").unwrap_or_default();
                if let Ok(agent_id) = uuid::Uuid::parse_str(agent_id_str) {
                    state.registry.send_to_agent(&agent_id, payload);
                }
            } else if channel.starts_with("sc:session:") {
                let session_id_str = channel.strip_prefix("sc:session:").unwrap_or_default();
                if let Ok(session_id) = uuid::Uuid::parse_str(session_id_str) {
                    if !state.registry.send_to_console(&session_id, payload.clone()) {
                        state.registry.send_to_session_agent(&session_id, payload);
                    }
                }
            }
        }
    })
}

/// Start all default Redis subscribers for this server instance.
pub fn start_all_subscribers(state: Arc<AppState>) -> Vec<JoinHandle<()>> {
    vec![
        start_subscriber(state.clone(), "sc:agent:*"),
        start_subscriber(state, "sc:session:*"),
    ]
}
