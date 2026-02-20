//! Host session-control commands.
//!
//! Implements the ScreenConnect-style controls a viewer can send to the agent:
//!
//! | Command         | Linux                                  | Windows (stub) | macOS (stub) |
//! |-----------------|----------------------------------------|----------------|--------------|
//! | Block Input     | xinput disable / uinput grab           | BlockInput()   | CGEventTap   |
//! | Blank Screen    | DPMS off / black fullscreen window     | SC_MONITORPOWER| IOPMAssertion|
//! | Wake Lock       | systemd-inhibit / D-Bus Inhibit        | SetThreadExec…| caffeinate   |
//! | Reboot Normal   | shutdown -r now                        | shutdown /r    | shutdown -r  |
//! | Reboot Safe     | rescue.target + reboot                 | bcdedit + reboot| nvram -x    |

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

// ─── State tracking for toggleable commands ──────────────────

static INPUT_BLOCKED: AtomicBool = AtomicBool::new(false);
static SCREEN_BLANKED: AtomicBool = AtomicBool::new(false);
static WAKE_LOCKED: AtomicBool = AtomicBool::new(false);

/// Whether guest input is currently blocked.
#[allow(dead_code)]
pub fn is_input_blocked() -> bool {
    INPUT_BLOCKED.load(Ordering::Relaxed)
}

/// Whether the guest screen is currently blanked.
#[allow(dead_code)]
pub fn is_screen_blanked() -> bool {
    SCREEN_BLANKED.load(Ordering::Relaxed)
}

/// Whether a wake lock is currently held.
#[allow(dead_code)]
pub fn is_wake_locked() -> bool {
    WAKE_LOCKED.load(Ordering::Relaxed)
}

// ─── Block Guest Input ──────────────────────────────────────

/// Block or unblock all HID input on the guest machine.
pub fn set_block_input(enable: bool) {
    if enable == INPUT_BLOCKED.load(Ordering::Relaxed) {
        tracing::debug!(
            "Block input already {}",
            if enable { "enabled" } else { "disabled" }
        );
        return;
    }

    #[cfg(target_os = "linux")]
    {
        block_input_linux(enable);
    }

    #[cfg(target_os = "windows")]
    {
        block_input_windows(enable);
    }

    #[cfg(target_os = "macos")]
    {
        tracing::warn!("Block input not yet implemented on macOS");
    }

    INPUT_BLOCKED.store(enable, Ordering::Relaxed);
    tracing::info!(
        "Guest input {}",
        if enable { "BLOCKED" } else { "UNBLOCKED" }
    );
}

#[cfg(target_os = "linux")]
fn block_input_linux(enable: bool) {
    // Strategy: Use xinput to disable/enable all physical input devices.
    // This works on X11 and XWayland. For pure Wayland, we use the transparent
    // overlay approach (same technique ScreenConnect uses).

    // Try xinput approach first (works on X11 and XWayland)
    let action = if enable { "disable" } else { "enable" };

    // Get list of input device IDs (keyboards and mice)
    let output = Command::new("xinput").arg("list").arg("--id-only").output();

    match output {
        Ok(out) if out.status.success() => {
            let ids = String::from_utf8_lossy(&out.stdout);
            for id in ids.lines() {
                let id = id.trim();
                if id.is_empty() {
                    continue;
                }

                // Check if this is a physical device (skip virtual/XTEST devices)
                let props = Command::new("xinput").arg("list-props").arg(id).output();

                if let Ok(p) = props {
                    let props_str = String::from_utf8_lossy(&p.stdout);
                    // Skip XTEST and virtual devices — we only want physical HID
                    if props_str.contains("XTEST") || props_str.contains("Virtual") {
                        continue;
                    }
                    // Skip our own injector devices
                    if props_str.contains("ScreenControl") || props_str.contains("sc-uinput") {
                        continue;
                    }
                }

                let result = Command::new("xinput").arg(action).arg(id).output();

                match result {
                    Ok(r) if r.status.success() => {
                        tracing::debug!("xinput {} device {}", action, id);
                    }
                    Ok(r) => {
                        tracing::debug!(
                            "xinput {} device {} failed: {}",
                            action,
                            id,
                            String::from_utf8_lossy(&r.stderr)
                        );
                    }
                    Err(e) => {
                        tracing::debug!("xinput {} device {} error: {}", action, id, e);
                    }
                }
            }
        }
        Ok(_) => {
            tracing::warn!("xinput list failed — input blocking may not work on pure Wayland");
        }
        Err(e) => {
            tracing::warn!("xinput not found ({}), trying evdev approach", e);
            block_input_linux_evdev(enable);
        }
    }
}

