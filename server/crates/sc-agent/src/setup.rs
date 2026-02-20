//! # macOS TCC Permission Setup Window
//!
//! Opens a native WebView window that guides the user through granting
//! the required macOS privacy permissions:
//!
//! - **Screen Recording** (`CGPreflightScreenCaptureAccess`)
//! - **Accessibility** (`AXIsProcessTrusted`)
//! - **Microphone** (`AVCaptureDevice::authorizationStatusForMediaType:`)
//! - **Full Disk Access** (probe-based detection)
//!
//! The window polls permission status every 2 seconds and updates the
//! UI in real-time. Once all permissions are granted, the user can
//! click "Finish Setup" to verify the daemon is running and close.

use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

// ── Custom events for the setup event loop ────────────────────

#[derive(Debug)]
enum SetupEvent {
    /// JS is ready; do an initial permission check.
    Ready,
    /// Re-check permissions and push the result to the WebView.
    CheckPermissions,
    /// Open the relevant System Settings pane.
    OpenSettings(String),
    /// User clicked "Start Agent" — install LaunchAgent + close.
    StartAgent,
}

// ── macOS FFI declarations ────────────────────────────────────

#[cfg(target_os = "macos")]
#[allow(unused)]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;

    // CoreFoundation
    fn CFStringCreateWithCString(
        allocator: *const std::ffi::c_void,
        c_str: *const i8,
        encoding: u32,
    ) -> *const std::ffi::c_void;
    fn CFDictionaryCreate(
        allocator: *const std::ffi::c_void,
        keys: *const *const std::ffi::c_void,
        values: *const *const std::ffi::c_void,
        num_values: isize,
        key_callbacks: *const std::ffi::c_void,
        value_callbacks: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFRelease(cf: *const std::ffi::c_void);

    static kCFBooleanTrue: *const std::ffi::c_void;
    static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
    static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;

    // Objective-C runtime
    fn objc_getClass(name: *const i8) -> *mut std::ffi::c_void;
    fn sel_registerName(name: *const i8) -> *mut std::ffi::c_void;
    fn dlopen(path: *const i8, mode: i32) -> *mut std::ffi::c_void;
}

// Microphone permission request via Objective-C runtime block FFI.
// We need a real Objective-C block to pass as completionHandler.
#[cfg(target_os = "macos")]
#[repr(C)]
struct CompletionBlock {
    isa: *const std::ffi::c_void,
    flags: i32,
    reserved: i32,
    invoke: extern "C" fn(*mut CompletionBlock, bool),
    descriptor: *const BlockDescr,
}

#[cfg(target_os = "macos")]
#[repr(C)]
struct BlockDescr {
    reserved: usize,
    size: usize,
}

#[cfg(target_os = "macos")]
extern "C" fn noop_block_invoke(_block: *mut CompletionBlock, _granted: bool) {}

#[cfg(target_os = "macos")]
extern "C" {
    static _NSConcreteGlobalBlock: *const std::ffi::c_void;
}

#[cfg(target_os = "macos")]
static NOOP_BLOCK_DESCRIPTOR: BlockDescr = BlockDescr {
    reserved: 0,
    size: std::mem::size_of::<CompletionBlock>(),
};

/// Trigger Screen Recording permission prompt.
/// Adds ScreenControl to the Screen Recording list in System Settings.
#[cfg(target_os = "macos")]
fn prompt_screen_recording() {
    unsafe {
        CGRequestScreenCaptureAccess();
    }
}

/// Trigger Accessibility permission prompt.
/// Adds ScreenControl to the Accessibility list and shows the system prompt.
#[cfg(target_os = "macos")]
fn prompt_accessibility() {
    unsafe {
        let key_str = std::ffi::CString::new("AXTrustedCheckOptionPrompt").unwrap();
        let key = CFStringCreateWithCString(
            std::ptr::null(),
            key_str.as_ptr(),
            0x08000100, // kCFStringEncodingUTF8
        );
        let keys = [key];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks as *const _ as *const std::ffi::c_void,
            &kCFTypeDictionaryValueCallBacks as *const _ as *const std::ffi::c_void,
        );
        AXIsProcessTrustedWithOptions(options);
        CFRelease(key);
        CFRelease(options);
    }
}

