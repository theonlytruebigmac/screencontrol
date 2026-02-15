//! # Embedded WebView Chat Window
//!
//! ScreenConnect-style persistent chat window embedded directly in the agent binary.
//! Uses `wry` (WebView) + `tao` (window management) to render an HTML/CSS chat UI
//! that matches the ScreenControl web application's design.
//!
//! ## Architecture
//!
//! **Interactive mode** (run from terminal):
//! ```text
//! Main Thread (GUI):  tao event loop → wry WebView
//!                          ↕ channels
//! Background Thread:  tokio runtime → WebSocket → connection.rs
//! ```
//!
//! **Daemon mode** (LaunchDaemon — no GUI session):
//! ```text
//! Daemon Process:  writes messages → /tmp/screencontrol-chat-inbox.jsonl
//!                  reads replies  ← /tmp/screencontrol-chat-outbox.jsonl
//!                  spawns →  sc-agent show-chat  (in user's GUI session)
//! Helper Process:  tao event loop → wry WebView
//!                  reads messages ← inbox file
//!                  writes replies → outbox file
//! ```

use std::io::Write;
use std::sync::{mpsc, Arc, Mutex};
#[cfg(target_os = "linux")]
use tao::platform::unix::WindowExtUnix;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    window::WindowBuilder,
};
use wry::WebViewBuilder;
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;

// ── IPC file paths for daemon mode ───────────────────────────
const CHAT_INBOX: &str = "/tmp/screencontrol-chat-inbox.jsonl";
const CHAT_OUTBOX: &str = "/tmp/screencontrol-chat-outbox.jsonl";

// ── Public types ──────────────────────────────────────────────

/// A message from the technician to display in the chat window.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub sender: String,
    pub content: String,
}

/// A reply from the local user back to the technician.
#[derive(Debug, Clone)]
pub struct OutgoingReply {
    pub content: String,
}

/// Custom events sent to the tao event loop from the async runtime.
#[derive(Debug)]
pub enum ChatEvent {
    /// Display a new incoming message.
    ShowMessage(IncomingMessage),
    /// Flush any pending messages now that the WebView JS is ready.
    WebViewReady,
}

/// Internal backend for ChatHandle
#[derive(Clone)]
enum ChatBackend {
    /// Interactive mode: sends events directly to the tao event loop
    WebView(EventLoopProxy<ChatEvent>),
    /// Daemon mode: writes messages to IPC file and spawns helper process
    Daemon { helper_spawned: Arc<Mutex<bool>> },
}

/// Clonable handle used by the connection loop to interact with the chat window.
#[derive(Clone)]
pub struct ChatHandle {
    backend: ChatBackend,
}

impl ChatHandle {
    pub fn show_message(&self, sender: String, content: String) {
        match &self.backend {
            ChatBackend::WebView(proxy) => {
                let _ =
                    proxy.send_event(ChatEvent::ShowMessage(IncomingMessage { sender, content }));
            }
            ChatBackend::Daemon { helper_spawned } => {
                // Write message to the IPC inbox file
                let escaped_sender = sender.replace('\\', "\\\\").replace('"', "\\\"");
                let escaped_content = content
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "");
                let line = format!(
                    "{{\"sender\":\"{}\",\"content\":\"{}\"}}\n",
                    escaped_sender, escaped_content
                );

                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(CHAT_INBOX)
                {
                    let _ = f.write_all(line.as_bytes());
                    let _ = f.flush();
                    tracing::info!("Wrote chat message to IPC inbox");
                }

                // Spawn the chat helper in the user's GUI session (once)
                let mut spawned = helper_spawned.lock().unwrap();
                if !*spawned {
                    *spawned = true;
                    spawn_chat_helper();
                }
            }
        }
    }
}

/// Create the event loop, chat handle, and reply receiver for **interactive mode**.
///
/// Returns:
/// - `EventLoop<ChatEvent>` — must be run on the main thread
/// - `ChatHandle` — clone and pass to the async runtime
/// - `mpsc::Receiver<OutgoingReply>` — poll from the async runtime for user replies
pub fn create() -> (
    EventLoop<ChatEvent>,
    ChatHandle,
    mpsc::Receiver<OutgoingReply>,
) {
    let event_loop = EventLoopBuilder::<ChatEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let (reply_tx, reply_rx) = mpsc::channel();

    // Store reply_tx in a static so the IPC handler can access it
    REPLY_TX.lock().unwrap().replace(reply_tx);

    let handle = ChatHandle {
        backend: ChatBackend::WebView(proxy),
    };
    (event_loop, handle, reply_rx)
}

