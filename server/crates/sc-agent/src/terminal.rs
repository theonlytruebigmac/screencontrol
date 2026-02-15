//! Terminal (PTY) management for remote shell sessions.
//!
//! Spawns a PTY, bridges read/write between the PTY and protobuf
//! TerminalData messages sent over WebSocket.

use std::collections::HashMap;
use std::io::{Read, Write};

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use prost::Message as ProstMessage;
use tokio::sync::mpsc;
use uuid::Uuid;

use sc_protocol::{envelope, Envelope, TerminalData};

/// Manages multiple concurrent terminal sessions for this agent.
pub struct TerminalManager {
    sessions: HashMap<String, TerminalSessionHandle>,
}

struct TerminalSessionHandle {
    /// Send data to the PTY's stdin.
    stdin_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Send resize commands.
    resize_tx: mpsc::UnboundedSender<(u16, u16)>,
    /// Handle for the reader task.
    reader_handle: tokio::task::JoinHandle<()>,
    /// Handle for the writer task.
    writer_handle: tokio::task::JoinHandle<()>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Spawn a new terminal session. PTY output is encoded as protobuf
    /// `TerminalData` envelopes and sent through `ws_tx`.
    pub fn spawn_session(
        &mut self,
        session_id: &str,
        cols: u16,
        rows: u16,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> anyhow::Result<()> {
        let pty_system = NativePtySystem::default();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Pick the default shell
        let shell = if cfg!(target_os = "windows") {
            "cmd.exe".to_string()
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
        };

        let mut cmd = CommandBuilder::new(&shell);
        if !cfg!(target_os = "windows") {
            cmd.arg("-l");
        }

        let _child = pair.slave.spawn_command(cmd)?;

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        // Channel for sending stdin data to the PTY
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        // Channel for resize commands
        let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<(u16, u16)>();

        let sid = session_id.to_string();

        // ── Reader task: PTY stdout → protobuf → WS ──────────
        let reader_sid = sid.clone();
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let envelope = Envelope {
                            id: Uuid::new_v4().to_string(),
                            session_id: reader_sid.clone(),
                            timestamp: None,
                            payload: Some(envelope::Payload::TerminalData(TerminalData {
                                data: buf[..n].to_vec(),
                            })),
                        };
                        let mut encoded = Vec::new();
                        if envelope.encode(&mut encoded).is_ok() {
                            if ws_tx.send(encoded).is_err() {
                                break; // WS channel closed
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("PTY read error (session {}): {}", reader_sid, e);
                        break;
                    }
                }
            }
            tracing::info!("Terminal reader exited for session {}", reader_sid);
        });

        // ── Writer task: WS → PTY stdin ──────────────────────
        let master_for_resize = pair.master;
        let writer_handle = tokio::spawn(async move {
            let mut writer = writer;
            loop {
                tokio::select! {
                    data = stdin_rx.recv() => {
                        match data {
                            Some(bytes) => {
                                if writer.write_all(&bytes).is_err() {
                                    break;
                                }
                                let _ = writer.flush();
                            }
                            None => break,
                        }
                    }
                    resize = resize_rx.recv() => {
                        match resize {
                            Some((cols, rows)) => {
                                let _ = master_for_resize.resize(PtySize {
                                    rows,
                                    cols,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                });
                                tracing::debug!("PTY resized to {}x{}", cols, rows);
                            }
                            None => break,
                        }
                    }
                }
            }
            tracing::info!("Terminal writer exited for session {}", sid);
        });

        self.sessions.insert(
            session_id.to_string(),
            TerminalSessionHandle {
                stdin_tx,
                resize_tx,
                reader_handle,
                writer_handle,
            },
        );

        tracing::info!(
            "Terminal session spawned: {} ({}x{})",
            session_id,
            cols,
            rows
        );
        Ok(())
    }

    /// Write data to a terminal session's stdin.
    pub fn write_to_session(&self, session_id: &str, data: Vec<u8>) -> bool {
        if let Some(handle) = self.sessions.get(session_id) {
            handle.stdin_tx.send(data).is_ok()
        } else {
            false
        }
    }

    /// Resize a terminal session.
    pub fn resize_session(&self, session_id: &str, cols: u16, rows: u16) -> bool {
        if let Some(handle) = self.sessions.get(session_id) {
            handle.resize_tx.send((cols, rows)).is_ok()
        } else {
            false
        }
    }

    /// Close a terminal session.
    pub fn close_session(&mut self, session_id: &str) {
        if let Some(handle) = self.sessions.remove(session_id) {
            handle.reader_handle.abort();
            handle.writer_handle.abort();
            tracing::info!("Terminal session closed: {}", session_id);
        }
    }

    /// Close all sessions (on agent shutdown).
    pub fn close_all(&mut self) {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        for id in ids {
            self.close_session(&id);
        }
    }
}
