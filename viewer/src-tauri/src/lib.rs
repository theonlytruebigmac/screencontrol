use reqwest;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

#[derive(Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Serialize, Deserialize)]
struct SessionResponse {
    id: String,
}

/// Stores the deep link URL that launched the app, so the frontend
/// can retrieve it on mount (avoids race condition with event timing).
struct DeepLinkState(Mutex<Option<String>>);

/// Get and clear any pending deep link URL.
/// Called by the frontend on mount to check if we were launched via deep link.
#[tauri::command]
fn get_pending_deep_link(state: tauri::State<DeepLinkState>) -> Option<String> {
    state.0.lock().unwrap().take()
}

/// Authenticate with the ScreenControl server. Returns a JWT token.
#[tauri::command]
async fn login(server: String, email: String, password: String) -> Result<String, String> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}/api/auth/login", server.trim_end_matches('/')))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Login failed ({}): {}", status, body));
    }

    let login_res: LoginResponse = res.json().await.map_err(|e| format!("Parse error: {}", e))?;
    Ok(login_res.token)
}

/// Create a new desktop session for an agent. Returns { id: "..." }.
#[tauri::command]
async fn create_session(server: String, token: String, agent_id: String) -> Result<SessionResponse, String> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{}/api/sessions", server.trim_end_matches('/')))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "session_type": "desktop"
        }))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Create session failed ({}): {}", status, body));
    }

    res.json::<SessionResponse>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// Build the WebSocket URL for a session.
#[tauri::command]
fn get_ws_url(server: String, session_id: String) -> String {
    let base = server.trim_end_matches('/');
    let ws_base = if base.starts_with("https://") {
        base.replace("https://", "wss://")
    } else {
        base.replace("http://", "ws://")
    };
    format!("{}/ws/console/{}", ws_base, session_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let deep_link_state = DeepLinkState(Mutex::new(None));

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(deep_link_state);

    // On desktop: single-instance so deep links go to the existing window
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            println!("[SingleInstance] new instance with argv: {:?}", argv);
            // Forward the deep-link URL to the running frontend
            if let Some(url) = argv.iter().find(|a| a.starts_with("screencontrol://")) {
                let _ = app.emit("deep-link-received", url.clone());
            }
        }));
    }

    builder = builder.plugin(tauri_plugin_deep_link::init());

    builder
        .invoke_handler(tauri::generate_handler![login, create_session, get_ws_url, get_pending_deep_link])
        .setup(|app| {
            // Check if app was started via a deep link (first launch)
            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;

                if let Ok(Some(urls)) = app.deep_link().get_current() {
                    println!("[DeepLink] started with URLs: {:?}", urls);
                    if let Some(url) = urls.first() {
                        let url_str = url.to_string();
                        println!("[DeepLink] storing pending URL: {}", url_str);

                        // Store in managed state so frontend can read it on mount
                        let state = app.state::<DeepLinkState>();
                        *state.0.lock().unwrap() = Some(url_str.clone());

                        // Also emit event after delay as backup
                        let handle = app.handle().clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                            let _ = handle.emit("deep-link-received", url_str);
                        });
                    }
                }

                // Listen for deep links while app is already running
                app.deep_link().on_open_url(|event| {
                    println!("[DeepLink] on_open_url: {:?}", event.urls());
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