/// Trigger Microphone permission prompt.
/// Calls AVCaptureDevice.requestAccessForMediaType(.audio) via ObjC runtime.
#[cfg(target_os = "macos")]
fn prompt_microphone() {
    unsafe {
        let framework_path = std::ffi::CString::new(
            "/System/Library/Frameworks/AVFoundation.framework/AVFoundation",
        )
        .unwrap();
        dlopen(framework_path.as_ptr(), 0x1); // RTLD_LAZY

        let av_class = objc_getClass(std::ffi::CString::new("AVCaptureDevice").unwrap().as_ptr());
        if !av_class.is_null() {
            let sel = sel_registerName(
                std::ffi::CString::new("requestAccessForMediaType:completionHandler:")
                    .unwrap()
                    .as_ptr(),
            );
            let ns_string_class =
                objc_getClass(std::ffi::CString::new("NSString").unwrap().as_ptr());
            let string_sel = sel_registerName(
                std::ffi::CString::new("stringWithUTF8String:")
                    .unwrap()
                    .as_ptr(),
            );
            let audio_cstr = std::ffi::CString::new("soun").unwrap();

            let msg_send: extern "C" fn(
                *mut std::ffi::c_void,
                *mut std::ffi::c_void,
                *const i8,
            ) -> *mut std::ffi::c_void =
                std::mem::transmute(objc_msgSend as *const std::ffi::c_void);
            let media_type = msg_send(ns_string_class, string_sel, audio_cstr.as_ptr());

            let block = Box::leak(Box::new(CompletionBlock {
                isa: _NSConcreteGlobalBlock,
                flags: (1 << 28),
                reserved: 0,
                invoke: noop_block_invoke,
                descriptor: &NOOP_BLOCK_DESCRIPTOR,
            }));

            let request_send: extern "C" fn(
                *mut std::ffi::c_void,
                *mut std::ffi::c_void,
                *mut std::ffi::c_void,
                *const CompletionBlock,
            ) = std::mem::transmute(objc_msgSend as *const std::ffi::c_void);
            request_send(av_class, sel, media_type, block as *const CompletionBlock);
        }
    }
}

#[cfg(target_os = "macos")]
extern "C" {
    fn objc_msgSend();
}

// ── Non-macOS stubs ──────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
fn check_screen_recording() -> bool {
    true
}
#[cfg(not(target_os = "macos"))]
fn check_accessibility() -> bool {
    true
}
#[cfg(not(target_os = "macos"))]
fn check_microphone() -> bool {
    true
}
#[cfg(not(target_os = "macos"))]
fn check_full_disk_access() -> bool {
    true
}

// ── macOS permission checks ──────────────────────────────────
// IMPORTANT: We query the system TCC database for the daemon's bundle ID
// (`com.screencontrol.agent`) instead of using process-local APIs like
// `CGPreflightScreenCaptureAccess()`, which only check the setup app's
// own permissions and would incorrectly report "granted".

/// The daemon's bundle ID for TCC lookups (client_type=0).
#[allow(dead_code)]
const DAEMON_BUNDLE_ID: &str = "com.screencontrol.agent";

/// The installed binary path for TCC lookups (client_type=1).
#[allow(dead_code)]
const DAEMON_BINARY_PATH: &str = "/Library/Application Support/ScreenControl/sc-agent";