/// Create a chat handle and reply receiver for **daemon mode** (no GUI).
///
/// Messages are written to an IPC file and a helper process is spawned
/// in the logged-in user's GUI session to display the chat WebView.
pub fn create_daemon() -> (ChatHandle, mpsc::Receiver<OutgoingReply>) {
    // Clean up stale IPC files from previous runs
    let _ = std::fs::remove_file(CHAT_INBOX);
    let _ = std::fs::remove_file(CHAT_OUTBOX);

    let (reply_tx, reply_rx) = mpsc::channel();

    // Background thread: watch the outbox file for replies from the helper
    std::thread::spawn(move || {
        let mut last_len: usize = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if let Ok(contents) = std::fs::read_to_string(CHAT_OUTBOX) {
                if contents.len() > last_len {
                    let new_part = &contents[last_len..];
                    for line in new_part.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        // Parse: {"content":"..."}
                        if let Some(start) = line.find("\"content\":\"") {
                            let rest = &line[start + 11..];
                            if let Some(end) = rest.rfind('"') {
                                let content = rest[..end]
                                    .replace("\\n", "\n")
                                    .replace("\\\"", "\"")
                                    .replace("\\\\", "\\");
                                let _ = reply_tx.send(OutgoingReply { content });
                            }
                        }
                    }
                    last_len = contents.len();
                }
            }
        }
    });

    let handle = ChatHandle {
        backend: ChatBackend::Daemon {
            helper_spawned: Arc::new(Mutex::new(false)),
        },
    };

    (handle, reply_rx)
}

