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
            return service::install(config);
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
    let tenant_token = std::env::var("SC_TENANT_TOKEN").expect("SC_TENANT_TOKEN is required");
    let group_name = std::env::var("SC_GROUP").unwrap_or_default();

    // Collect system information
    let sys_info = sysinfo_collector::collect_system_info();
    tracing::info!(
        "Machine: {} | OS: {} {} | Arch: {}",
        sys_info.machine_name,
        sys_info.os,
        sys_info.os_version,
        sys_info.arch
    );

    // ── Create chat window infrastructure ────────────────────
    // The GUI event loop must run on the main thread (platform requirement).
    // Tokio async runtime runs on a background thread.
    let (event_loop, chat_handle, reply_rx) = chat::create();

    // ── Spawn tokio runtime on background thread ─────────────
    let chat_handle_clone = chat_handle.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async move {
            let mut last_update_check = std::time::Instant::now()
                .checked_sub(Duration::from_secs(updater::UPDATE_CHECK_INTERVAL_SECS))
                .unwrap_or_else(std::time::Instant::now);

            // Wrap the reply receiver for sharing across reconnection iterations
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
    });

    // ── Run GUI event loop on main thread (blocks forever) ───
    chat::run_event_loop(event_loop);
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