/// Query the system TCC database for a given service.
/// Checks both the bundle ID (client_type=0) and the binary path (client_type=1),
/// since macOS may register either depending on how the permission was granted.
/// Returns true if auth_value == 2 (allowed).
#[cfg(target_os = "macos")]
fn check_tcc_for_daemon(service: &str) -> bool {
    use std::process::Command;
    // The system TCC database is at /Library/Application Support/com.apple.TCC/TCC.db
    // We need root to read it, which we have since setup runs via `sudo sc-agent install`.
    //
    // Query both:
    //   - client_type=0 (bundle ID): used when permission is granted to an .app bundle
    //   - client_type=1 (binary path): used when permission is granted to a command-line binary
    let output = Command::new("sqlite3")
        .args([
            "/Library/Application Support/com.apple.TCC/TCC.db",
            &format!(
                "SELECT auth_value FROM access WHERE service='{}' AND ((client='{}' AND client_type=0) OR (client='{}' AND client_type=1)) AND auth_value=2 LIMIT 1;",
                service, DAEMON_BUNDLE_ID, DAEMON_BINARY_PATH
            ),
        ])
        .output();
    match output {
        Ok(out) => {
            let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
            result == "2"
        }
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn check_screen_recording() -> bool {
    // Primary: process-local check via CGPreflightScreenCaptureAccess.
    // Since setup runs from the .app bundle (via `open ScreenControl.app`),
    // this checks the bundle's own permissions — which IS the daemon identity.
    let granted = unsafe { CGPreflightScreenCaptureAccess() };
    if granted {
        return true;
    }
    // Fallback: TCC database query (works when running as root)
    check_tcc_for_daemon("kTCCServiceScreenCapture")
}

#[cfg(target_os = "macos")]
fn check_accessibility() -> bool {
    // Primary: process-local check via AXIsProcessTrusted.
    // Works correctly from the .app bundle context.
    let granted = unsafe { AXIsProcessTrusted() };
    if granted {
        return true;
    }
    // Fallback: TCC database query (works when running as root)
    check_tcc_for_daemon("kTCCServiceAccessibility")
}

/// Check microphone permission for the daemon.
#[cfg(target_os = "macos")]
fn check_microphone() -> bool {
    // Primary: check AVFoundation authorization status via osascript.
    // Returns "authorized" if microphone access is granted.
    let output = std::process::Command::new("osascript")
        .args([
            "-e", "use framework \"AVFoundation\"",
            "-e", "set authStatus to current application's AVCaptureDevice's authorizationStatusForMediaType:(current application's AVMediaTypeAudio)",
            "-e", "if authStatus is 3 then return \"authorized\"",
            "-e", "return \"denied\"",
        ])
        .output();
    if let Ok(out) = output {
        let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if result == "authorized" {
            return true;
        }
    }
    // Fallback: TCC database query
    check_tcc_for_daemon("kTCCServiceMicrophone")
}

/// Check Full Disk Access for the daemon.
#[cfg(target_os = "macos")]
fn check_full_disk_access() -> bool {
    // Primary: try to read a TCC-protected path.
    // Non-root processes can only read this if FDA is granted.
    let can_read = std::fs::read_dir("/Library/Application Support/com.apple.TCC").is_ok();
    if can_read {
        return true;
    }
    // Fallback: TCC database query
    check_tcc_for_daemon("kTCCServiceSystemPolicyAllFiles")
}

// ── Verify daemon is installed & running ───────────────────────

fn verify_agent_running() -> Result<(), String> {
    // Check if the LaunchAgent plist exists
    let plist_path = "/Library/LaunchAgents/com.screencontrol.agent.plist";
    if !std::path::Path::new(plist_path).exists() {
        return Err("Agent not installed. Run 'sudo sc-agent install' first.".into());
    }

    // Check if the agent is loaded
    let output = std::process::Command::new("launchctl")
        .args(["list"])
        .output()
        .map_err(|e| format!("Failed to run launchctl: {}", e))?;

    let list_output = String::from_utf8_lossy(&output.stdout);
    if list_output.contains("com.screencontrol.agent") {
        Ok(())
    } else {
        // Try to load it
        let status = std::process::Command::new("launchctl")
            .args(["load", "-w", plist_path])
            .status()
            .map_err(|e| format!("Failed to load agent: {}", e))?;

        if status.success() {
            Ok(())
        } else {
            Err("Agent is installed but not running. Try: launchctl load -w /Library/LaunchAgents/com.screencontrol.agent.plist".into())
        }
    }
}

// ── Public entry point ────────────────────────────────────────

/// Run the TCC onboarding setup window. Blocks the current thread.
pub fn run_setup() -> ! {
    let event_loop = EventLoopBuilder::<SetupEvent>::with_user_event().build();
    let ready_proxy = event_loop.create_proxy();
    let check_proxy = event_loop.create_proxy();
    let settings_proxy = event_loop.create_proxy();
    let start_proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("ScreenControl — Setup")
        .with_inner_size(tao::dpi::LogicalSize::new(520.0, 780.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(480.0, 740.0))
        .with_resizable(false)
        .build(&event_loop)
        .expect("failed to build setup window");

    #[cfg(not(target_os = "linux"))]
    let webview = WebViewBuilder::new()
        .with_html(SETUP_HTML)
        .with_ipc_handler(move |ipc_msg| {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(ipc_msg.body()) {
                match data.get("type").and_then(|v| v.as_str()) {
                    Some("ready") => {
                        let _ = ready_proxy.send_event(SetupEvent::Ready);
                    }
                    Some("check_permissions") => {
                        let _ = check_proxy.send_event(SetupEvent::CheckPermissions);
                    }
                    Some("open_settings") => {
                        if let Some(cat) = data.get("category").and_then(|v| v.as_str()) {
                            let _ = settings_proxy
                                .send_event(SetupEvent::OpenSettings(cat.to_string()));
                        }
                    }
                    Some("start_agent") => {
                        let _ = start_proxy.send_event(SetupEvent::StartAgent);
                    }
                    _ => {}
                }
            }
        })
        .with_transparent(false)
        .build(&window)
        .expect("failed to build setup webview");

    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().expect("failed to get GTK vbox");
        WebViewBuilder::new()
            .with_html(SETUP_HTML)
            .with_ipc_handler(move |ipc_msg| {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(ipc_msg.body()) {
                    match data.get("type").and_then(|v| v.as_str()) {
                        Some("ready") => {
                            let _ = ready_proxy.send_event(SetupEvent::Ready);
                        }
                        Some("check_permissions") => {
                            let _ = check_proxy.send_event(SetupEvent::CheckPermissions);
                        }
                        Some("open_settings") => {
                            if let Some(cat) = data.get("category").and_then(|v| v.as_str()) {
                                let _ = settings_proxy
                                    .send_event(SetupEvent::OpenSettings(cat.to_string()));
                            }
                        }
                        Some("start_agent") => {
                            let _ = start_proxy.send_event(SetupEvent::StartAgent);
                        }
                        _ => {}
                    }
                }
            })
            .with_transparent(false)
            .build_gtk(vbox)
            .expect("failed to build setup webview")
    };

    event_loop.run(move |event, _event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(SetupEvent::Ready) => {
                // Just check current permission status — no prompts.
                // Prompts are triggered when the user clicks each button.
                let screen_ok = check_screen_recording();
                let accessibility_ok = check_accessibility();
                let mic_ok = check_microphone();
                let fda_ok = check_full_disk_access();
                let js = format!(
                    "updatePermissions({}, {}, {}, {})",
                    screen_ok, accessibility_ok, mic_ok, fda_ok
                );
                let _ = webview.evaluate_script(&js);
            }

            Event::UserEvent(SetupEvent::CheckPermissions) => {
                let screen_ok = check_screen_recording();
                let accessibility_ok = check_accessibility();
                let mic_ok = check_microphone();
                let fda_ok = check_full_disk_access();
                let js = format!(
                    "updatePermissions({}, {}, {}, {})",
                    screen_ok, accessibility_ok, mic_ok, fda_ok
                );
                let _ = webview.evaluate_script(&js);
            }

            Event::UserEvent(SetupEvent::OpenSettings(category)) => {
                #[cfg(target_os = "macos")]
                {
                    // Open the appropriate System Settings pane.
                    // The install already pre-granted TCC entries via sqlite3 INSERT.
                    // We just need the user to toggle/confirm in System Settings.
                    match category.as_str() {
                        "screen_recording" => {
                            // Trigger native Screen Recording prompt — adds ScreenControl to the list
                            prompt_screen_recording();
                            let _ = std::process::Command::new("open")
                                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                                .spawn();
                        }
                        "accessibility" => {
                            // Trigger native Accessibility prompt — adds ScreenControl to the list
                            prompt_accessibility();
                            let _ = std::process::Command::new("open")
                                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                                .spawn();
                        }
                        "microphone" => {
                            // Trigger native Microphone prompt — shows the system dialog
                            prompt_microphone();
                        }
                        "full_disk_access" => {
                            // No native API — just open System Settings
                            let _ = std::process::Command::new("open")
                                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
                                .spawn();
                        }
                        _ => {}
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = category;
                    tracing::info!("Open settings not supported on this platform");
                }
            }

            Event::UserEvent(SetupEvent::StartAgent) => {
                // Restart the agent so it picks up newly granted TCC permissions.
                // macOS (especially Sequoia) requires app restart for Screen Recording.
                #[cfg(target_os = "macos")]
                {
                    // Get console user UID for gui/ domain
                    let uid = std::process::Command::new("stat")
                        .args(["-f", "%u", "/dev/console"])
                        .output()
                        .ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .map(|s| s.trim().to_string())
                        .unwrap_or_else(|| "501".to_string());
                    let target = format!("gui/{}/com.screencontrol.agent", uid);
                    let _ = std::process::Command::new("launchctl")
                        .args(["kickstart", "-kp", &target])
                        .output();
                }

                // Give the daemon a moment to restart before checking
                std::thread::sleep(std::time::Duration::from_millis(1500));

                let result = verify_agent_running();
                let js = match &result {
                    Ok(()) => "agentStarted(true, '')".to_string(),
                    Err(e) => format!("agentStarted(false, '{}')", e.replace('\'', "\\'")),
                };
                let _ = webview.evaluate_script(&js);

                if result.is_ok() {
                    // Give a moment for the success animation, then exit
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        std::process::exit(0);
                    });
                }
            }

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }

            _ => {}
        }
    })
}

