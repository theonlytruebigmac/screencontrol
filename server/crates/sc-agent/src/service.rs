//! Cross-platform service installer for ScreenControl Agent.
//!
//! Supports:
//! - **Linux**: systemd service unit (runs as root)
//! - **macOS**: launchd plist (LaunchDaemon, runs as root)
//! - **Windows**: Windows Service via `sc.exe` (runs as LocalSystem)
//!
//! Usage:
//!   sc-agent install --server-url ws://... --token ... [--group "Group Name"] [--silent]
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

// ─── Uniform directory helpers ─────────────────────────────────────

/// Directory where the agent binary lives.
pub fn install_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into()))
            .join("ScreenControl")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/Application Support/ScreenControl")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/opt/screencontrol")
    }
}

/// Directory where configuration files are stored.
pub fn config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into()))
            .join("ScreenControl")
    }
    #[cfg(target_os = "macos")]
    {
        // macOS keeps config alongside binary (in Application Support)
        PathBuf::from("/Library/Application Support/ScreenControl")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/etc/screencontrol")
    }
}

/// Directory for agent log files.
pub fn log_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into()))
            .join("ScreenControl")
            .join("logs")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/var/log")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/var/log/screencontrol")
    }
}

/// Directory for persistent agent data (cache, state, etc.).
pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into()))
            .join("ScreenControl")
            .join("data")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/Application Support/ScreenControl/data")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/var/lib/screencontrol")
    }
}

/// Path to the config file on this platform.
pub fn config_file() -> PathBuf {
    config_dir().join("config.env")
}

// ─── Install / Uninstall ───────────────────────────────────────────

/// Install the agent as a system service.
pub fn install(config: InstallConfig) -> anyhow::Result<()> {
    let src_exe = std::env::current_exe()?;
    let bin_dir = install_dir();
    let cfg_dir = config_dir();
    let lg_dir = log_dir();
    let dt_dir = data_dir();

    // Create all directories
    for dir in [&bin_dir, &cfg_dir, &lg_dir, &dt_dir] {
        std::fs::create_dir_all(dir)?;
        tracing::info!("Ensured directory: {:?}", dir);
    }

    // Copy agent binary to install directory
    let exe_name = if cfg!(windows) {
        "sc-agent.exe"
    } else {
        "sc-agent"
    };
    let dest_exe = bin_dir.join(exe_name);
    if src_exe != dest_exe {
        std::fs::copy(&src_exe, &dest_exe)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest_exe, std::fs::Permissions::from_mode(0o755))?;
        }
        tracing::info!("Copied binary to {:?}", dest_exe);
    }

    // Write configuration
    let cfg_path = config_file();
    let mut env_content = format!(
        "SC_SERVER_URL={}\nSC_TENANT_TOKEN={}\n",
        config.server_url, config.token
    );
    if !config.group.is_empty() {
        env_content.push_str(&format!("SC_GROUP={}\n", config.group));
    }
    // Point log output to our log directory
    env_content.push_str(&format!(
        "SC_LOG_DIR={}\nSC_DATA_DIR={}\n",
        lg_dir.display(),
        dt_dir.display()
    ));
    std::fs::write(&cfg_path, &env_content)?;
    tracing::info!("Wrote configuration to {:?}", cfg_path);

    // Also write a legacy `.env` next to the binary for backward compat
    let legacy_env = bin_dir.join(".env");
    if cfg_path != legacy_env {
        std::fs::write(&legacy_env, &env_content)?;
    }

    // Install platform-specific service
    #[cfg(target_os = "linux")]
    install_systemd(&dest_exe)?;

    #[cfg(target_os = "macos")]
    install_launchd(&dest_exe)?;

    #[cfg(target_os = "windows")]
    install_windows_service(&dest_exe)?;

    println!("✅ ScreenControl Agent installed successfully");
    println!("   Binary:  {}", dest_exe.display());
    println!("   Config:  {}", cfg_path.display());
    println!("   Logs:    {}", lg_dir.display());
    println!("   Data:    {}", dt_dir.display());
    println!("   Server:  {}", config.server_url);
    if !config.group.is_empty() {
        println!("   Group:   {}", config.group);
    }
    Ok(())
}

