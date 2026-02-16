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

mod chat;
mod command;
mod connection;
mod consent;
mod filebrowser;
mod input;
mod screen;
mod service;
mod setup;
mod sysinfo_collector;
mod terminal;
mod updater;

use std::time::Duration;
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Parse CLI subcommand and flags
    let args: Vec<String> = std::env::args().collect();
    let subcommand = args.get(1).map(|s| s.as_str()).unwrap_or("run");

    match subcommand {
        "install" => {
            let config = parse_install_flags(&args[2..])?;
            service::install(config)?;
            // Auto-launch setup unless --no-setup or --silent was passed
            let silent = args.iter().any(|a| a == "--silent");
            if !silent && !args.iter().any(|a| a == "--no-setup") {
                println!("\nLaunching permission setup...");
                setup::run_setup();
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

    let unattended = std::env::var("SC_UNATTENDED").unwrap_or_default() == "1"
        || unsafe { libc::getppid() } == 1;

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
