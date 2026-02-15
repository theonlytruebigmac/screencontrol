//! Consent dialog for unattended access approval.
//!
//! When a remote user requests a desktop session, the agent shows a native
//! dialog to the local user asking permission. This prevents unauthorized
//! screen viewing on attended machines.
//!
//! ## Platform Implementations
//!
//! - **Linux**: `zenity --question` dialog
//! - **macOS**: `osascript` AppleScript dialog
//! - **Windows**: PowerShell `MessageBox`

use std::time::Duration;

/// Result of a consent prompt.
#[derive(Debug)]
pub enum ConsentResult {
    /// User granted access.
    Granted,
    /// User explicitly denied access.
    Denied,
    /// Dialog timed out without user action.
    TimedOut,
    /// No display server available (headless/service mode) — auto-grant.
    NoDisplay,
}

/// Show a consent dialog to the local user.
///
/// Returns the user's decision with a timeout. If no display server is
/// available (e.g. the agent is running as a headless service), access is
/// auto-granted.
pub async fn prompt_user(
    requester_name: &str,
    session_type: &str,
    timeout: Duration,
) -> ConsentResult {
    let requester = requester_name.to_string();
    let stype = session_type.to_string();
    let timeout_secs = timeout.as_secs().max(5);

    // Run in blocking thread since native dialogs block
    let result = tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || show_native_dialog(&requester, &stype, timeout_secs)),
    )
    .await;

    match result {
        Ok(Ok(Ok(true))) => ConsentResult::Granted,
        Ok(Ok(Ok(false))) => ConsentResult::Denied,
        Ok(Ok(Err(e))) => {
            tracing::debug!(
                "Consent dialog failed: {} — auto-granting (headless mode)",
                e
            );
            ConsentResult::NoDisplay
        }
        Ok(Err(e)) => {
            tracing::error!("Consent task panicked: {}", e);
            ConsentResult::NoDisplay
        }
        Err(_) => ConsentResult::TimedOut,
    }
}

/// Show a native dialog and return true if granted.
fn show_native_dialog(
    requester: &str,
    session_type: &str,
    timeout_secs: u64,
) -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    return show_dialog_linux(requester, session_type, timeout_secs);

    #[cfg(target_os = "macos")]
    return show_dialog_macos(requester, session_type, timeout_secs);

    #[cfg(target_os = "windows")]
    return show_dialog_windows(requester, session_type, timeout_secs);

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (requester, session_type, timeout_secs);
        Ok(true) // Auto-grant on unknown platforms
    }
}

// ─── Linux: zenity ─────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn show_dialog_linux(
    requester: &str,
    session_type: &str,
    timeout_secs: u64,
) -> Result<bool, String> {
    // Check if DISPLAY or WAYLAND_DISPLAY is set (need a GUI session)
    let has_display = std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();

    if !has_display {
        return Err("No display server available".to_string());
    }

    let message = format!(
        "<b>{}</b> is requesting <b>{}</b> access to this computer.\n\nDo you want to allow this connection?",
        requester, session_type
    );

    let status = std::process::Command::new("zenity")
        .args([
            "--question",
            "--title=ScreenControl - Remote Access Request",
            &format!("--text={}", message),
            "--ok-label=Allow",
            "--cancel-label=Deny",
            &format!("--timeout={}", timeout_secs),
            "--width=400",
        ])
        .status()
        .map_err(|e| format!("zenity not found: {}", e))?;

    // zenity returns 0 for OK, 1 for Cancel, 5 for timeout
    Ok(status.success())
}

// ─── macOS: osascript ──────────────────────────────────────────

#[cfg(target_os = "macos")]
fn show_dialog_macos(
    requester: &str,
    session_type: &str,
    timeout_secs: u64,
) -> Result<bool, String> {
    // When running as a service (LaunchDaemon or LaunchAgent), we cannot
    // reliably show GUI dialogs. Auto-grant for unattended service mode.
    // Detection: ppid == 1 means launched by launchd, uid == 0 means root,
    // or SC_UNATTENDED=1 is set explicitly.
    extern "C" {
        fn getuid() -> u32;
        fn getppid() -> u32;
    }
    let uid = unsafe { getuid() };
    let ppid = unsafe { getppid() };
    let unattended_env = std::env::var("SC_UNATTENDED").unwrap_or_default() == "1";

    if uid == 0 || ppid == 1 || unattended_env {
        return Err(format!(
            "Running as service (uid={}, ppid={}, unattended={}) — auto-granting",
            uid, ppid, unattended_env
        ));
    }

    let script = format!(
        r#"display dialog "{requester} is requesting {session_type} access to this computer.\n\nDo you want to allow this connection?" \
        buttons {{"Deny", "Allow"}} default button "Deny" cancel button "Deny" \
        with title "ScreenControl - Remote Access Request" \
        giving up after {timeout}"#,
        requester = requester,
        session_type = session_type,
        timeout = timeout_secs,
    );

    let output = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("osascript failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // osascript returns "button returned:Allow" on Allow
    // Returns exit code 1 (error) on Deny or timeout
    if stdout.contains("Allow") && !stdout.contains("gave up:true") {
        Ok(true)
    } else {
        Ok(false)
    }
}

// ─── Windows: PowerShell MessageBox ────────────────────────────

#[cfg(target_os = "windows")]
fn show_dialog_windows(
    requester: &str,
    session_type: &str,
    timeout_secs: u64,
) -> Result<bool, String> {
    let message = format!(
        "{} is requesting {} access to this computer.\\n\\nDo you want to allow this connection?",
        requester, session_type
    );

    // Use a WScript.Shell Popup which supports timeout
    let ps_script = format!(
        r#"
$wsh = New-Object -ComObject WScript.Shell
$result = $wsh.Popup("{message}", {timeout}, "ScreenControl - Remote Access Request", 4 + 32)
# 6 = Yes, 7 = No, -1 = Timeout
if ($result -eq 6) {{ exit 0 }} else {{ exit 1 }}
"#,
        message = message,
        timeout = timeout_secs,
    );

    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
        .status()
        .map_err(|e| format!("PowerShell failed: {}", e))?;

    Ok(status.success())
}