/// Uninstall the system service and clean up directories.
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
    let cfg_file = config_file();
    let lg_dir = log_dir();
    let dt_dir = data_dir();

    let unit_content = format!(
        r#"[Unit]
Description={description}
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=300
StartLimitBurst=5

[Service]
Type=simple
ExecStart={exe}
Restart=always
RestartSec=5
EnvironmentFile=-{config_file}

# Logging — use journal with syslog identifier
StandardOutput=journal
StandardError=journal
SyslogIdentifier=screencontrol-agent

# Runs as root for screen capture / input injection access
# (similar to ScreenConnect agent)

# Allow writes to our directories
ReadWritePaths={log_dir} {data_dir} /tmp

# Restart policies
TimeoutStopSec=10

[Install]
WantedBy=multi-user.target
"#,
        description = SERVICE_DESCRIPTION,
        exe = exe_path.display(),
        config_file = cfg_file.display(),
        log_dir = lg_dir.display(),
        data_dir = dt_dir.display(),
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
    println!("   Logs:    journalctl -u {} -f", SERVICE_NAME);
    println!("   Stop:    sudo systemctl stop {}", SERVICE_NAME);
    println!("   Restart: sudo systemctl restart {}", SERVICE_NAME);
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

    // Clean up directories
    let dirs_to_clean = [install_dir(), config_dir(), log_dir(), data_dir()];
    for dir in &dirs_to_clean {
        if dir.exists() {
            let _ = std::fs::remove_dir_all(dir);
            tracing::info!("Removed directory: {:?}", dir);
        }
    }

    println!("✅ Service and files uninstalled: {}", SERVICE_NAME);
    Ok(())
}

// ─── macOS: launchd ────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn install_launchd(exe_path: &PathBuf) -> anyhow::Result<()> {
    let label = "com.screencontrol.agent";
    let plist_path = format!("/Library/LaunchDaemons/{}.plist", label);
    let install_dir = exe_path.parent().unwrap_or(std::path::Path::new(
        "/Library/Application Support/ScreenControl",
    ));

    // ── Create .app bundle so macOS shows the proper icon in TCC panels ──
    let app_dir = install_dir.join("ScreenControl.app");
    let contents_dir = app_dir.join("Contents");
    let macos_dir = contents_dir.join("MacOS");
    let resources_dir = contents_dir.join("Resources");

    std::fs::create_dir_all(&macos_dir)?;
    std::fs::create_dir_all(&resources_dir)?;

    // Create data directory
    let dt_dir = data_dir();
    std::fs::create_dir_all(&dt_dir)?;

    // Copy binary into the .app bundle
    let bundle_exe = macos_dir.join("sc-agent");
    std::fs::copy(exe_path, &bundle_exe)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bundle_exe, std::fs::Permissions::from_mode(0o755))?;
    }
    tracing::info!("Created .app bundle at {:?}", app_dir);

    // Copy icon into Resources (look next to the source binary first, then install dir)
    let icon_candidates = [
        exe_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("assets/icon.icns"),
        install_dir.join("icon.icns"),
        std::env::current_dir()
            .unwrap_or_default()
            .join("crates/sc-agent/assets/icon.icns"),
    ];
    for icon_src in &icon_candidates {
        if icon_src.exists() {
            std::fs::copy(icon_src, resources_dir.join("icon.icns"))?;
            tracing::info!("Copied icon from {:?}", icon_src);
            break;
        }
    }

    // Write Info.plist for the .app bundle
    let info_plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>ScreenControl</string>
    <key>CFBundleDisplayName</key>
    <string>ScreenControl</string>
    <key>CFBundleIdentifier</key>
    <string>{label}</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleExecutable</key>
    <string>sc-agent</string>
    <key>CFBundleIconFile</key>
    <string>icon</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>ScreenControl needs microphone access to share audio during remote sessions.</string>
    <key>NSCameraUsageDescription</key>
    <string>ScreenControl may use the camera for remote sessions.</string>
