//! Agent manifest reader — caches the agent-builds/manifest.json
//! and provides update info for the heartbeat handler.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::RwLock;

/// Cached manifest state (read at startup, refreshed periodically).
static MANIFEST_CACHE: OnceLock<RwLock<Option<AgentManifest>>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
pub struct AgentManifest {
    pub version: String,
    pub builds: HashMap<String, BuildEntry>,
    #[serde(default)]
    pub release_notes: String,
    #[serde(default)]
    pub mandatory: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuildEntry {
    pub file: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
}

/// What the heartbeat handler needs to include in an ack.
#[derive(Debug, Clone)]
pub struct UpdateHint {
    pub version: String,
    pub download_url: String,
    pub sha256: String,
}

fn cache() -> &'static RwLock<Option<AgentManifest>> {
    MANIFEST_CACHE.get_or_init(|| RwLock::new(None))
}

/// Load (or reload) the manifest from disk into the global cache.
/// Called once at server startup and can be refreshed later.
pub async fn load_manifest() {
    let manifest_path = std::path::Path::new("./agent-builds/manifest.json");
    match tokio::fs::read_to_string(manifest_path).await {
        Ok(data) => match serde_json::from_str::<AgentManifest>(&data) {
            Ok(m) => {
                tracing::info!(
                    version = %m.version,
                    targets = m.builds.len(),
                    "Agent manifest loaded"
                );
                *cache().write().await = Some(m);
            }
            Err(e) => {
                tracing::warn!("Failed to parse agent manifest: {}", e);
            }
        },
        Err(e) => {
            tracing::debug!("No agent manifest found: {} — updates disabled", e);
        }
    }
}

/// Check if `agent_version` is older than the manifest version, and if so
/// return an `UpdateHint` with the download info for the given os/arch.
///
/// Returns `None` if no update is available or the manifest isn't loaded.
pub async fn check_for_update(agent_version: &str, os: &str, arch: &str) -> Option<UpdateHint> {
    let guard = cache().read().await;
    let manifest = guard.as_ref()?;

    if !is_newer(&manifest.version, agent_version) {
        return None;
    }

    let build_key = format!("{}-{}", os, arch);
    let build = manifest.builds.get(&build_key)?;

    Some(UpdateHint {
        version: manifest.version.clone(),
        download_url: format!("/api/downloads/{}", build.file),
        sha256: build.sha256.clone(),
    })
}

/// Simple semver comparison: returns true if `server` is newer than `agent`.
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
