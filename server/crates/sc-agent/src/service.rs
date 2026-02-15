//! Cross-platform service installer for ScreenControl Agent.
//!
//! Supports:
//! - **Linux**: systemd service unit
//! - **macOS**: launchd plist (LaunchDaemon)
//! - **Windows**: Windows Service via `sc.exe`
//!
//! Usage:
//!   sc-agent install --server-url ws://... --token ... [--group "Group Name"]
//!   sc-agent uninstall
//!   sc-agent run

use std::path::PathBuf;

#[allow(dead_code)]
const SERVICE_NAME: &str = "screencontrol-agent";
#[allow(dead_code)]
const SERVICE_DISPLAY_NAME: &str = "ScreenControl Agent";
#[allow(dead_code)]
const SERVICE_DESCRIPTION: &str = "ScreenControl remote access agent — provides screen capture, terminal, file transfer, and remote input.";

/// Configuration provided by `--server-url`, `--token`, `--group` install flags.
pub struct InstallConfig {
    pub server_url: String,
    pub token: String,
    pub group: String,
}

/// Get the standard install directory for this platform.
#[allow(dead_code)]
fn install_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into()))
            .join("ScreenControl")
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from("/opt/screencontrol")
    }
}

/// Install the agent as a system service.
pub fn install(config: InstallConfig) -> anyhow::Result<()> {
    let src_exe = std::env::current_exe()?;
    let dir = install_dir();

    // Create install directory
    std::fs::create_dir_all(&dir)?;
    tracing::info!("Install directory: {:?}", dir);

    // Copy agent binary to install directory
    let exe_name = if cfg!(windows) {
        "sc-agent.exe"
    } else {
        "sc-agent"
    };
    let dest_exe = dir.join(exe_name);
    if src_exe != dest_exe {
        std::fs::copy(&src_exe, &dest_exe)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest_exe, std::fs::Permissions::from_mode(0o755))?;
        }
        tracing::info!("Copied binary to {:?}", dest_exe);
    }

    // Write .env configuration
    let env_path = dir.join(".env");
    let mut env_content = format!(
        "SC_SERVER_URL={}\nSC_TENANT_TOKEN={}\n",
        config.server_url, config.token
    );
    if !config.group.is_empty() {
        env_content.push_str(&format!("SC_GROUP={}\n", config.group));
    }
    std::fs::write(&env_path, &env_content)?;
    tracing::info!("Wrote configuration to {:?}", env_path);

    // Install platform-specific service
    #[cfg(target_os = "linux")]
    install_systemd(&dest_exe)?;

    #[cfg(target_os = "macos")]
    install_launchd(&dest_exe)?;

    #[cfg(target_os = "windows")]
    install_windows_service(&dest_exe)?;

    println!("✅ ScreenControl Agent installed successfully");
    println!("   Directory: {}", dir.display());
    println!("   Server:    {}", config.server_url);
    if !config.group.is_empty() {
        println!("   Group:     {}", config.group);
    }
    Ok(())
}

/// Uninstall the system service.
pub fn uninstall() -> anyhow::Result<()> {
    tracing::info!("Uninstalling {}", SERVICE_NAME);

    #[cfg(target_os = "linux")]
    uninstall_systemd()?;

    #[cfg(target_os = "macos")]
    uninstall_launchd()?;

    #[cfg(target_os = "windows")]
    uninstall_windows_service()?;

    tracing::info!("Service uninstalled successfully");
    Ok(())
}

// ─── Linux: systemd ────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn install_systemd(exe_path: &PathBuf) -> anyhow::Result<()> {
    let unit_path = PathBuf::from("/etc/systemd/system").join(format!("{}.service", SERVICE_NAME));

    // Load environment file path (for SC_SERVER_URL, SC_TENANT_TOKEN)
    let env_file = exe_path
        .parent()
        .unwrap_or(std::path::Path::new("/opt/screencontrol"))
        .join(".env");

    let unit_content = format!(
        r#"[Unit]
Description={description}
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={exe}
Restart=always
RestartSec=5
EnvironmentFile=-{env_file}
# Security hardening
NoNewPrivileges=false
ProtectSystem=strict
ReadWritePaths=/tmp

[Install]
WantedBy=multi-user.target
"#,
        description = SERVICE_DESCRIPTION,
        exe = exe_path.display(),
        env_file = env_file.display(),
    );

    std::fs::write(&unit_path, unit_content)?;
    tracing::info!("Wrote systemd unit to {:?}", unit_path);

    // Reload systemd and enable the service
    let status = std::process::Command::new("systemctl")
        .args(["daemon-reload"])
        .status()?;
    if !status.success() {
        anyhow::bail!("systemctl daemon-reload failed");
    }

    let status = std::process::Command::new("systemctl")
        .args(["enable", "--now", SERVICE_NAME])
        .status()?;
    if !status.success() {
        anyhow::bail!("systemctl enable --now failed");
    }

    println!("✅ Service installed and started: {}", SERVICE_NAME);
    println!("   View logs:  journalctl -u {} -f", SERVICE_NAME);
    println!("   Stop:       sudo systemctl stop {}", SERVICE_NAME);
    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_systemd() -> anyhow::Result<()> {
    let _ = std::process::Command::new("systemctl")
        .args(["stop", SERVICE_NAME])
        .status();

    let _ = std::process::Command::new("systemctl")
        .args(["disable", SERVICE_NAME])
        .status();

    let unit_path = format!("/etc/systemd/system/{}.service", SERVICE_NAME);
    if std::path::Path::new(&unit_path).exists() {
        std::fs::remove_file(&unit_path)?;
    }

    let _ = std::process::Command::new("systemctl")
        .args(["daemon-reload"])
        .status();

    println!("✅ Service uninstalled: {}", SERVICE_NAME);
    Ok(())
}

