//! # ScreenControl Relay
//!
//! Signaling + data relay server for NAT traversal.
//! When P2P connections fail, agents and consoles connect through the relay.

use std::net::SocketAddr;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

/// Shared relay state.
struct RelayState {
    /// Pending connections waiting for their peer.
    pending: DashMap<Uuid, mpsc::Sender<TcpStream>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let port: u16 = std::env::var("SC_RELAY_PORT")
        .unwrap_or_else(|_| "8041".to_string())
        .parse()?;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;

    let state = Arc::new(RelayState {
        pending: DashMap::new(),
    });

    tracing::info!("ScreenControl Relay listening on {}", addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            tracing::info!("Relay connection from {}", peer_addr);
            if let Err(e) = handle_relay_connection(stream, state).await {
                tracing::error!("Relay connection error: {}", e);
            }
        });
    }
}

async fn handle_relay_connection(
    mut stream: TcpStream,
    state: Arc<RelayState>,
) -> anyhow::Result<()> {
    // The first 16 bytes are the session UUID.
    // The next byte is the role: 0x01 = agent, 0x02 = console.
    let mut header = [0u8; 17];
    stream.read_exact(&mut header).await?;

    let session_id = Uuid::from_bytes(header[0..16].try_into()?);
    let role = header[16];

    tracing::info!(
        "Relay: session={}, role={}",
        session_id,
        if role == 1 { "agent" } else { "console" }
    );

    // Check if peer is already waiting
    if let Some((_, peer_tx)) = state.pending.remove(&session_id) {
        // Peer is waiting — send our stream to them for bridging
        let _ = peer_tx.send(stream).await;
        tracing::info!("Relay: paired session {}", session_id);
    } else {
        // We're the first — wait for peer
        let (tx, mut rx) = mpsc::channel::<TcpStream>(1);
        state.pending.insert(session_id, tx);

        if let Some(mut peer_stream) = rx.recv().await {
            // Bridge the two streams
            tracing::info!("Relay: bridging session {}", session_id);
            let (mut r1, mut w1) = stream.split();
            let (mut r2, mut w2) = peer_stream.split();

            let copy1 = tokio::io::copy(&mut r1, &mut w2);
            let copy2 = tokio::io::copy(&mut r2, &mut w1);

            tokio::select! {
                r = copy1 => { tracing::debug!("Relay copy1 done: {:?}", r); }
                r = copy2 => { tracing::debug!("Relay copy2 done: {:?}", r); }
            }
        }

        state.pending.remove(&session_id);
    }

    Ok(())
}
