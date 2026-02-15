//! In-memory registry for live WebSocket connections.
//!
//! Tracks which agents and consoles are connected to **this** server instance.
//! Cross-instance routing goes through Redis pub/sub (see `services::pubsub`).

use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::Message;
use dashmap::DashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Channel sender capable of pushing WS frames to a connected peer.
pub type WsSender = mpsc::UnboundedSender<Message>;

/// Real-time metrics from the agent's latest heartbeat.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AgentMetrics {
    pub cpu_usage: f64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub uptime_secs: u32,
    pub ip_address: String,
    pub logged_in_user: String,
    pub cpu_model: String,
}

/// Metadata kept for each connected agent.
#[derive(Debug, Clone)]
pub struct AgentConnection {
    pub agent_id: Uuid,
    pub machine_name: String,
    pub tx: WsSender,
    pub metrics: AgentMetrics,
    /// When the server last issued a thumbnail upload URL to this agent.
    pub last_thumbnail_at: Option<Instant>,
}

/// Sender handles for both sides of an active session.
#[derive(Debug, Clone)]
pub struct SessionBinding {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub agent_tx: WsSender,
    pub console_tx: Option<WsSender>,
}

/// Central registry shared across all WebSocket handler tasks.
#[derive(Debug, Default)]
pub struct ConnectionRegistry {
    /// agent_id → connection handle
    pub agents: DashMap<Uuid, AgentConnection>,
    /// session_id → bound agent + console senders
    pub sessions: DashMap<Uuid, SessionBinding>,
    /// Event subscribers — UI clients listening for real-time status events
    pub event_subs: DashMap<Uuid, WsSender>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
            sessions: DashMap::new(),
            event_subs: DashMap::new(),
        }
    }

    // ─── Agent lifecycle ─────────────────────────────────────

    /// Register a newly-connected agent.
    pub fn register_agent(&self, agent_id: Uuid, machine_name: String, tx: WsSender) {
        tracing::info!(%agent_id, %machine_name, "Agent registered in connection registry");
        self.agents.insert(
            agent_id,
            AgentConnection {
                agent_id,
                machine_name: machine_name.clone(),
                tx,
                metrics: AgentMetrics::default(),
                last_thumbnail_at: None,
            },
        );
        // Broadcast status event to all UI subscribers
        self.broadcast_event(&serde_json::json!({
            "type": "agent.status",
            "agent_id": agent_id.to_string(),
            "machine_name": machine_name,
            "status": "online"
        }));
    }

    /// Remove an agent (on disconnect).
    pub fn unregister_agent(&self, agent_id: &Uuid) {
        let machine_name = self.agents.get(agent_id).map(|c| c.machine_name.clone());
        if self.agents.remove(agent_id).is_some() {
            tracing::info!(%agent_id, "Agent unregistered from connection registry");
            // Broadcast status event
            self.broadcast_event(&serde_json::json!({
                "type": "agent.status",
                "agent_id": agent_id.to_string(),
                "machine_name": machine_name.unwrap_or_default(),
                "status": "offline"
            }));
        }
        // Also remove any sessions bound to this agent
        self.sessions
            .retain(|_, binding| binding.agent_id != *agent_id);
    }

    /// Send a binary message to a connected agent.
    pub fn send_to_agent(&self, agent_id: &Uuid, data: Vec<u8>) -> bool {
        if let Some(conn) = self.agents.get(agent_id) {
            conn.tx.send(Message::Binary(data.into())).is_ok()
        } else {
            false
        }
    }

    // ─── Session lifecycle ───────────────────────────────────

    /// Bind a session to an agent (called when the server creates a session).
    pub fn bind_session(&self, session_id: Uuid, agent_id: Uuid) -> bool {
        if let Some(agent) = self.agents.get(&agent_id) {
            self.sessions.insert(
                session_id,
                SessionBinding {
                    session_id,
                    agent_id,
                    agent_tx: agent.tx.clone(),
                    console_tx: None,
                },
            );
            tracing::info!(%session_id, %agent_id, "Session bound to agent");
            true
        } else {
            tracing::warn!(%session_id, %agent_id, "Cannot bind session — agent not connected");
            false
        }
    }

    /// Attach a console sender to an existing session.
    pub fn attach_console(&self, session_id: &Uuid, tx: WsSender) -> bool {
        if let Some(mut binding) = self.sessions.get_mut(session_id) {
            binding.console_tx = Some(tx);
            tracing::info!(%session_id, "Console attached to session");
            true
        } else {
            false
        }
    }

    /// Remove a session binding.
    pub fn unbind_session(&self, session_id: &Uuid) {
        if self.sessions.remove(session_id).is_some() {
            tracing::info!(%session_id, "Session unbound");
        }
    }

    /// Send binary data to the console side of a session.
    pub fn send_to_console(&self, session_id: &Uuid, data: Vec<u8>) -> bool {
        if let Some(binding) = self.sessions.get(session_id) {
            if let Some(ref tx) = binding.console_tx {
                return tx.send(Message::Binary(data.into())).is_ok();
            }
        }
        false
    }

    /// Send binary data to the agent side of a session.
    pub fn send_to_session_agent(&self, session_id: &Uuid, data: Vec<u8>) -> bool {
        if let Some(binding) = self.sessions.get(session_id) {
            return binding.agent_tx.send(Message::Binary(data.into())).is_ok();
        }
        false
    }

    // ─── Metrics ─────────────────────────────────────────────

    /// Update real-time metrics for an agent (called on each heartbeat).
    pub fn update_agent_metrics(&self, agent_id: &Uuid, metrics: AgentMetrics) {
        if let Some(mut conn) = self.agents.get_mut(agent_id) {
            conn.metrics = metrics;
        }
    }

    /// Check whether enough time has elapsed to request a new thumbnail.
    /// If yes, updates the timestamp and returns `true`.
    pub fn should_capture_thumbnail(&self, agent_id: &Uuid, interval_secs: u64) -> bool {
        if let Some(mut conn) = self.agents.get_mut(agent_id) {
            let now = Instant::now();
            match conn.last_thumbnail_at {
                Some(last) if now.duration_since(last).as_secs() < interval_secs => false,
                _ => {
                    conn.last_thumbnail_at = Some(now);
                    true
                }
            }
        } else {
            false
        }
    }

    /// Unconditionally mark the thumbnail timestamp (e.g. after an on-connect capture).
    pub fn mark_thumbnail_sent(&self, agent_id: &Uuid) {
        if let Some(mut conn) = self.agents.get_mut(agent_id) {
            conn.last_thumbnail_at = Some(Instant::now());
        }
    }

    /// Get the latest metrics for an agent (returns None if not connected).
    pub fn get_agent_metrics(&self, agent_id: &Uuid) -> Option<AgentMetrics> {
        self.agents.get(agent_id).map(|c| c.metrics.clone())
    }

    // ─── Stats ───────────────────────────────────────────────

    pub fn online_agent_count(&self) -> usize {
        self.agents.len()
    }

    pub fn active_session_count(&self) -> usize {
        self.sessions.len()
    }

    // ─── Event subscribers ───────────────────────────────────

    /// Add a UI event subscriber.
    pub fn add_event_sub(&self, id: Uuid, tx: WsSender) {
        self.event_subs.insert(id, tx);
        tracing::debug!(%id, subs = self.event_subs.len(), "Event subscriber added");
    }

    /// Remove a UI event subscriber.
    pub fn remove_event_sub(&self, id: &Uuid) {
        self.event_subs.remove(id);
    }

    /// Broadcast a JSON event to all event subscribers.
    pub fn broadcast_event(&self, event: &serde_json::Value) {
        let text = event.to_string();
        let mut dead = Vec::new();
        for entry in self.event_subs.iter() {
            if entry
                .value()
                .send(Message::Text(text.clone().into()))
                .is_err()
            {
                dead.push(*entry.key());
            }
        }
        for id in dead {
            self.event_subs.remove(&id);
        }
    }
}