// ── Embedded HTML/CSS/JS for the setup window ─────────────────

const SETUP_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
  @import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap');

  * { margin: 0; padding: 0; box-sizing: border-box; }

  :root {
    --bg: #121212;
    --surface: #1a1a1a;
    --surface-light: #242424;
    --surface-lighter: #2e2e2e;
    --border: #333;
    --primary: #e05246;
    --primary-dark: #c43d32;
    --primary-glow: rgba(224, 82, 70, 0.15);
    --success: #10b981;
    --success-glow: rgba(16, 185, 129, 0.15);
    --text: #e8e8e8;
    --text-dim: #999;
    --text-muted: #666;
    --warning: #f59e0b;
  }

  html, body {
    height: 100%;
    background: var(--bg);
    color: var(--text);
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    font-size: 14px;
    overflow: hidden;
    -webkit-font-smoothing: antialiased;
    user-select: none;
    -webkit-user-select: none;
  }

  body {
    display: flex;
    flex-direction: column;
  }

  /* ── Header ────────────────────────────── */
  .header {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    padding: 32px 24px 24px;
    background: linear-gradient(180deg, var(--surface-light) 0%, var(--bg) 100%);
  }

  .logo {
    width: 64px;
    height: 64px;
    border-radius: 16px;
    background: linear-gradient(135deg, #2a2a2a 0%, #1a1a1a 100%);
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 16px;
    box-shadow: 0 4px 16px rgba(0,0,0,0.3);
    border: 1px solid var(--border);
  }

  .logo svg {
    width: 36px;
    height: 36px;
    color: var(--primary);
  }

  .header h1 {
    font-size: 20px;
    font-weight: 700;
    margin-bottom: 6px;
    letter-spacing: -0.3px;
  }

  .header p {
    font-size: 13px;
    color: var(--text-dim);
    text-align: center;
    line-height: 1.5;
    max-width: 340px;
  }

  /* ── Permission cards ──────────────────── */
  .permissions {
    flex: 1;
    padding: 0 24px;
    overflow-y: auto;
  }

  .perm-card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 18px 20px;
    margin-bottom: 12px;
    display: flex;
    align-items: center;
    gap: 16px;
    transition: all 0.3s ease;
  }

  .perm-card.granted {
    border-color: var(--success);
    background: var(--success-glow);
  }

  .perm-icon {
    width: 44px;
    height: 44px;
    min-width: 44px;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 22px;
    background: var(--surface-light);
    transition: all 0.3s ease;
  }

  .perm-card.granted .perm-icon {
    background: var(--success);
    color: white;
  }

  .perm-info {
    flex: 1;
    min-width: 0;
  }

  .perm-title {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 3px;
  }

  .perm-desc {
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.4;
  }

  .perm-action {
    min-width: 80px;
    text-align: right;
  }

  .perm-action .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-weight: 600;
    padding: 5px 10px;
    border-radius: 6px;
  }

  .status-badge.pending {
    background: rgba(245, 158, 11, 0.15);
    color: var(--warning);
  }

  .status-badge.done {
    background: var(--success-glow);
    color: var(--success);
  }

  .open-btn {
    display: inline-block;
    background: var(--surface-lighter);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 6px;
    font-family: 'Inter', sans-serif;
    font-size: 12px;
    font-weight: 500;
    padding: 6px 12px;
    cursor: pointer;
    transition: all 0.15s;
    margin-top: 6px;
  }

  .open-btn:hover {
    background: var(--surface-light);
    border-color: var(--primary);
    color: var(--primary);
  }

  /* ── Footer ────────────────────────────── */
  .footer {
    padding: 20px 24px 28px;
  }

  .start-btn {
    width: 100%;
    background: var(--primary);
    color: white;
    border: none;
    border-radius: 10px;
    font-family: 'Inter', sans-serif;
    font-size: 15px;
    font-weight: 600;
    padding: 14px;
    cursor: pointer;
    transition: all 0.2s;
    letter-spacing: -0.2px;
  }

  .start-btn:hover:not(:disabled) {
    background: var(--primary-dark);
    transform: translateY(-1px);
    box-shadow: 0 4px 16px rgba(224, 82, 70, 0.3);
  }

  .start-btn:active:not(:disabled) {
    transform: scale(0.98);
  }

  .start-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .start-btn.success {
    background: var(--success);
  }

  .footer-note {
    text-align: center;
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 10px;
    line-height: 1.4;
  }

  .error-msg {
    text-align: center;
    font-size: 12px;
    color: #ef4444;
    margin-top: 8px;
    display: none;
  }

  /* ── Animations ────────────────────────── */
  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .perm-card { animation: fadeIn 0.4s ease-out; }
  .perm-card:nth-child(2) { animation-delay: 0.1s; }
  .perm-card:nth-child(3) { animation-delay: 0.2s; }
  .perm-card:nth-child(4) { animation-delay: 0.3s; }

  @keyframes checkPop {
    0% { transform: scale(0); }
    50% { transform: scale(1.2); }
    100% { transform: scale(1); }
  }
  .check-anim { animation: checkPop 0.3s ease-out; }