/// Spawn the chat helper process in the logged-in user's GUI session.
fn spawn_chat_helper() {
    #[cfg(target_os = "macos")]
    {
        // Find the ScreenControl.app bundle binary
        let app_exe =
            "/Library/Application Support/ScreenControl/ScreenControl.app/Contents/MacOS/sc-agent";

        // Get the console (logged-in) user's UID
        let uid = std::process::Command::new("id")
            .args(["-u"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u32>().ok());

        // Try to get the GUI user (may differ from root)
        let gui_uid = std::process::Command::new("stat")
            .args(["-f", "%u", "/dev/console"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u32>().ok());

        let target_uid = gui_uid.or(uid).unwrap_or(501); // fallback to typical first user

        // Use launchctl asuser to run in the user's GUI session
        let result = std::process::Command::new("launchctl")
            .args(["asuser", &target_uid.to_string(), app_exe, "show-chat"])
            .spawn();

        match result {
            Ok(_) => tracing::info!("Spawned chat helper for user {}", target_uid),
            Err(e) => {
                tracing::warn!("Failed to spawn chat helper via launchctl: {}", e);
                // Fallback: try running directly via `open`
                let _ = std::process::Command::new("launchctl")
                    .args([
                        "asuser",
                        &target_uid.to_string(),
                        "open",
                        "/Library/Application Support/ScreenControl/ScreenControl.app",
                        "--args",
                        "show-chat",
                    ])
                    .spawn();
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        tracing::warn!("Daemon-mode chat helper not implemented for this platform");
    }
}

/// Run the chat helper process (spawned by the daemon).
///
/// This watches the IPC inbox file for messages and displays them in a WebView.
/// Replies are written to the outbox file for the daemon to read.
pub fn run_chat_helper() -> ! {
    // Create the tao event loop and WebView
    let event_loop = EventLoopBuilder::<ChatEvent>::with_user_event().build();
    let ready_proxy = event_loop.create_proxy();
    let msg_proxy = event_loop.create_proxy();

    // Background thread: watch inbox file for new messages
    std::thread::spawn(move || {
        let mut last_len: usize = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if let Ok(contents) = std::fs::read_to_string(CHAT_INBOX) {
                if contents.len() > last_len {
                    let new_part = &contents[last_len..];
                    for line in new_part.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        // Parse: {"sender":"...","content":"..."}
                        let sender = extract_json_field(line, "sender")
                            .unwrap_or_else(|| "Support".to_string());
                        let content = extract_json_field(line, "content").unwrap_or_default();
                        let _ = msg_proxy.send_event(ChatEvent::ShowMessage(IncomingMessage {
                            sender,
                            content,
                        }));
                    }
                    last_len = contents.len();
                }
            }
        }
    });

    let mut webview: Option<wry::WebView> = None;
    let mut window: Option<tao::window::Window> = None;
    let mut webview_ready = false;
    let mut pending_messages: Vec<IncomingMessage> = Vec::new();

    event_loop.run(move |event, event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(ChatEvent::ShowMessage(msg)) => {
                // Ensure window + webview exist
                if window.is_none() {
                    let win = WindowBuilder::new()
                        .with_title("ScreenControl — Support Chat")
                        .with_inner_size(tao::dpi::LogicalSize::new(420.0, 520.0))
                        .with_min_inner_size(tao::dpi::LogicalSize::new(340.0, 400.0))
                        .with_always_on_top(false)
                        .build(event_loop)
                        .expect("failed to build chat window");

                    let ready_proxy = ready_proxy.clone();

                    let builder = WebViewBuilder::new()
                        .with_html(CHAT_HTML)
                        .with_ipc_handler(move |ipc_msg| {
                            if let Ok(data) =
                                serde_json::from_str::<serde_json::Value>(ipc_msg.body())
                            {
                                if data.get("type").and_then(|v| v.as_str()) == Some("ready") {
                                    let _ = ready_proxy.send_event(ChatEvent::WebViewReady);
                                    return;
                                }
                                // User reply → write to outbox file
                                if let Some(content) = data.get("content").and_then(|v| v.as_str())
                                {
                                    let escaped = content
                                        .replace('\\', "\\\\")
                                        .replace('"', "\\\"")
                                        .replace('\n', "\\n");
                                    let line = format!("{{\"content\":\"{}\"}}\n", escaped);
                                    if let Ok(mut f) = std::fs::OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open(CHAT_OUTBOX)
                                    {
                                        let _ = f.write_all(line.as_bytes());
                                        let _ = f.flush();
                                    }
                                }
                            }
                        })
                        .with_transparent(false);

                    #[cfg(target_os = "linux")]
                    let wv = {
                        let vbox = win.default_vbox().expect("failed to get GTK vbox");
                        builder.build_gtk(vbox).expect("failed to build webview")
                    };
                    #[cfg(not(target_os = "linux"))]
                    let wv = builder.build(&win).expect("failed to build webview");

                    webview = Some(wv);
                    window = Some(win);
                    webview_ready = false;
                }

                // Bring window to front
                if let Some(ref w) = window {
                    w.set_visible(true);
                    w.set_focus();
                }

                if webview_ready {
                    if let Some(ref wv) = webview {
                        let js = inject_message_js(&msg);
                        let _ = wv.evaluate_script(&js);
                    }
                } else {
                    pending_messages.push(msg);
                }
            }

            Event::UserEvent(ChatEvent::WebViewReady) => {
                webview_ready = true;
                if let Some(ref wv) = webview {
                    for msg in pending_messages.drain(..) {
                        let js = inject_message_js(&msg);
                        let _ = wv.evaluate_script(&js);
                    }
                }
            }

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Clean up IPC files and exit
                let _ = std::fs::remove_file(CHAT_INBOX);
                let _ = std::fs::remove_file(CHAT_OUTBOX);
                std::process::exit(0);
            }

            _ => {}
        }
    })
}

/// Extract a JSON string field value from a simple JSON line.
/// Handles basic escape sequences. Does NOT use a full JSON parser.
fn extract_json_field(json: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", field);
    let start = json.find(&pattern)? + pattern.len();
    let rest = &json[start..];
    // Find the closing unescaped quote
    let mut end = 0;
    let bytes = rest.as_bytes();
    while end < bytes.len() {
        if bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\') {
            break;
        }
        end += 1;
    }
    if end >= bytes.len() {
        return None;
    }
    let raw = &rest[..end];
    Some(
        raw.replace("\\n", "\n")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\"),
    )
}

/// Escape a string for safe embedding in a JS single-quoted string literal.
fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "")
}

/// Build the JS snippet to inject a single message into the WebView.
fn inject_message_js(msg: &IncomingMessage) -> String {
    let sender_js = js_escape(&msg.sender);
    let content_js = js_escape(&msg.content);
    format!(
        "window.addMessage('{}', '{}', false);",
        sender_js, content_js
    )
}