// ─── macOS: launchd ────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn install_launchd(exe_path: &PathBuf) -> anyhow::Result<()> {
    let label = format!("com.screencontrol.agent");
    let plist_path = PathBuf::from("/Library/LaunchDaemons").join(format!("{}.plist", label));

    let env_file = exe_path
        .parent()
        .unwrap_or(std::path::Path::new("/opt/screencontrol"))
        .join(".env");

    // Build environment dict entries by reading .env if it exists
    let mut env_entries = String::new();
    if env_file.exists() {
        if let Ok(contents) = std::fs::read_to_string(&env_file) {
            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    env_entries.push_str(&format!(
                        "            <key>{}</key>\n            <string>{}</string>\n",
                        key.trim(),
                        value.trim()
                    ));
                }
            }
        }
    }

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/var/log/screencontrol-agent.log</string>
    <key>StandardErrorPath</key>
    <string>/var/log/screencontrol-agent.err</string>
    <key>EnvironmentVariables</key>
    <dict>
{env_entries}    </dict>
</dict>
</plist>
"#,
        label = label,
        exe = exe_path.display(),
        env_entries = env_entries,
    );

    std::fs::write(&plist_path, plist_content)?;
    tracing::info!("Wrote launchd plist to {:?}", plist_path);

    let status = std::process::Command::new("launchctl")
        .args(["load", "-w", &plist_path.to_string_lossy()])
        .status()?;
    if !status.success() {
        anyhow::bail!("launchctl load failed");
    }

    println!("✅ Service installed and started: {}", label);
    println!("   Logs: /var/log/screencontrol-agent.log");
    println!("   Stop: sudo launchctl unload {}", plist_path.display());
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_launchd() -> anyhow::Result<()> {
    let label = "com.screencontrol.agent";
    let plist_path = format!("/Library/LaunchDaemons/{}.plist", label);

    let _ = std::process::Command::new("launchctl")
        .args(["unload", "-w", &plist_path])
        .status();

    if std::path::Path::new(&plist_path).exists() {
        std::fs::remove_file(&plist_path)?;
    }

    println!("✅ Service uninstalled: {}", label);
    Ok(())
}

// ─── Windows: sc.exe service registration ──────────────────────────

#[cfg(target_os = "windows")]
fn install_windows_service(exe_path: &PathBuf) -> anyhow::Result<()> {
    // Use sc.exe to create the service
    let status = std::process::Command::new("sc.exe")
        .args([
            "create",
            SERVICE_NAME,
            &format!("binPath= \"{}\"", exe_path.display()),
            &format!("DisplayName= \"{}\"", SERVICE_DISPLAY_NAME),
            "start= auto",
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("sc.exe create failed — are you running as Administrator?");
    }

    // Set description
    let _ = std::process::Command::new("sc.exe")
        .args(["description", SERVICE_NAME, SERVICE_DESCRIPTION])
        .status();

    // Configure failure recovery (restart on crash)
    let _ = std::process::Command::new("sc.exe")
        .args([
            "failure",
            SERVICE_NAME,
            "reset= 86400",
            "actions= restart/5000/restart/10000/restart/30000",
        ])
        .status();

    // Start the service
    let status = std::process::Command::new("sc.exe")
        .args(["start", SERVICE_NAME])
        .status()?;
    if !status.success() {
        tracing::warn!("Service created but failed to start — check Event Viewer");
    }

    println!("✅ Service installed: {}", SERVICE_NAME);
    println!("   Status: sc.exe query {}", SERVICE_NAME);
    println!("   Stop:   sc.exe stop {}", SERVICE_NAME);
    Ok(())
}

#[cfg(target_os = "windows")]
fn uninstall_windows_service() -> anyhow::Result<()> {
    let _ = std::process::Command::new("sc.exe")
        .args(["stop", SERVICE_NAME])
        .status();

    // Small delay to let the service stop
    std::thread::sleep(std::time::Duration::from_secs(2));

    let status = std::process::Command::new("sc.exe")
        .args(["delete", SERVICE_NAME])
        .status()?;

    if !status.success() {
        anyhow::bail!("sc.exe delete failed — are you running as Administrator?");
    }

    println!("✅ Service uninstalled: {}", SERVICE_NAME);
    Ok(())
}
