//! # ScreenControl Agent
//!
//! Cross-platform remote control agent that runs as a system service.
//! Connects to the ScreenControl server via WebSocket, provides screen
//! capture, input injection, terminal, and file transfer capabilities.
//!
//! ## Usage
//!
//! ```sh
//! sc-agent                                           # Run in foreground (default)
//! sc-agent run                                       # Same as above
//! sc-agent install --server-url ws://... --token ...  # Install as system service
//! sc-agent install --server-url ws://... --token ... --group "My Group"
//! sc-agent uninstall                                 # Stop and remove system service
//! sc-agent version                                   # Print version info
//! ```

mod audio;
mod chat;
mod clipboard;
mod command;
mod connection;
mod consent;
mod cursor;
mod encoder;
mod ffmpeg;
mod filebrowser;
mod host_commands;
mod input;
mod screen;
mod service;
mod setup;
mod sysinfo_collector;
mod terminal;
mod updater;

use std::time::Duration;
use tracing_subscriber::EnvFilter;

/// On Linux, when running as a root system service, detect the active graphical
/// session's environment variables (DISPLAY, WAYLAND_DISPLAY,
/// DBUS_SESSION_BUS_ADDRESS, XDG_RUNTIME_DIR) by scanning /proc for a process
/// that belongs to a logged-in graphical user. Without these, Mutter D-Bus
/// screen capture and X11/Wayland input injection will fail.
#[cfg(target_os = "linux")]
fn adopt_graphical_session_env() {
    use std::fs;

    // Only needed when these aren't already set (i.e. running as a service).
    // We must have DBUS_SESSION_BUS_ADDRESS *and* a display variable — sudo
    // may preserve DISPLAY but strip DBUS_SESSION_BUS_ADDRESS.
    let has_display = std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();
    let has_dbus = std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok();
    if has_display && has_dbus {
        tracing::info!("Display + D-Bus environment already set, skipping auto-detection");
        return;
    }

    tracing::info!("Detecting active graphical session environment...");

    // The env vars we want to inherit
    let target_vars = [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_RUNTIME_DIR",
    ];

    // Try to find a graphical session user via loginctl
    let session_uid = find_graphical_session_uid();

    // Scan /proc for a process with the needed env vars.
    // We need at least DBUS_SESSION_BUS_ADDRESS to connect to Mutter.
    // On modern GNOME/Wayland, gnome-shell's environ often lacks DISPLAY
    // and WAYLAND_DISPLAY (those are set later by XWayland / child processes),
    // so we accept DBUS_SESSION_BUS_ADDRESS alone and derive the rest.
    let proc_entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("Cannot read /proc: {}", e);
            return;
        }
    };

    for entry in proc_entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only look at numeric (PID) directories
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        // If we have a target UID, filter by it
        if let Some(uid) = session_uid {
            if let Ok(status) = fs::read_to_string(entry.path().join("status")) {
                let proc_uid = status
                    .lines()
                    .find(|l| l.starts_with("Uid:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|s| s.parse::<u32>().ok());
                if proc_uid != Some(uid) {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Read the process UID + GID for later use
        let (proc_uid, proc_gid) = {
            if let Ok(status) = fs::read_to_string(entry.path().join("status")) {
                let uid = status
                    .lines()
                    .find(|l| l.starts_with("Uid:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|s| s.parse::<u32>().ok());
                let gid = status
                    .lines()
                    .find(|l| l.starts_with("Gid:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|s| s.parse::<u32>().ok());
                (uid, gid)
            } else {
                (None, None)
            }
        };

        // Skip root-owned processes (uid 0) — we want the session user
        if proc_uid == Some(0) {
            continue;
        }

        // Read the process environment
        let environ_path = entry.path().join("environ");
        let environ = match fs::read(&environ_path) {
            Ok(data) => data,
            Err(_) => continue,
        };

        // Parse null-separated environment variables
        let env_str = String::from_utf8_lossy(&environ);
        let vars: std::collections::HashMap<&str, &str> = env_str
            .split('\0')
            .filter_map(|entry| entry.split_once('='))
            .collect();

        // Accept any process with DBUS_SESSION_BUS_ADDRESS — this is the minimum
        // needed to connect to Mutter's D-Bus API. DISPLAY/WAYLAND_DISPLAY may not
        // be in the process environ on GNOME Wayland (set by XWayland later).
        let has_dbus = vars.contains_key("DBUS_SESSION_BUS_ADDRESS");

        if has_dbus {
            // Found a suitable process — adopt its display environment
            for var_name in &target_vars {
                if let Some(value) = vars.get(var_name) {
                    tracing::info!("Adopted {}={} from PID {}", var_name, value, name_str);
                    std::env::set_var(var_name, value);
                }
            }

            // Derive missing display variables from XDG_RUNTIME_DIR if possible
            if std::env::var("XDG_RUNTIME_DIR").is_err() {
                // Derive from UID: /run/user/{uid}
                if let Some(uid) = proc_uid {
                    let runtime_dir = format!("/run/user/{}", uid);
                    if std::path::Path::new(&runtime_dir).exists() {
                        tracing::info!("Derived XDG_RUNTIME_DIR={}", runtime_dir);
                        std::env::set_var("XDG_RUNTIME_DIR", &runtime_dir);
                    }
                }
            }

            if std::env::var("WAYLAND_DISPLAY").is_err() {
                if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
                    // Check if wayland-0 socket exists
                    let wayland_path = format!("{}/wayland-0", xdg);
                    if std::path::Path::new(&wayland_path).exists() {
                        tracing::info!("Derived WAYLAND_DISPLAY=wayland-0");
                        std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
                    }
                }
            }

            if std::env::var("DISPLAY").is_err() {
                // Default X display for XWayland
                std::env::set_var("DISPLAY", ":0");
                tracing::info!("Set default DISPLAY=:0");
            }

            // Store the session user's UID/GID so screen capture can seteuid to it
            if let Some(uid) = proc_uid {
                std::env::set_var("SC_SESSION_UID", uid.to_string());
                tracing::info!("Set SC_SESSION_UID={}", uid);
            }
            if let Some(gid) = proc_gid {
                std::env::set_var("SC_SESSION_GID", gid.to_string());
                tracing::info!("Set SC_SESSION_GID={}", gid);
            }
            tracing::info!(
                "Successfully adopted graphical session environment from PID {}",
                name_str
            );
            return;
        }
    }

    tracing::warn!("Could not find an active graphical session to adopt environment from");
}

/// Find the UID of the first active graphical session via loginctl.
#[cfg(target_os = "linux")]
fn find_graphical_session_uid() -> Option<u32> {
    use std::process::Command;

    // List sessions and find a graphical one
    let output = Command::new("loginctl")
        .args(["list-sessions", "--no-legend"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let session_id = parts[0];

        // Check if this session has Type=wayland or Type=x11
        let show_output = Command::new("loginctl")
            .args(["show-session", session_id, "-p", "Type", "-p", "Uid"])
            .output()
            .ok()?;
        let show_str = String::from_utf8_lossy(&show_output.stdout);

        let mut session_type = "";
        let mut uid: Option<u32> = None;

        for prop_line in show_str.lines() {
            if let Some(val) = prop_line.strip_prefix("Type=") {
                session_type = if val == "wayland" || val == "x11" {
                    val
                } else {
                    ""
                };
            }
            if let Some(val) = prop_line.strip_prefix("Uid=") {
                uid = val.parse().ok();
            }
        }

        if !session_type.is_empty() {
            if let Some(u) = uid {
                tracing::info!(
                    "Found graphical session {} (type={}, uid={})",
                    session_id,
                    session_type,
                    u
                );
                return Some(u);
            }
        }
    }

    None
}

#[cfg(not(target_os = "linux"))]
fn adopt_graphical_session_env() {
    // Not needed on macOS/Windows — those platforms handle session access differently
}

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // On Linux, adopt the active graphical session's environment if running as a service
    adopt_graphical_session_env();

    // Parse CLI subcommand and flags.
    // Skip macOS-injected "-psn_*" arguments (Process Serial Number) that
    // `open` adds when launching .app bundles.
    let args: Vec<String> = std::env::args().collect();
    let subcommand = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("-psn"))
        .map(|s| s.as_str())
        .unwrap_or("run");

    match subcommand {
        "install" => {
            let config = parse_install_flags(&args[2..])?;
            service::install(config)?;
            // Auto-launch setup on macOS only (TCC permissions)
            // Launch via the .app bundle so macOS attributes TCC permissions
            // to com.screencontrol.agent (the daemon's bundle ID)
            if cfg!(target_os = "macos") {
                let silent = args.iter().any(|a| a == "--silent");
                if !silent && !args.iter().any(|a| a == "--no-setup") {
                    println!("\nLaunching permission setup...");
                    let app_path = "/Library/Application Support/ScreenControl/ScreenControl.app";
                    let launched = std::process::Command::new("open")
                        .args(["-n", app_path, "--args", "setup"])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if !launched {
                        // Fallback: run setup in-process
                        setup::run_setup();
                    }
                }
            }
            return Ok(());
        }
        "setup" => {
            setup::run_setup();
        }
        "show-chat" => {
            // Chat helper process — spawned by the daemon to show the WebView
            // in the logged-in user's GUI session
            chat::run_chat_helper();
        }
        "uninstall" => {
            return service::uninstall();
        }
        "version" => {
            println!(
                "ScreenControl Agent v{} ({} {})",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
            );
            return Ok(());
        }
        "run" | _ => {
            // Fall through to normal agent operation
        }
    }

    tracing::info!(
        "ScreenControl Agent v{} starting...",
        env!("CARGO_PKG_VERSION")
    );

    // Load agent configuration
    let server_url = std::env::var("SC_SERVER_URL")
        .unwrap_or_else(|_| "ws://localhost:8080/ws/agent".to_string());
    let tenant_token = std::env::var("SC_TENANT_TOKEN").unwrap_or_default();
    let group_name = std::env::var("SC_GROUP").unwrap_or_else(|_| {
        if tenant_token.is_empty() {
            "unmanaged".to_string()
        } else {
            String::new()
        }
    });

    if tenant_token.is_empty() {
        tracing::warn!("SC_TENANT_TOKEN not set — registering into 'unmanaged' group");
    }

    // Collect system information
    let sys_info = sysinfo_collector::collect_system_info();
    tracing::info!(
        "Machine: {} | OS: {} {} | Arch: {}",
        sys_info.machine_name,
        sys_info.os,
        sys_info.os_version,
        sys_info.arch
    );

    let unattended =
        std::env::var("SC_UNATTENDED").unwrap_or_default() == "1" || is_orphan_process();

    if unattended {
        // ── Headless / service mode — no GUI, run tokio on main thread ──
        tracing::info!("Running in unattended (service) mode — chat via IPC helper");

        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async {
            let mut last_update_check = std::time::Instant::now()
                .checked_sub(Duration::from_secs(updater::UPDATE_CHECK_INTERVAL_SECS))
                .unwrap_or_else(std::time::Instant::now);

            loop {
                tracing::info!("Connecting to server: {}", server_url);

                // Create daemon-mode chat handle (file-based IPC → spawns helper)
                let (chat_handle, reply_rx) = chat::create_daemon();

                let reply_rx = std::sync::Arc::new(std::sync::Mutex::new(reply_rx));

                match connection::connect_and_run(
                    &server_url,
                    &tenant_token,
                    &group_name,
                    &sys_info,
                    chat_handle.clone(),
                    reply_rx.clone(),
                )
                .await
                {
                    Ok(()) => {
                        tracing::info!("Connection closed gracefully");
                    }
                    Err(e) => {
                        tracing::error!("Connection error: {}", e);
                    }
                }

                // Check for updates periodically between reconnection attempts
                if last_update_check.elapsed().as_secs() >= updater::UPDATE_CHECK_INTERVAL_SECS {
                    last_update_check = std::time::Instant::now();
                    match updater::check_and_apply(&server_url).await {
                        Ok(true) => {
                            tracing::info!("Update applied, exiting for service restart...");
                            std::process::exit(0);
                        }
                        Ok(false) => {} // No update
                        Err(e) => {
                            tracing::debug!("Update check error: {}", e);
                        }
                    }
                }

                tracing::info!("Reconnecting in 5 seconds...");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    } else {
        // ── Interactive mode — chat GUI on main thread, tokio on background ──

        // The GUI event loop must run on the main thread (platform requirement).
        // Tokio async runtime runs on a background thread.
        let (event_loop, chat_handle, reply_rx) = chat::create();

        let chat_handle_clone = chat_handle.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(async move {
                let mut last_update_check = std::time::Instant::now()
                    .checked_sub(Duration::from_secs(updater::UPDATE_CHECK_INTERVAL_SECS))
                    .unwrap_or_else(std::time::Instant::now);

                let reply_rx = std::sync::Arc::new(std::sync::Mutex::new(reply_rx));

                loop {
                    tracing::info!("Connecting to server: {}", server_url);

                    match connection::connect_and_run(
                        &server_url,
                        &tenant_token,
                        &group_name,
                        &sys_info,
                        chat_handle_clone.clone(),
                        reply_rx.clone(),
                    )
                    .await
                    {
                        Ok(()) => {
                            tracing::info!("Connection closed gracefully");
                        }
                        Err(e) => {
                            tracing::error!("Connection error: {}", e);
                        }
                    }

                    // Check for updates periodically between reconnection attempts
                    if last_update_check.elapsed().as_secs() >= updater::UPDATE_CHECK_INTERVAL_SECS
                    {
                        last_update_check = std::time::Instant::now();
                        match updater::check_and_apply(&server_url).await {
                            Ok(true) => {
                                tracing::info!("Update applied, exiting for service restart...");
                                std::process::exit(0);
                            }
                            Ok(false) => {} // No update
                            Err(e) => {
                                tracing::debug!("Update check error: {}", e);
                            }
                        }
                    }

                    tracing::info!("Reconnecting in 5 seconds...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            });
        });

        // Run GUI event loop on main thread (blocks forever)
        chat::run_event_loop(event_loop);
    }

    Ok(())
}

/// Check if the process is an orphan (parent PID == 1, i.e. adopted by init/launchd).
/// On Windows, `getppid` is not available — we always return `false`.
#[cfg(unix)]
fn is_orphan_process() -> bool {
    (unsafe { libc::getppid() }) == 1
}

#[cfg(not(unix))]
fn is_orphan_process() -> bool {
    false
}

/// Parse `--server-url`, `--token`, `--group` flags from install arguments.
fn parse_install_flags(args: &[String]) -> anyhow::Result<service::InstallConfig> {
    let mut server_url = None;
    let mut token = None;
    let mut group = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--server-url" => {
                i += 1;
                server_url = args.get(i).cloned();
            }
            "--token" => {
                i += 1;
                token = args.get(i).cloned();
            }
            "--group" => {
                i += 1;
                group = args.get(i).cloned();
            }
            "--no-setup" | "--silent" => {
                // Handled in main(), just skip here
            }
            other => {
                anyhow::bail!("Unknown install flag: {}", other);
            }
        }
        i += 1;
    }

    Ok(service::InstallConfig {
        server_url: server_url
            .ok_or_else(|| anyhow::anyhow!("--server-url is required for install"))?,
        token: token.ok_or_else(|| anyhow::anyhow!("--token is required for install"))?,
        group: group.unwrap_or_default(),
    })
}