/// Run the tao/wry event loop on the main thread. **This blocks forever.**
pub fn run_event_loop(event_loop: EventLoop<ChatEvent>) -> ! {
    let mut webview: Option<wry::WebView> = None;
    let mut window: Option<tao::window::Window> = None;
    let mut webview_ready = false;
    let mut pending_messages: Vec<IncomingMessage> = Vec::new();

    // Create the proxy for the IPC "ready" signal BEFORE entering the event loop
    // (create_proxy is only available on EventLoop, not EventLoopWindowTarget)
    let ready_proxy = event_loop.create_proxy();

    event_loop.run(move |event, event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(ChatEvent::ShowMessage(msg)) => {
                // Ensure window + webview exist
                if window.is_none() {
                    let win = WindowBuilder::new()
                        .with_title("ScreenControl — Support Chat")
                        .with_inner_size(tao::dpi::LogicalSize::new(420.0, 520.0))
                        .with_min_inner_size(tao::dpi::LogicalSize::new(340.0, 400.0))
                        .with_always_on_top(false)
                        .build(event_loop)
                        .expect("failed to build chat window");

                    // Clone the pre-created proxy so the IPC handler can signal readiness
                    let ready_proxy = ready_proxy.clone();

                    let builder = WebViewBuilder::new()
                        .with_html(CHAT_HTML)
                        .with_ipc_handler(move |ipc_msg| {
                            if let Ok(data) =
                                serde_json::from_str::<serde_json::Value>(ipc_msg.body())
                            {
                                // JS signals readiness: {"type": "ready"}
                                if data.get("type").and_then(|v| v.as_str()) == Some("ready") {
                                    let _ = ready_proxy.send_event(ChatEvent::WebViewReady);
                                    return;
                                }
                                // User reply: {"content": "..."}
                                if let Some(content) = data.get("content").and_then(|v| v.as_str())
                                {
                                    if let Some(tx) = REPLY_TX.lock().unwrap().as_ref() {
                                        let _ = tx.send(OutgoingReply {
                                            content: content.to_string(),
                                        });
                                    }
                                }
                            }
                        })
                        .with_transparent(false);

                    // On Linux, wry needs a GTK window handle (not raw window handle)
                    #[cfg(target_os = "linux")]
                    let wv = {
                        let vbox = win.default_vbox().expect("failed to get GTK vbox");
                        builder.build_gtk(vbox).expect("failed to build webview")
                    };
                    #[cfg(not(target_os = "linux"))]
                    let wv = builder.build(&win).expect("failed to build webview");

                    webview = Some(wv);
                    window = Some(win);
                    webview_ready = false;
                }

                // Bring window to front
                if let Some(ref w) = window {
                    w.set_visible(true);
                    w.set_focus();
                }

                if webview_ready {
                    // WebView is loaded — inject immediately
                    if let Some(ref wv) = webview {
                        let js = inject_message_js(&msg);
                        let _ = wv.evaluate_script(&js);
                    }
                } else {
                    // WebView still loading — queue for later
                    pending_messages.push(msg);
                }
            }

            Event::UserEvent(ChatEvent::WebViewReady) => {
                webview_ready = true;
                // Flush any messages that arrived before the WebView was ready
                if let Some(ref wv) = webview {
                    for msg in pending_messages.drain(..) {
                        let js = inject_message_js(&msg);
                        let _ = wv.evaluate_script(&js);
                    }
                }
            }

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Hide instead of closing
                if let Some(ref w) = window {
                    w.set_visible(false);
                }
            }

            _ => {}
        }
    })
}

// ── Reply channel (shared via mutex for IPC handler) ─────────

static REPLY_TX: Mutex<Option<mpsc::Sender<OutgoingReply>>> = Mutex::new(None);

// ── Embedded HTML/CSS/JS ─────────────────────────────────────