</dict>
</plist>
"#,
        label = label,
    );
    std::fs::write(contents_dir.join("Info.plist"), &info_plist)?;

    // ── Codesign the .app bundle with a stable identifier ──────────
    let codesign_status = std::process::Command::new("codesign")
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

    match codesign_status {
        Ok(s) if s.success() => {
            tracing::info!("Signed .app bundle with identifier '{}'", label);
        }
        Ok(s) => {
            tracing::warn!(
                "codesign exited with status {} — TCC may re-prompt on updates",
                s
            );
        }
        Err(e) => {
            tracing::warn!("codesign failed: {} — TCC may re-prompt on updates", e);
        }
    }

    // ── Read environment variables from config file ──
    let cfg_file = config_file();

    let mut env_entries = String::new();
    if cfg_file.exists() {
        if let Ok(contents) = std::fs::read_to_string(&cfg_file) {
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

    // Always include RUST_LOG for debugging
    if !env_entries.contains("RUST_LOG") {
        env_entries
            .push_str("            <key>RUST_LOG</key>\n            <string>info</string>\n");
    }

    // ── Write LaunchDaemon plist pointing to the .app bundle binary ──
    let daemon_plist_content = format!(
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
    <key>WorkingDirectory</key>
    <string>{working_dir}</string>
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
        exe = bundle_exe.display(),
        working_dir = install_dir.display(),
        env_entries = env_entries,
    );

    std::fs::write(&plist_path, &daemon_plist_content)?;
    tracing::info!("Wrote LaunchDaemon plist: {}", plist_path);

    // Unload any existing daemon, then load the new one
    let _ = std::process::Command::new("launchctl")
        .args(["unload", "-w", &plist_path])
        .status();

    let status = std::process::Command::new("launchctl")
        .args(["load", "-w", &plist_path])
        .status()?;

    if !status.success() {
        anyhow::bail!("launchctl load failed — are you running as root (sudo)?");
    }

    println!("✅ LaunchDaemon installed and started: {}", label);
    println!("   App bundle: {}", app_dir.display());
    println!("   Logs: /var/log/screencontrol-agent.log");
    println!("   Stop: sudo launchctl unload {}", plist_path);
    println!();
    println!("   Run 'sc-agent setup' to grant Screen Recording & Accessibility permissions.");
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_launchd() -> anyhow::Result<()> {
    let label = "com.screencontrol.agent";

    // Unload and remove LaunchDaemon
    let daemon_plist = format!("/Library/LaunchDaemons/{}.plist", label);
    if std::path::Path::new(&daemon_plist).exists() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", "-w", &daemon_plist])
            .status();
        std::fs::remove_file(&daemon_plist)?;
        tracing::info!("Removed LaunchDaemon plist: {}", daemon_plist);
    }

    // Also clean up any legacy user LaunchAgent
    if let Some(home) = dirs::home_dir() {
        let user_plist = home
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", label));
        if user_plist.exists() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", "-w", &user_plist.to_string_lossy()])
                .status();
            let _ = std::fs::remove_file(&user_plist);
            tracing::info!("Removed legacy LaunchAgent plist: {:?}", user_plist);
        }

        // Clean up user log directory if it exists
        let log_dir = home.join("Library/Logs/ScreenControl");
        if log_dir.exists() {
            let _ = std::fs::remove_dir_all(&log_dir);
        }
    }

    // Remove install directory
    let dir = install_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
        tracing::info!("Removed install directory: {:?}", dir);
    }

    // Remove data directory
    let dt = data_dir();
    if dt.exists() {
        let _ = std::fs::remove_dir_all(&dt);
    }

    println!("✅ Service uninstalled: {}", label);
    Ok(())
}

// ─── Windows: sc.exe service registration ──────────────────────────

#[cfg(target_os = "windows")]
fn install_windows_service(exe_path: &PathBuf) -> anyhow::Result<()> {
    // Create the Windows Service using sc.exe
    // The service runs as LocalSystem for full desktop/input access
    let status = std::process::Command::new("sc.exe")
        .args([
            "create",
            SERVICE_NAME,
            &format!("binPath= \"{}\"", exe_path.display()),
            &format!("DisplayName= \"{}\"", SERVICE_DISPLAY_NAME),
            "start= delayed-auto",
            "obj= LocalSystem",
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!("sc.exe create failed — are you running as Administrator?");
    }

    // Set description
    let _ = std::process::Command::new("sc.exe")
        .args(["description", SERVICE_NAME, SERVICE_DESCRIPTION])
        .status();

    // Configure failure recovery (restart on crash with escalating delays)
    let _ = std::process::Command::new("sc.exe")
        .args([
            "failure",
            SERVICE_NAME,
            "reset= 86400",
            "actions= restart/5000/restart/10000/restart/30000",
        ])
        .status();

    // Allow service to interact with desktop (needed for screen capture)
    let _ = std::process::Command::new("sc.exe")
        .args(["config", SERVICE_NAME, "type= interact", "type= own"])
        .status();

    // Start the service
    let status = std::process::Command::new("sc.exe")
        .args(["start", SERVICE_NAME])
        .status()?;
    if !status.success() {
        tracing::warn!("Service created but failed to start — check Event Viewer");
    }

    println!("✅ Service installed: {}", SERVICE_NAME);
    println!("   Account: LocalSystem");
    println!("   Startup: Delayed Auto-Start");
    println!("   Status:  sc.exe query {}", SERVICE_NAME);
    println!("   Stop:    sc.exe stop {}", SERVICE_NAME);
    println!("   Logs:    {}", log_dir().display());
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

    // Clean up directories
    let install = install_dir();
    if install.exists() {
        let _ = std::fs::remove_dir_all(&install);
        tracing::info!("Removed install directory: {:?}", install);
    }
    let config = config_dir();
    if config.exists() {
        let _ = std::fs::remove_dir_all(&config);
        tracing::info!("Removed config directory: {:?}", config);
    }

    println!("✅ Service and files uninstalled: {}", SERVICE_NAME);
    Ok(())
}
