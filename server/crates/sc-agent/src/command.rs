//! Command execution for remote shell commands.
//!
//! Runs shell commands requested by the server, captures stdout/stderr,
//! enforces timeout, and sends `CommandResponse` back over WebSocket.

use prost::Message as ProstMessage;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use uuid::Uuid;

use sc_protocol::{envelope, CommandResponse, Envelope};

/// Execute a command and send the `CommandResponse` over the WebSocket channel.
pub async fn execute_command(
    session_id: &str,
    command: &str,
    args: &[String],
    working_dir: &str,
    timeout_secs: u32,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
) {
    let timeout = if timeout_secs == 0 {
        Duration::from_secs(60) // default 60s
    } else {
        Duration::from_secs(timeout_secs as u64)
    };

    let shell = if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "sh"
    };

    let shell_flag = if cfg!(target_os = "windows") {
        "/C"
    } else {
        "-c"
    };

    // Build the full command string
    let full_cmd = if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    };

    let mut cmd = Command::new(shell);
    cmd.arg(shell_flag).arg(&full_cmd);

    if !working_dir.is_empty() {
        cmd.current_dir(working_dir);
    }

    let result = tokio::time::timeout(timeout, cmd.output()).await;

    let (exit_code, stdout, stderr, timed_out) = match result {
        Ok(Ok(output)) => (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            false,
        ),
        Ok(Err(e)) => (
            -1,
            String::new(),
            format!("Failed to execute: {}", e),
            false,
        ),
        Err(_) => (-1, String::new(), "Command timed out".to_string(), true),
    };

    tracing::info!(
        "Command '{}' exited with code {} (timed_out: {})",
        full_cmd,
        exit_code,
        timed_out
    );

    let envelope = Envelope {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        timestamp: None,
        payload: Some(envelope::Payload::CommandResponse(CommandResponse {
            exit_code,
            stdout,
            stderr,
            timed_out,
        })),
    };

    let mut buf = Vec::new();
    if envelope.encode(&mut buf).is_ok() {
        let _ = tx.send(buf);
    }
}