const CHAT_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  @import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap');

  * { margin: 0; padding: 0; box-sizing: border-box; }

  :root {
    --bg: #1a1a1a;
    --surface: #1e1e1e;
    --surface-light: #2a2a2a;
    --surface-dark: #141414;
    --border: #333333;
    --border-light: #3f3f3f;
    --primary: #e05246;
    --primary-dark: #c43d32;
    --text: #e0e0e0;
    --text-dim: #888888;
    --text-muted: #555555;
    --success: #10b981;
    --accent: #22d3ee;
  }

  html, body {
    height: 100%;
    background: var(--bg);
    color: var(--text);
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    font-size: 13px;
    overflow: hidden;
    -webkit-font-smoothing: antialiased;
  }

  body {
    display: flex;
    flex-direction: column;
  }

  /* ── Header ────────────────────────────── */
  .header {
    display: flex;
    align-items: center;
    background: var(--surface);
    height: 44px;
    min-height: 44px;
    border-bottom: 1px solid var(--border);
    padding: 0 12px 0 0;
  }
  .header-accent {
    width: 4px;
    height: 100%;
    background: var(--primary);
    margin-right: 10px;
  }
  .header-title {
    font-size: 14px;
    font-weight: 600;
    flex: 1;
  }
  .header-status {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--success);
    animation: pulse 2s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  /* ── Messages area ─────────────────────── */
  .messages {
    flex: 1;
    overflow-y: auto;
    padding: 14px;
    background: var(--surface-dark);
  }
  .messages::-webkit-scrollbar { width: 5px; }
  .messages::-webkit-scrollbar-track { background: transparent; }
  .messages::-webkit-scrollbar-thumb { background: #4b5563; border-radius: 3px; }

  .msg-group {
    margin-bottom: 14px;
    animation: fadeIn 0.2s ease-out;
  }
  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(4px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .msg-sender {
    font-size: 11px;
    font-weight: 600;
    margin-bottom: 3px;
  }
  .msg-sender.tech { color: var(--primary); }
  .msg-sender.self { color: var(--accent); }
  .msg-content {
    font-size: 13px;
    line-height: 1.45;
    word-wrap: break-word;
  }
  .msg-time {
    font-size: 10px;
    color: var(--text-muted);
    margin-top: 3px;
  }

  /* ── Input area ────────────────────────── */
  .input-area {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    background: var(--surface);
    border-top: 1px solid var(--border);
  }
  .input-field {
    flex: 1;
    background: var(--surface-light);
    border: 1px solid var(--border-light);
    border-radius: 6px;
    color: var(--text);
    font-family: 'Inter', sans-serif;
    font-size: 13px;
    padding: 9px 12px;
    outline: none;
    transition: border-color 0.15s;
  }
  .input-field:focus {
    border-color: var(--primary);
  }
  .input-field::placeholder {
    color: var(--text-muted);
  }
  .send-btn {
    background: var(--primary);
    color: white;
    border: none;
    border-radius: 6px;
    font-family: 'Inter', sans-serif;
    font-size: 12px;
    font-weight: 600;
    padding: 9px 16px;
    cursor: pointer;
    transition: background 0.15s;
    white-space: nowrap;
  }
  .send-btn:hover {
    background: var(--primary-dark);
  }
  .send-btn:active {
    transform: scale(0.97);
  }
</style>
</head>
<body>

<div class="header">
  <div class="header-accent"></div>
  <span class="header-title">Support Chat</span>
  <div class="header-status">
    <div class="status-dot"></div>
    <span>Connected</span>
  </div>
</div>

<div class="messages" id="messages"></div>

<div class="input-area">
  <input class="input-field" id="input" type="text"
         placeholder="Type a message..." autocomplete="off">
  <button class="send-btn" id="sendBtn">Send</button>
</div>

<script>
  const messagesEl = document.getElementById('messages');
  const inputEl = document.getElementById('input');
  const sendBtn = document.getElementById('sendBtn');

  function formatTime() {
    const d = new Date();
    let h = d.getHours();
    const m = String(d.getMinutes()).padStart(2, '0');
    const ampm = h >= 12 ? 'PM' : 'AM';
    h = h % 12 || 12;
    return h + ':' + m + ' ' + ampm;
  }

  window.addMessage = function(sender, content, isSelf) {
    const group = document.createElement('div');
    group.className = 'msg-group';

    const senderEl = document.createElement('div');
    senderEl.className = 'msg-sender ' + (isSelf ? 'self' : 'tech');
    senderEl.textContent = isSelf ? 'You' : sender;

    const contentEl = document.createElement('div');
    contentEl.className = 'msg-content';
    contentEl.textContent = content;

    const timeEl = document.createElement('div');
    timeEl.className = 'msg-time';
    timeEl.textContent = formatTime();

    group.appendChild(senderEl);
    group.appendChild(contentEl);
    group.appendChild(timeEl);
    messagesEl.appendChild(group);
    messagesEl.scrollTop = messagesEl.scrollHeight;
  };

  function sendMessage() {
    const text = inputEl.value.trim();
    if (!text) return;
    inputEl.value = '';
    window.addMessage('You', text, true);
    window.ipc.postMessage(JSON.stringify({ content: text }));
  }

  sendBtn.addEventListener('click', sendMessage);
  inputEl.addEventListener('keydown', function(e) {
    if (e.key === 'Enter') sendMessage();
  });

  inputEl.focus();

  // Signal to Rust that the DOM is ready and addMessage is defined
  window.ipc.postMessage(JSON.stringify({ type: 'ready' }));
</script>

</body>
</html>
"##;
