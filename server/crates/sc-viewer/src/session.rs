//! HTTP session management — authentication and session creation.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Serialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    access_token: String,
    // refresh_token, token_type, expires_in also present but unused
}

#[derive(Debug, Serialize)]
struct CreateSessionRequest {
    agent_id: String,
    session_type: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionInfo {
    pub id: String, // UUID as string
    #[allow(dead_code)]
    pub agent_id: String,
    #[allow(dead_code)]
    pub status: String,
}

/// Authenticate with the server and get a JWT token.
pub async fn login(server_url: &str, email: &str, password: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/auth/login", server_url);

    info!("Authenticating as {}", email);

    let resp = client
        .post(&url)
        .json(&LoginRequest {
            email: email.to_string(),
            password: password.to_string(),
        })
        .send()
        .await
        .context("Failed to send login request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Login failed ({}): {}", status, body);
    }

    let login_resp: LoginResponse = resp
        .json()
        .await
        .context("Failed to parse login response")?;

    info!("Authenticated successfully");
    Ok(login_resp.access_token)
}

/// Create a new desktop session for the given agent.
pub async fn create_session(server_url: &str, token: &str, agent_id: &str) -> Result<SessionInfo> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/sessions", server_url);

    info!("Creating desktop session for agent {}", agent_id);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&CreateSessionRequest {
            agent_id: agent_id.to_string(),
            session_type: "desktop".to_string(),
        })
        .send()
        .await
        .context("Failed to create session")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Create session failed ({}): {}", status, body);
    }

    let session: SessionInfo = resp
        .json()
        .await
        .context("Failed to parse session response")?;

    info!("Session created: {}", session.id);
    Ok(session)
}