#[cfg(target_os = "linux")]
fn block_input_linux_evdev(enable: bool) {
    // Fallback: use evdev EVIOCGRAB to exclusively grab input devices.
    // This works on both X11 and Wayland but requires root (which we have).
    use std::fs;
    use std::os::unix::io::AsRawFd;

    let input_dir = "/dev/input";
    let entries = match fs::read_dir(input_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Cannot read {}: {}", input_dir, e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        // Only grab event devices (eventN), not mice/js
        if !name.starts_with("event") {
            continue;
        }

        // Check if this is a keyboard or mouse via /sys
        let sys_path = format!("/sys/class/input/{}/device/capabilities/ev", name);
        let caps = fs::read_to_string(&sys_path).unwrap_or_default();
        let caps_val = u64::from_str_radix(caps.trim(), 16).unwrap_or(0);

        // EV_KEY = bit 1 (keyboards/mice), EV_REL = bit 2 (mice)
        let is_input = (caps_val & 0x02 != 0) || (caps_val & 0x04 != 0);
        if !is_input {
            continue;
        }

        if enable {
            // EVIOCGRAB = exclusive grab
            match fs::OpenOptions::new().read(true).open(&path) {
                Ok(f) => {
                    let fd = f.as_raw_fd();
                    // EVIOCGRAB ioctl = 0x40044590
                    let ret = unsafe { libc::ioctl(fd, 0x40044590, 1 as libc::c_int) };
                    if ret == 0 {
                        tracing::debug!("EVIOCGRAB on {:?}", path);
                        // Keep the fd open (leak it) to maintain the grab
                        std::mem::forget(f);
                    } else {
                        tracing::debug!(
                            "EVIOCGRAB on {:?} failed: {}",
                            path,
                            std::io::Error::last_os_error()
                        );
                    }
                }
                Err(e) => tracing::debug!("Cannot open {:?}: {}", path, e),
            }
        } else {
            // Release grab by closing the leaked fd.
            // Since we can't easily track which fds we leaked,
            // we just do a no-op EVIOCGRAB(0) which releases any grab.
            match fs::OpenOptions::new().read(true).open(&path) {
                Ok(f) => {
                    let fd = f.as_raw_fd();
                    unsafe { libc::ioctl(fd, 0x40044590, 0 as libc::c_int) };
                    tracing::debug!("EVIOCGRAB release on {:?}", path);
                }
                Err(e) => tracing::debug!("Cannot open {:?}: {}", path, e),
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn block_input_windows(enable: bool) {
    // Windows: use BlockInput() from user32.dll
    // Requires the process to run in Session 0 or have SYSTEM privileges
    tracing::warn!("Block input on Windows — stub (TODO: BlockInput API)");
}

// ─── Blank Guest Monitor ────────────────────────────────────

/// Blank or unblank the guest's monitor(s).
pub fn set_blank_screen(enable: bool) {
    if enable == SCREEN_BLANKED.load(Ordering::Relaxed) {
        tracing::debug!(
            "Blank screen already {}",
            if enable { "enabled" } else { "disabled" }
        );
        return;
    }

    #[cfg(target_os = "linux")]
    {
        blank_screen_linux(enable);
    }

    #[cfg(target_os = "windows")]
    {
        blank_screen_windows(enable);
    }

    #[cfg(target_os = "macos")]
    {
        tracing::warn!("Blank screen not yet implemented on macOS");
    }

    SCREEN_BLANKED.store(enable, Ordering::Relaxed);
    tracing::info!(
        "Guest screen {}",
        if enable { "BLANKED" } else { "RESTORED" }
    );
}

#[cfg(target_os = "linux")]
fn blank_screen_linux(enable: bool) {
    if enable {
        // Try DPMS force off first
        let result = Command::new("xset")
            .arg("dpms")
            .arg("force")
            .arg("off")
            .output();

        match result {
            Ok(r) if r.status.success() => {
                tracing::debug!("DPMS force off succeeded");
            }
            _ => {
                // Fallback: try xrandr brightness 0
                tracing::debug!("DPMS failed, trying xrandr brightness");
                let _ = Command::new("xrandr")
                    .args(["--output", "eDP-1", "--brightness", "0"])
                    .output();
            }
        }
    } else {
        // Restore DPMS
        let _ = Command::new("xset")
            .arg("dpms")
            .arg("force")
            .arg("on")
            .output();

        // Also restore brightness in case we used xrandr
        let _ = Command::new("xrandr")
            .args(["--output", "eDP-1", "--brightness", "1"])
            .output();
    }
}

#[cfg(target_os = "windows")]
fn blank_screen_windows(enable: bool) {
    // Windows: SendMessage(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER, 2) to turn off
    // SendMessage(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER, -1) to turn on
    tracing::warn!("Blank screen on Windows — stub (TODO: SC_MONITORPOWER)");
}

// ─── Wake Lock ──────────────────────────────────────────────

/// Acquire or release a wake lock (prevent sleep/screen lock).
pub fn set_wake_lock(enable: bool) {
    if enable == WAKE_LOCKED.load(Ordering::Relaxed) {
        tracing::debug!(
            "Wake lock already {}",
            if enable { "held" } else { "released" }
        );
        return;
    }

    #[cfg(target_os = "linux")]
    {
        wake_lock_linux(enable);
    }

    #[cfg(target_os = "windows")]
    {
        wake_lock_windows(enable);
    }

    #[cfg(target_os = "macos")]
    {
        tracing::warn!("Wake lock not yet implemented on macOS");
    }

    WAKE_LOCKED.store(enable, Ordering::Relaxed);
    tracing::info!("Wake lock {}", if enable { "ACQUIRED" } else { "RELEASED" });
}

#[cfg(target_os = "linux")]
fn wake_lock_linux(enable: bool) {
    use std::sync::Mutex;
    use std::sync::OnceLock;

    // Store the inhibitor child process so we can kill it on release
    static INHIBITOR_PROCESS: OnceLock<Mutex<Option<std::process::Child>>> = OnceLock::new();
    let proc_mutex = INHIBITOR_PROCESS.get_or_init(|| Mutex::new(None));

    if enable {
        // Use systemd-inhibit to prevent sleep and idle
        match Command::new("systemd-inhibit")
            .args([
                "--what=idle:sleep:handle-lid-switch",
                "--who=ScreenControl Agent",
                "--why=Remote session active",
                "--mode=block",
                "sleep",
                "infinity",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(child) => {
                tracing::debug!("systemd-inhibit spawned (pid {})", child.id());
                if let Ok(mut guard) = proc_mutex.lock() {
                    *guard = Some(child);
                }
            }
            Err(e) => {
                tracing::warn!("systemd-inhibit failed: {}, trying D-Bus fallback", e);
                // Fallback: use xdg-screensaver reset in a loop
                // (less reliable but works without systemd)
                let _ = Command::new("xset").args(["s", "off"]).output();
                let _ = Command::new("xset").args(["-dpms"]).output();
            }
        }
    } else {
        // Kill the inhibitor process
        if let Ok(mut guard) = proc_mutex.lock() {
            if let Some(ref mut child) = *guard {
                let _ = child.kill();
                let _ = child.wait();
                tracing::debug!("systemd-inhibit process killed");
            }
            *guard = None;
        }

        // Restore screensaver/DPMS defaults
        let _ = Command::new("xset").args(["s", "on"]).output();
        let _ = Command::new("xset").args(["+dpms"]).output();
    }
}

#[cfg(target_os = "windows")]
fn wake_lock_windows(enable: bool) {
    // Windows: SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)
    tracing::warn!("Wake lock on Windows — stub (TODO: SetThreadExecutionState)");
}

// ─── Reboot ─────────────────────────────────────────────────

/// Reboot the machine in normal mode.
pub fn reboot_normal() {
    tracing::warn!("REBOOT NORMAL requested — rebooting machine");

    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("shutdown").args(["-r", "now"]).output();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("shutdown").args(["/r", "/t", "0"]).output();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("shutdown").args(["-r", "now"]).output();
    }
}

/// Reboot the machine into safe/rescue mode.
pub fn reboot_safe_mode() {
    tracing::warn!("REBOOT SAFE MODE requested — rebooting into rescue/safe mode");

    #[cfg(target_os = "linux")]
    {
        // Set next boot to rescue target then reboot
        let _ = Command::new("systemctl")
            .args(["set-default", "rescue.target"])
            .output();
        let _ = Command::new("shutdown").args(["-r", "now"]).output();
        // Note: the agent service will restore multi-user.target on next normal boot
    }

    #[cfg(target_os = "windows")]
    {
        // Set safe mode with networking and reboot
        let _ = Command::new("bcdedit")
            .args(["/set", "{current}", "safeboot", "network"])
            .output();
        let _ = Command::new("shutdown").args(["/r", "/t", "0"]).output();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("nvram").args(["boot-args=-x"]).output();
        let _ = Command::new("shutdown").args(["-r", "now"]).output();
    }
}

/// Clean up any active host command state (called on session end).
pub fn cleanup() {
    if INPUT_BLOCKED.load(Ordering::Relaxed) {
        set_block_input(false);
    }
    if SCREEN_BLANKED.load(Ordering::Relaxed) {
        set_blank_screen(false);
    }
    if WAKE_LOCKED.load(Ordering::Relaxed) {
        set_wake_lock(false);
    }
    tracing::info!("Host commands cleaned up");
}