</style>
</head>
<body>

<div class="header">
  <div class="logo">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="2" y="3" width="20" height="14" rx="2" ry="2"/>
      <line x1="8" y1="21" x2="16" y2="21"/>
      <line x1="12" y1="17" x2="12" y2="21"/>
    </svg>
  </div>
  <h1>ScreenControl Setup</h1>
  <p>Grant the required permissions for remote desktop access. The agent needs these to capture your screen and control input.</p>
</div>

<div class="permissions">
  <div class="perm-card" id="perm-screen">
    <div class="perm-icon">🖥️</div>
    <div class="perm-info">
      <div class="perm-title">Screen Recording</div>
      <div class="perm-desc">Allows the agent to capture and stream your screen during remote sessions.</div>
      <button class="open-btn" onclick="openSettings('screen_recording')">Open System Settings</button>
    </div>
    <div class="perm-action">
      <div class="status-badge pending" id="screen-status">⏳ Needed</div>
    </div>
  </div>

  <div class="perm-card" id="perm-accessibility">
    <div class="perm-icon">🖱️</div>
    <div class="perm-info">
      <div class="perm-title">Accessibility</div>
      <div class="perm-desc">Allows the agent to control mouse and keyboard for remote input injection.</div>
      <button class="open-btn" onclick="openSettings('accessibility')">Open System Settings</button>
    </div>
    <div class="perm-action">
      <div class="status-badge pending" id="accessibility-status">⏳ Needed</div>
    </div>
  </div>

  <div class="perm-card" id="perm-microphone">
    <div class="perm-icon">🎤</div>
    <div class="perm-info">
      <div class="perm-title">Microphone</div>
      <div class="perm-desc">Allows the agent to capture and stream audio during remote sessions.</div>
      <button class="open-btn" onclick="openSettings('microphone')">Request Microphone Access</button>
    </div>
    <div class="perm-action">
      <div class="status-badge pending" id="microphone-status">⏳ Needed</div>
    </div>
  </div>

  <div class="perm-card" id="perm-fda">
    <div class="perm-icon">📁</div>
    <div class="perm-info">
      <div class="perm-title">Full Disk Access</div>
      <div class="perm-desc">Allows the agent to access files for remote file transfer and management.</div>
      <button class="open-btn" onclick="openSettings('full_disk_access')">Open System Settings</button>
    </div>
    <div class="perm-action">
      <div class="status-badge pending" id="fda-status">⏳ Needed</div>
    </div>
  </div>
