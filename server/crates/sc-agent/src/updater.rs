//! Self-update mechanism for ScreenControl Agent.
//!
//! The agent periodically checks a release endpoint for newer versions.
//! If an update is available, it downloads the new binary, verifies its
//! integrity, replaces itself, and restarts via the service manager.
//!
//! ## Update Flow
//!
//! 1. Agent sends GET to `{server}/api/agent/update-check?version={current}&os={os}&arch={arch}`
//! 2. Server responds with JSON: `{ "update_available": true, "version": "1.2.3", "download_url": "...", "sha256": "..." }`
//! 3. Agent downloads new binary to a temp file
//! 4. Verifies SHA-256 checksum
//! 5. Replaces current binary (atomic rename on Linux/macOS, rename-and-restart on Windows)
//! 6. Triggers service restart

use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

/// How often to check for updates (in seconds).
pub const UPDATE_CHECK_INTERVAL_SECS: u64 = 3600; // 1 hour

/// Response from the update-check endpoint.
#[derive(serde::Deserialize, Debug)]
pub struct UpdateCheckResponse {
    pub update_available: bool,
    pub version: Option<String>,
    pub download_url: Option<String>,
    pub sha256: Option<String>,
}

/// Check for available updates and apply if found.
///
/// Returns `Ok(true)` if an update was applied (caller should restart),
/// `Ok(false)` if no update available, or `Err` on failure.
pub async fn check_and_apply(server_base_url: &str) -> anyhow::Result<bool> {
    let current_version = env!("CARGO_PKG_VERSION");
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    // Build the HTTP base URL from the WebSocket URL
    let http_base = server_base_url
        .replace("wss://", "https://")
        .replace("ws://", "http://")
        .trim_end_matches("/ws/agent")
        .to_string();

    let check_url = format!(
        "{}/api/agent/update-check?version={}&os={}&arch={}",
        http_base, current_version, os, arch
    );

    tracing::debug!("Checking for updates: {}", check_url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let resp = match client.get(&check_url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(
                "Update check failed (server may not support updates yet): {}",
                e
            );
            return Ok(false);
        }
    };

    if !resp.status().is_success() {
        tracing::debug!("Update check returned HTTP {}", resp.status());
        return Ok(false);
    }

    let update_info: UpdateCheckResponse = match resp.json().await {
        Ok(info) => info,
        Err(e) => {
            tracing::debug!("Failed to parse update response: {}", e);
            return Ok(false);
        }
    };

    if !update_info.update_available {
        tracing::debug!("Agent is up to date (v{})", current_version);
        return Ok(false);
    }

    let new_version = update_info.version.as_deref().unwrap_or("unknown");
    let download_url = match &update_info.download_url {
        Some(url) => url,
        None => {
            tracing::warn!("Update available but no download URL provided");
            return Ok(false);
        }
    };
    let expected_sha256 = update_info.sha256.as_deref();

    tracing::info!(
        "Update available: v{} → v{}, downloading...",
        current_version,
        new_version
    );

    // Download new binary
    let new_binary = download_binary(&client, download_url).await?;

    // Verify checksum
    if let Some(expected) = expected_sha256 {
        let actual = sha256_hex(&new_binary);
        if actual != expected {
            anyhow::bail!("Checksum mismatch: expected {}, got {}", expected, actual);
        }
        tracing::info!("Checksum verified: {}", actual);
    }

    // Replace current binary
    let current_exe = std::env::current_exe()?;
    replace_binary(&current_exe, &new_binary).await?;

    // On macOS, re-sign the .app bundle so TCC permissions persist.
    // The binary lives inside ScreenControl.app/Contents/MacOS/sc-agent,
    // so we walk up from the binary to find the .app directory.
    #[cfg(target_os = "macos")]
    {
        if let Some(macos_dir) = current_exe.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                if let Some(app_dir) = contents_dir.parent() {
                    if app_dir.extension().and_then(|e| e.to_str()) == Some("app") {
                        let label = "com.screencontrol.agent";
                        let sign_result = std::process::Command::new("codesign")
                            .args([
                                "--force",
                                "--deep",
                                "--sign",
                                "-",
                                "--identifier",
                                label,
                                app_dir.to_str().unwrap_or_default(),
                            ])
                            .status();
                        match sign_result {
                            Ok(s) if s.success() => {
                                tracing::info!("Re-signed .app bundle after update");
                            }
                            _ => {
                                tracing::warn!("Failed to re-sign .app bundle — TCC may re-prompt");
                            }
                        }
                    }
                }
            }
        }
    }

    tracing::info!(
        "Update applied: v{} → v{}. Restarting...",
        current_version,
        new_version
    );

    // Trigger restart via service manager
    restart_service();

    Ok(true)
}

/// Download a binary from a URL.
async fn download_binary(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<u8>> {
    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Download failed: HTTP {}", resp.status());
    }

    let bytes = resp.bytes().await?;
    tracing::info!("Downloaded {} bytes", bytes.len());

    Ok(bytes.to_vec())
}

/// Compute SHA-256 hex digest of data.
fn sha256_hex(data: &[u8]) -> String {
    use std::io::Write;

    // Simple SHA-256 using the command-line tool (avoids adding a crypto dependency)
    // In production, use the `sha2` crate instead
    let mut child = std::process::Command::new("sha256sum")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("sha256sum not found");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(data).ok();
    }

    let output = child.wait_with_output().expect("sha256sum failed");
    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout.split_whitespace().next().unwrap_or("").to_string()
}

/// Replace the current binary with a new one.
///
/// On Unix: write to temp file, chmod +x, rename over current binary.
/// On Windows: rename current to .old, write new binary, schedule cleanup.
async fn replace_binary(current_exe: &PathBuf, new_binary: &[u8]) -> anyhow::Result<()> {
    let parent = current_exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine binary directory"))?;

    let tmp_path = parent.join("sc-agent.update");

    // Write new binary to temp file
    let mut file = tokio::fs::File::create(&tmp_path).await?;
    file.write_all(new_binary).await?;
    file.flush().await?;
    drop(file);

    // Platform-specific replacement
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Make executable
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&tmp_path, perms)?;
        // Atomic rename
        std::fs::rename(&tmp_path, current_exe)?;
    }

    #[cfg(windows)]
    {
        let old_path = parent.join("sc-agent.old");
        // Move current to .old (Windows can't overwrite a running exe)
        let _ = std::fs::remove_file(&old_path);
        std::fs::rename(current_exe, &old_path)?;
        std::fs::rename(&tmp_path, current_exe)?;
    }

    Ok(())
}

/// Trigger a service restart to pick up the new binary.
fn restart_service() {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("systemctl")
            .args(["restart", "screencontrol-agent"])
            .spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let label = "com.screencontrol.agent";
        let plist = format!("/Library/LaunchDaemons/{}.plist", label);
        let _ = std::process::Command::new("launchctl")
            .args(["unload", &plist])
            .status();
        let _ = std::process::Command::new("launchctl")
            .args(["load", "-w", &plist])
            .status();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("sc.exe")
            .args(["stop", "screencontrol-agent"])
            .status();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::process::Command::new("sc.exe")
            .args(["start", "screencontrol-agent"])
            .status();
    }
}
