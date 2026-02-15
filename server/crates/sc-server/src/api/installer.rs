//! Installer Script Generator API — generates platform-specific install scripts.
//!
//! Endpoints:
//!   GET /api/installer/script?os=linux&group_id=...  — returns a shell/PowerShell install script
//!   GET /api/installer/script?os=linux               — returns script without group assignment

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::header,
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::middleware::AuthUser;
use crate::AppState;
use sc_common::AppResult;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/script", get(generate_script))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct InstallerQuery {
    os: String,
    #[serde(default)]
    group_id: Option<Uuid>,
}

async fn generate_script(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(params): Query<InstallerQuery>,
) -> AppResult<impl IntoResponse> {
    // Look up the tenant's enrollment token for the authenticated user
    let tenant_token: (String,) = sqlx::query_as(
        "SELECT enrollment_token FROM tenants WHERE id = (SELECT tenant_id FROM users WHERE id = $1)"
    )
    .bind(_auth.user_id())
    .fetch_one(&state.db)
    .await
    .map_err(|e| sc_common::AppError::Internal(anyhow::anyhow!("Failed to look up tenant token: {}", e)))?;

    // Resolve group name if group_id was provided
    let group_name = if let Some(gid) = params.group_id {
        let row: Option<(String,)> = sqlx::query_as("SELECT name FROM agent_groups WHERE id = $1")
            .bind(gid)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
        row.map(|(name,)| name).unwrap_or_default()
    } else {
        String::new()
    };

    // Build the server URL — use the configured external URL or best guess
    let server_url = std::env::var("SC_EXTERNAL_URL")
        .unwrap_or_else(|_| "wss://your-server:8080/ws/agent".to_string());

    let (script, content_type, filename) = match params.os.as_str() {
        "linux" | "macos" => {
            let script = generate_bash_script(&server_url, &tenant_token.0, &group_name);
            (script, "text/x-shellscript", "install-screencontrol.sh")
        }
        "windows" => {
            let script = generate_powershell_script(&server_url, &tenant_token.0, &group_name);
            (script, "text/plain", "Install-ScreenControl.ps1")
        }
        _ => {
            return Err(sc_common::AppError::BadRequest(
                "Invalid OS — use 'linux', 'macos', or 'windows'".into(),
            ));
        }
    };

    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        script,
    ))
}

fn generate_bash_script(server_url: &str, token: &str, group_name: &str) -> String {
    let group_flag = if group_name.is_empty() {
        String::new()
    } else {
        format!(" --group \"{}\"", group_name)
    };

    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

# ScreenControl Agent Installer
# Generated automatically — do not edit.

INSTALL_DIR="/opt/screencontrol"
BINARY_NAME="sc-agent"
SERVER_URL="{server_url}"
TOKEN="{token}"

echo "═══════════════════════════════════════════════"
echo "  ScreenControl Agent Installer"
echo "═══════════════════════════════════════════════"

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)  ARTIFACT="sc-agent-linux-x86_64" ;;
    aarch64) ARTIFACT="sc-agent-linux-aarch64" ;;
    *)       echo "❌ Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Check root
if [ "$(id -u)" -ne 0 ]; then
    echo "❌ This installer must be run as root (use sudo)"
    exit 1
fi

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download agent binary (placeholder URL — update with real release URL)
DOWNLOAD_URL="${{SERVER_URL/ws:\/\//http://}}"
DOWNLOAD_URL="${{DOWNLOAD_URL/wss:\/\//https://}}"
DOWNLOAD_URL="${{DOWNLOAD_URL%%/ws/agent}}/api/downloads/agent/$ARTIFACT"

echo "📥 Downloading agent from $DOWNLOAD_URL ..."
if command -v curl &>/dev/null; then
    curl -fSL -o "$INSTALL_DIR/$BINARY_NAME" "$DOWNLOAD_URL"
elif command -v wget &>/dev/null; then
    wget -q -O "$INSTALL_DIR/$BINARY_NAME" "$DOWNLOAD_URL"
else
    echo "❌ Neither curl nor wget found"
    exit 1
fi

chmod +x "$INSTALL_DIR/$BINARY_NAME"
echo "✅ Agent binary installed to $INSTALL_DIR/$BINARY_NAME"

# Install as service with configuration
"$INSTALL_DIR/$BINARY_NAME" install --server-url "$SERVER_URL" --token "$TOKEN"{group_flag}

echo ""
echo "✅ ScreenControl Agent installed and running!"
echo "   View logs: journalctl -u screencontrol-agent -f"
"#,
        server_url = server_url,
        token = token,
        group_flag = group_flag,
    )
}

fn generate_powershell_script(server_url: &str, token: &str, group_name: &str) -> String {
    let group_flag = if group_name.is_empty() {
        String::new()
    } else {
        format!(" --group \"{}\"", group_name)
    };

    format!(
        r#"#Requires -RunAsAdministrator
# ScreenControl Agent Installer for Windows
# Generated automatically — do not edit.

$ErrorActionPreference = "Stop"

$InstallDir = "$env:ProgramFiles\ScreenControl"
$BinaryName = "sc-agent.exe"
$ServerUrl = "{server_url}"
$Token = "{token}"

Write-Host "═══════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "  ScreenControl Agent Installer" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════" -ForegroundColor Cyan

# Create install directory
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

# Download agent binary
$DownloadUrl = $ServerUrl -replace "wss://","https://" -replace "ws://","http://" -replace "/ws/agent","/api/downloads/agent/sc-agent-windows-x86_64.exe"

Write-Host "Downloading agent from $DownloadUrl ..."
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Invoke-WebRequest -Uri $DownloadUrl -OutFile "$InstallDir\$BinaryName" -UseBasicParsing

Write-Host "Agent binary installed to $InstallDir\$BinaryName" -ForegroundColor Green

# Install as service with configuration
& "$InstallDir\$BinaryName" install --server-url $ServerUrl --token $Token{group_flag}

Write-Host ""
Write-Host "ScreenControl Agent installed and running!" -ForegroundColor Green
Write-Host "   Status: sc.exe query screencontrol-agent"
"#,
        server_url = server_url,
        token = token,
        group_flag = group_flag,
    )
}