</div>

<div class="footer">
  <button class="start-btn" id="startBtn" disabled onclick="startAgent()">
    Grant All Permissions to Continue
  </button>
  <div class="error-msg" id="errorMsg"></div>
  <div class="footer-note">
    The agent runs as a background system service (LaunchDaemon).
  </div>
</div>

<script>
  let screenOk = false;
  let accessibilityOk = false;
  let microphoneOk = false;
  let fdaOk = false;
  let started = false;

  // Poll permissions every 2 seconds
  setInterval(() => {
    if (!started) {
      window.ipc.postMessage(JSON.stringify({ type: 'check_permissions' }));
    }
  }, 2000);

  function openSettings(category) {
    window.ipc.postMessage(JSON.stringify({ type: 'open_settings', category }));
  }

  // Called from Rust with current permission state
  window.updatePermissions = function(screen, accessibility, microphone, fda) {
    const screenChanged = screen !== screenOk;
    const accessibilityChanged = accessibility !== accessibilityOk;
    const microphoneChanged = microphone !== microphoneOk;
    const fdaChanged = fda !== fdaOk;

    screenOk = screen;
    accessibilityOk = accessibility;
    microphoneOk = microphone;
    fdaOk = fda;

    // Update Screen Recording card
    const screenCard = document.getElementById('perm-screen');
    const screenStatus = document.getElementById('screen-status');
    if (screen) {
      screenCard.classList.add('granted');
      if (screenChanged) {
        screenStatus.innerHTML = '<span class="check-anim">✅</span> Granted';
      } else {
        screenStatus.innerHTML = '✅ Granted';
      }
      screenStatus.className = 'status-badge done';
      const btn = screenCard.querySelector('.open-btn');
      if (btn) btn.style.display = 'none';
    }

    // Update Accessibility card
    const accessCard = document.getElementById('perm-accessibility');
    const accessStatus = document.getElementById('accessibility-status');
    if (accessibility) {
      accessCard.classList.add('granted');
      if (accessibilityChanged) {
        accessStatus.innerHTML = '<span class="check-anim">✅</span> Granted';
      } else {
        accessStatus.innerHTML = '✅ Granted';
      }
      accessStatus.className = 'status-badge done';
      const btn = accessCard.querySelector('.open-btn');
      if (btn) btn.style.display = 'none';
    }

    // Update Microphone card
    const micCard = document.getElementById('perm-microphone');
    const micStatus = document.getElementById('microphone-status');
    if (microphone) {
      micCard.classList.add('granted');
      if (microphoneChanged) {
        micStatus.innerHTML = '<span class="check-anim">✅</span> Granted';
      } else {
        micStatus.innerHTML = '✅ Granted';
      }
      micStatus.className = 'status-badge done';
      const btn = micCard.querySelector('.open-btn');
      if (btn) btn.style.display = 'none';
    }

    // Update Full Disk Access card
    const fdaCard = document.getElementById('perm-fda');
    const fdaStatus = document.getElementById('fda-status');
    if (fda) {
      fdaCard.classList.add('granted');
      if (fdaChanged) {
        fdaStatus.innerHTML = '<span class="check-anim">✅</span> Granted';
      } else {
        fdaStatus.innerHTML = '✅ Granted';
      }
      fdaStatus.className = 'status-badge done';
      const btn = fdaCard.querySelector('.open-btn');
      if (btn) btn.style.display = 'none';
    }

    // Update start button
    const startBtn = document.getElementById('startBtn');
    if (screen && accessibility && microphone && fda) {
      startBtn.disabled = false;
      startBtn.textContent = 'Finish Setup';
    } else {
      startBtn.disabled = true;
      const remaining = [];
      if (!screen) remaining.push('Screen Recording');
      if (!accessibility) remaining.push('Accessibility');
      if (!microphone) remaining.push('Microphone');
      if (!fda) remaining.push('Full Disk Access');
      startBtn.textContent = 'Grant ' + remaining.join(', ') + ' to Continue';
    }
  };

  function startAgent() {
    if (started) return;
    started = true;
    const btn = document.getElementById('startBtn');
    btn.textContent = 'Starting...';
    btn.disabled = true;
    window.ipc.postMessage(JSON.stringify({ type: 'start_agent' }));
  }

  // Called from Rust after agent start attempt
  window.agentStarted = function(success, error) {
    const btn = document.getElementById('startBtn');
    const errorMsg = document.getElementById('errorMsg');

    if (success) {
      btn.textContent = '✅ Setup Complete — Agent Running!';
      btn.classList.add('success');
      errorMsg.style.display = 'none';
    } else {
      btn.textContent = 'Start Agent';
      btn.disabled = false;
      started = false;
      errorMsg.textContent = 'Error: ' + error;
      errorMsg.style.display = 'block';
    }
  };

  // Signal ready to Rust
  window.ipc.postMessage(JSON.stringify({ type: 'ready' }));
</script>

</body>
</html>
"##;
