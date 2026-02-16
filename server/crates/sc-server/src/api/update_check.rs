//! Agent update-check endpoint.
//!
//! Agents periodically poll this endpoint to discover if a newer version is
//! available.  The server reads `./agent-builds/manifest.json` to answer.
//!
//! Endpoint:
//!   GET /api/agent/update-check?version={current}&os={os}&arch={arch}
//!
//! No authentication required — agents call this before they even register.

use axum::{extract::Query, routing::get, Json, Router};
use serde::{Deserialize, Serialize};

pub fn router() -> Router {
    Router::new().route("/update-check", get(update_check))
}

// ─── Types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct UpdateCheckQuery {
    version: String,
    os: String,
    arch: String,
}

#[derive(Debug, Serialize)]
struct UpdateCheckResponse {
    update_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    download_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mandatory: Option<bool>,
}

/// Manifest schema matching `./agent-builds/manifest.json`.
#[derive(Debug, Deserialize)]
struct AgentManifest {
    version: String,
    builds: std::collections::HashMap<String, BuildEntry>,
    #[serde(default)]
    release_notes: String,
    #[serde(default)]
    mandatory: bool,
}

#[derive(Debug, Deserialize)]
struct BuildEntry {
    file: String,
    sha256: String,
    #[serde(default)]
    size: u64,
}

// ─── Handler ─────────────────────────────────────────────────

async fn update_check(Query(params): Query<UpdateCheckQuery>) -> Json<UpdateCheckResponse> {
    // Read manifest from disk
    let manifest = match read_manifest().await {
        Some(m) => m,
        None => {
            tracing::debug!("No agent manifest found — update check returning false");
            return Json(UpdateCheckResponse {
                update_available: false,
                version: None,
                download_url: None,
                sha256: None,
                release_notes: None,
                mandatory: None,
            });
        }
    };

    // Check if the manifest version is newer than the agent's current version
    if !is_newer(&manifest.version, &params.version) {
        return Json(UpdateCheckResponse {
            update_available: false,
            version: None,
            download_url: None,
            sha256: None,
            release_notes: None,
            mandatory: None,
        });
    }

    // Look up the build for this os-arch combination
    let build_key = format!("{}-{}", params.os, params.arch);
    let build = match manifest.builds.get(&build_key) {
        Some(b) => b,
        None => {
            tracing::debug!(
                "No build found for {} in manifest (available: {:?})",
                build_key,
                manifest.builds.keys().collect::<Vec<_>>()
            );
            return Json(UpdateCheckResponse {
                update_available: false,
                version: None,
                download_url: None,
                sha256: None,
                release_notes: None,
                mandatory: None,
            });
        }
    };

    let download_url = format!("/api/downloads/{}", build.file);

    Json(UpdateCheckResponse {
        update_available: true,
        version: Some(manifest.version),
        download_url: Some(download_url),
        sha256: Some(build.sha256.clone()),
        release_notes: if manifest.release_notes.is_empty() {
            None
        } else {
            Some(manifest.release_notes)
        },
        mandatory: Some(manifest.mandatory),
    })
}

/// Read and parse the agent builds manifest.
async fn read_manifest() -> Option<AgentManifest> {
    let manifest_path = std::path::Path::new("./agent-builds/manifest.json");

    let data = match tokio::fs::read_to_string(manifest_path).await {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!("Cannot read agent manifest: {}", e);
            return None;
        }
    };

    match serde_json::from_str(&data) {
        Ok(m) => Some(m),
        Err(e) => {
            tracing::warn!("Failed to parse agent manifest: {}", e);
            None
        }
    }
}

/// Simple semver comparison: returns true if `server` is newer than `agent`.
///
/// Handles versions like "0.1.0", "1.2.3", etc.
/// Falls back to string comparison if parsing fails.
fn is_newer(server: &str, agent: &str) -> bool {
    let parse = |v: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() >= 3 {
            Some((
                parts[0].parse().ok()?,
                parts[1].parse().ok()?,
                parts[2].parse().ok()?,
            ))
        } else {
            None
        }
    };

    match (parse(server), parse(agent)) {
        (Some(s), Some(a)) => s > a,
        _ => server != agent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }
}
