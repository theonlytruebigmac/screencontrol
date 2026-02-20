#![allow(dead_code)]
//! Input injection for remote desktop sessions.
//!
//! Platform-specific backends:
//! - **Linux/Wayland (GNOME)**: Mutter RemoteDesktop D-Bus API — zero-overhead,
//!   no portal consent dialog.
//! - **macOS**: CoreGraphics event posting via FFI.
//! - **Windows**: Native Win32 `SendInput` API for lowest latency.
//! - **Fallback**: `enigo` crate (X11 / macOS / Windows).

use sc_protocol::input_event;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Tracks pressed keys and mouse buttons with timestamps.
/// Used to detect and release "stuck" keys when sessions drop or timeout.
pub struct KeyTracker {
    /// Maps key code → press timestamp
    pressed_keys: HashMap<u32, Instant>,
    /// Maps button id → press timestamp
    pressed_buttons: HashMap<u32, Instant>,
}

impl KeyTracker {
    pub fn new() -> Self {
        Self {
            pressed_keys: HashMap::new(),
            pressed_buttons: HashMap::new(),
        }
    }

    /// Record a key press or release.
    pub fn track_key(&mut self, key_code: u32, pressed: bool) {
        if pressed {
            self.pressed_keys.insert(key_code, Instant::now());
        } else {
            self.pressed_keys.remove(&key_code);
        }
    }

    /// Record a mouse button press or release.
    pub fn track_button(&mut self, button: u32, pressed: bool) {
        if pressed {
            self.pressed_buttons.insert(button, Instant::now());
        } else {
            self.pressed_buttons.remove(&button);
        }
    }

    /// Returns key codes that have been held longer than `max_duration`.
    pub fn stuck_keys(&self, max_duration: Duration) -> Vec<u32> {
        let now = Instant::now();
        self.pressed_keys
            .iter()
            .filter(|(_, ts)| now.duration_since(**ts) > max_duration)
            .map(|(code, _)| *code)
            .collect()
    }

    /// Returns button ids that have been held longer than `max_duration`.
    pub fn stuck_buttons(&self, max_duration: Duration) -> Vec<u32> {
        let now = Instant::now();
        self.pressed_buttons
            .iter()
            .filter(|(_, ts)| now.duration_since(**ts) > max_duration)
            .map(|(btn, _)| *btn)
            .collect()
    }

    /// Remove all tracked keys and buttons (call on session end).
    pub fn clear(&mut self) {
        self.pressed_keys.clear();
        self.pressed_buttons.clear();
    }

    /// Number of currently pressed keys.
    pub fn key_count(&self) -> usize {
        self.pressed_keys.len()
    }

    /// Number of currently pressed buttons.
    pub fn button_count(&self) -> usize {
        self.pressed_buttons.len()
    }
}

/// Handles input injection for desktop sessions.
pub enum InputInjector {
    #[cfg(target_os = "linux")]
    Mutter(MutterInputInjector),
    #[cfg(target_os = "macos")]
    CoreGraphics(CoreGraphicsInputInjector),
    #[cfg(target_os = "windows")]
    Win32(Win32InputInjector),
    #[allow(dead_code)]
    Enigo(EnigoInputInjector),
}

impl InputInjector {
    /// Dispatch a protobuf `InputEvent` to native input simulation.
    pub fn handle_event(&mut self, event: &sc_protocol::InputEvent) {
        match self {
            #[cfg(target_os = "linux")]
            InputInjector::Mutter(m) => m.handle_event(event),
            #[cfg(target_os = "macos")]
            InputInjector::CoreGraphics(cg) => cg.handle_event(event),
            #[cfg(target_os = "windows")]
            InputInjector::Win32(w) => w.handle_event(event),
            InputInjector::Enigo(e) => e.handle_event(event),
        }
    }

    /// Release all held modifier keys to prevent "stuck key" syndrome.
    /// Should be called when a desktop session ends.
    pub fn release_all_keys(&mut self) {
        match self {
            #[cfg(target_os = "linux")]
            InputInjector::Mutter(m) => m.release_all_modifiers(),
            #[cfg(target_os = "macos")]
            InputInjector::CoreGraphics(cg) => cg.release_all_modifiers(),
            #[cfg(target_os = "windows")]
            InputInjector::Win32(w) => w.release_all_modifiers(),
            InputInjector::Enigo(e) => e.release_all_modifiers(),
        }
    }

    /// Update the monitor geometry for multi-monitor coordinate mapping.
    /// Backends that support per-monitor offsets will use this to
    /// transform normalized coordinates to the correct monitor.
    pub fn set_monitor_geometry(&mut self, geometry: MonitorGeometry) {
        tracing::info!(?geometry, "Setting monitor geometry on input injector");
        match self {
            #[cfg(target_os = "linux")]
            InputInjector::Mutter(m) => {
                m.screen_width = geometry.width;
                m.screen_height = geometry.height;
                // Mutter D-Bus uses per-monitor coordinates already
            }
            #[cfg(target_os = "macos")]
            InputInjector::CoreGraphics(cg) => {
                cg.screen_width = geometry.width;
                cg.screen_height = geometry.height;
            }
            #[cfg(target_os = "windows")]
            InputInjector::Win32(w) => {
                w.geometry = geometry;
            }
            InputInjector::Enigo(e) => {
                e.screen_width = geometry.width;
                e.screen_height = geometry.height;
            }
        }
    }
}

// ─── Multi-Monitor Coordinate Mapping ──────────────────────────────────

/// Describes the geometry of the monitor being captured for input injection.
/// Converts normalized (0.0–1.0) canvas coordinates into absolute pixel
/// coordinates that account for multi-monitor offset.
#[derive(Debug, Clone)]
pub struct MonitorGeometry {
    /// Offset of this monitor's origin from the virtual desktop origin.
    pub x_offset: i32,
    pub y_offset: i32,
    /// Pixel dimensions of this monitor.
    pub width: u32,
    pub height: u32,
    /// Total virtual desktop size (needed for platforms like Win32 that
    /// use 0–65535 absolute coordinates spanning the entire desktop).
    pub total_width: u32,
    pub total_height: u32,
}

impl MonitorGeometry {
    /// Create geometry for a single-monitor (or full-desktop) setup.
    pub fn single(screen_width: u32, screen_height: u32) -> Self {
        Self {
            x_offset: 0,
            y_offset: 0,
            width: screen_width,
            height: screen_height,
            total_width: screen_width,
            total_height: screen_height,
        }
    }

    /// Create geometry for a specific monitor within a multi-monitor desktop.
    pub fn for_monitor(
        mon_x: i32,
        mon_y: i32,
        mon_w: u32,
        mon_h: u32,
        total_w: u32,
        total_h: u32,
    ) -> Self {
        Self {
            x_offset: mon_x,
            y_offset: mon_y,
            width: mon_w,
            height: mon_h,
            total_width: total_w,
            total_height: total_h,
        }
    }

    /// Convert normalized (0.0–1.0) coordinates to absolute pixel coordinates.
    pub fn to_absolute(&self, norm_x: f64, norm_y: f64) -> (f64, f64) {
        let abs_x = self.x_offset as f64 + norm_x * self.width as f64;
        let abs_y = self.y_offset as f64 + norm_y * self.height as f64;
        (abs_x, abs_y)
    }

    /// Convert normalized coordinates to Win32 absolute 0–65535 range.
    pub fn to_absolute_65535(&self, norm_x: f64, norm_y: f64) -> (i32, i32) {
        let (abs_x, abs_y) = self.to_absolute(norm_x, norm_y);
        let x = (abs_x / self.total_width as f64 * 65535.0) as i32;
        let y = (abs_y / self.total_height as f64 * 65535.0) as i32;
        (x, y)
    }
}

// ─── Mutter D-Bus Backend (Linux/Wayland) ──────────────────────────────

#[cfg(target_os = "linux")]
pub struct MutterInputInjector {
    connection: zbus::Connection,
    rd_session_path: zbus::zvariant::OwnedObjectPath,
    stream_path: String,
    screen_width: u32,
    screen_height: u32,
    /// Tokio runtime handle for blocking D-Bus calls from sync context
    rt: tokio::runtime::Handle,
}

#[cfg(target_os = "linux")]
impl MutterInputInjector {
    pub fn from_handle(handle: crate::screen::MutterInputHandle) -> Self {
        tracing::info!(
            screen_width = handle.screen_width,
            screen_height = handle.screen_height,
            rd_session = %handle.rd_session_path,
            stream = %handle.stream_path,
            "MutterInputInjector created (D-Bus RemoteDesktop)"
        );
        Self {
            connection: handle.connection,
            rd_session_path: handle.rd_session_path,
            stream_path: handle.stream_path.as_str().to_string(),
            screen_width: handle.screen_width,
            screen_height: handle.screen_height,
            rt: tokio::runtime::Handle::current(),
        }
    }

    pub fn handle_event(&mut self, event: &sc_protocol::InputEvent) {
        match &event.event {
            Some(input_event::Event::MouseMove(mv)) => {
                let x = mv.x * self.screen_width as f64;
                let y = mv.y * self.screen_height as f64;
                tracing::trace!(norm_x = mv.x, norm_y = mv.y, "MouseMove (D-Bus)");
                self.notify_pointer_motion_absolute(x, y);
            }
            Some(input_event::Event::MouseButton(btn)) => {
                let x = btn.x * self.screen_width as f64;
                let y = btn.y * self.screen_height as f64;
                tracing::info!(
                    button = btn.button,
                    pressed = btn.pressed,
                    norm_x = btn.x,
                    norm_y = btn.y,
                    "MouseButton (D-Bus)"
                );
                // Move to position, then click
                self.notify_pointer_motion_absolute(x, y);
                // Mutter button codes: BTN_LEFT=272, BTN_MIDDLE=274, BTN_RIGHT=273
                let linux_button: i32 = match btn.button {
                    0 => 272, // BTN_LEFT
                    1 => 274, // BTN_MIDDLE
                    2 => 273, // BTN_RIGHT
                    _ => 272,
                };
                self.notify_pointer_button(linux_button, btn.pressed);
            }
            Some(input_event::Event::MouseScroll(scroll)) => {
                let x = scroll.x * self.screen_width as f64;
                let y = scroll.y * self.screen_height as f64;
                // Move to position first
                self.notify_pointer_motion_absolute(x, y);

                if scroll.delta_y.abs() > 0.01 || scroll.delta_x.abs() > 0.01 {
                    let dx = scroll.delta_x * 10.0;
                    let dy = scroll.delta_y * 10.0;
                    self.notify_pointer_axis(dx, dy);
                }
            }
            Some(input_event::Event::KeyEvent(key)) => {
                tracing::info!(
                    key_code = key.key_code,
                    pressed = key.pressed,
                    "KeyEvent (D-Bus)"
                );
                if let Some(evdev_code) = map_web_keycode_to_evdev(key.key_code) {
                    self.notify_keyboard_keycode(evdev_code, key.pressed);
                }
            }
            Some(input_event::Event::RelativeMouseMove(rel)) => {
                tracing::trace!(dx = rel.dx, dy = rel.dy, "RelativeMouseMove (D-Bus)");
                self.notify_pointer_motion_relative(rel.dx as f64, rel.dy as f64);
            }
            None => {}
        }
    }

    fn notify_pointer_motion_absolute(&self, x: f64, y: f64) {
        let conn = self.connection.clone();
        let path = self.rd_session_path.clone();
        let stream = self.stream_path.clone();
        let _ = self.rt.spawn(async move {
            if let Err(e) = conn
                .call_method(
                    Some("org.gnome.Mutter.RemoteDesktop"),
                    path.as_ref(),
                    Some("org.gnome.Mutter.RemoteDesktop.Session"),
                    "NotifyPointerMotionAbsolute",
                    &(stream.as_str(), x, y),
                )
                .await
            {
                tracing::warn!("NotifyPointerMotionAbsolute failed: {}", e);
            }
        });
    }

    fn notify_pointer_motion_relative(&self, dx: f64, dy: f64) {
        let conn = self.connection.clone();
        let path = self.rd_session_path.clone();
        let _ = self.rt.spawn(async move {
            if let Err(e) = conn
                .call_method(
                    Some("org.gnome.Mutter.RemoteDesktop"),
                    path.as_ref(),
                    Some("org.gnome.Mutter.RemoteDesktop.Session"),
                    "NotifyPointerMotionRelative",
                    &(dx, dy),
                )
                .await
            {
                tracing::warn!("NotifyPointerMotionRelative failed: {}", e);
            }
        });
    }

    fn notify_pointer_button(&self, button: i32, pressed: bool) {
        let conn = self.connection.clone();
        let path = self.rd_session_path.clone();
        // state: true = pressed
        let _ = self.rt.spawn(async move {
            if let Err(e) = conn
                .call_method(
                    Some("org.gnome.Mutter.RemoteDesktop"),
                    path.as_ref(),
                    Some("org.gnome.Mutter.RemoteDesktop.Session"),
                    "NotifyPointerButton",
                    &(button, pressed),
                )
                .await
            {
                tracing::warn!("NotifyPointerButton failed: {}", e);
            }
        });
    }

    fn notify_pointer_axis(&self, dx: f64, dy: f64) {
        let conn = self.connection.clone();
        let path = self.rd_session_path.clone();
        let flags: u32 = 0; // CLUTTER_SCROLL_FLAG_NONE
        let _ = self.rt.spawn(async move {
            if let Err(e) = conn
                .call_method(
                    Some("org.gnome.Mutter.RemoteDesktop"),
                    path.as_ref(),
                    Some("org.gnome.Mutter.RemoteDesktop.Session"),
                    "NotifyPointerAxis",
                    &(dx, dy, flags),
                )
                .await
            {
                tracing::warn!("NotifyPointerAxis failed: {}", e);
            }
        });
    }

    fn notify_keyboard_keycode(&self, keycode: u32, pressed: bool) {
        let conn = self.connection.clone();
        let path = self.rd_session_path.clone();
        let _ = self.rt.spawn(async move {
            if let Err(e) = conn
                .call_method(
                    Some("org.gnome.Mutter.RemoteDesktop"),
                    path.as_ref(),
                    Some("org.gnome.Mutter.RemoteDesktop.Session"),
                    "NotifyKeyboardKeycode",
                    &(keycode, pressed),
                )
                .await
            {
                tracing::warn!("NotifyKeyboardKeycode failed: {}", e);
            }
        });
    }

    /// Release all modifier keys (Shift, Ctrl, Alt, Meta).
    pub fn release_all_modifiers(&mut self) {
        tracing::info!("Releasing all modifier keys (Mutter D-Bus)");
        // evdev keycodes for modifiers
        for keycode in &[42u32, 54, 29, 97, 56, 100, 125, 126] {
            // 42=LShift, 54=RShift, 29=LCtrl, 97=RCtrl, 56=LAlt, 100=RAlt, 125=LMeta, 126=RMeta
            self.notify_keyboard_keycode(*keycode, false);
        }
    }
}

/// Map web KeyboardEvent.keyCode to Linux evdev keycode.
/// evdev keycodes are offset by 8 from X11 keycodes.
#[cfg(target_os = "linux")]
fn map_web_keycode_to_evdev(web_code: u32) -> Option<u32> {
    // Web keyCode → evdev keycode mapping
    let evdev = match web_code {
        // Letters A-Z
        65 => 30, // KEY_A
        66 => 48, // KEY_B
        67 => 46, // KEY_C
        68 => 32, // KEY_D
        69 => 18, // KEY_E
        70 => 33, // KEY_F
        71 => 34, // KEY_G
        72 => 35, // KEY_H
        73 => 23, // KEY_I
        74 => 36, // KEY_J
        75 => 37, // KEY_K
        76 => 38, // KEY_L
        77 => 50, // KEY_M
        78 => 49, // KEY_N
        79 => 24, // KEY_O
        80 => 25, // KEY_P
        81 => 16, // KEY_Q
        82 => 19, // KEY_R
        83 => 31, // KEY_S
        84 => 20, // KEY_T
        85 => 22, // KEY_U
        86 => 47, // KEY_V
        87 => 17, // KEY_W
        88 => 45, // KEY_X
        89 => 21, // KEY_Y
        90 => 44, // KEY_Z
        // Digits 0-9
        48 => 11, // KEY_0
        49 => 2,  // KEY_1
        50 => 3,  // KEY_2
        51 => 4,  // KEY_3
        52 => 5,  // KEY_4
        53 => 6,  // KEY_5
        54 => 7,  // KEY_6
        55 => 8,  // KEY_7
        56 => 9,  // KEY_8
        57 => 10, // KEY_9
        // Function keys
        112 => 59, // KEY_F1
        113 => 60, // KEY_F2
        114 => 61, // KEY_F3
        115 => 62, // KEY_F4
        116 => 63, // KEY_F5
        117 => 64, // KEY_F6
        118 => 65, // KEY_F7
        119 => 66, // KEY_F8
        120 => 67, // KEY_F9
        121 => 68, // KEY_F10
        122 => 87, // KEY_F11
        123 => 88, // KEY_F12
        // Special keys
        8 => 14,   // KEY_BACKSPACE
        9 => 15,   // KEY_TAB
        13 => 28,  // KEY_ENTER
        16 => 42,  // KEY_LEFTSHIFT
        17 => 29,  // KEY_LEFTCTRL
        18 => 56,  // KEY_LEFTALT
        20 => 58,  // KEY_CAPSLOCK
        27 => 1,   // KEY_ESC
        32 => 57,  // KEY_SPACE
        33 => 104, // KEY_PAGEUP
        34 => 109, // KEY_PAGEDOWN
        35 => 107, // KEY_END
        36 => 102, // KEY_HOME
        37 => 105, // KEY_LEFT
        38 => 103, // KEY_UP
        39 => 106, // KEY_RIGHT
        40 => 108, // KEY_DOWN
        45 => 110, // KEY_INSERT
        46 => 111, // KEY_DELETE
        91 => 125, // KEY_LEFTMETA (Super/Windows)
        // Numpad
        96 => 82,  // KEY_KP0
        97 => 79,  // KEY_KP1
        98 => 80,  // KEY_KP2
        99 => 81,  // KEY_KP3
        100 => 75, // KEY_KP4
        101 => 76, // KEY_KP5
        102 => 77, // KEY_KP6
        103 => 71, // KEY_KP7
        104 => 72, // KEY_KP8
        105 => 73, // KEY_KP9
        106 => 55, // KEY_KPASTERISK (Numpad *)
        107 => 78, // KEY_KPPLUS
        109 => 74, // KEY_KPMINUS
        110 => 83, // KEY_KPDOT
        111 => 98, // KEY_KPSLASH
        // Toggle / lock keys
        19 => 119, // KEY_PAUSE
        44 => 99,  // KEY_SYSRQ (PrintScreen)
        144 => 69, // KEY_NUMLOCK
        145 => 70, // KEY_SCROLLLOCK
        93 => 127, // KEY_COMPOSE (ContextMenu)
        // Punctuation
        186 => 39, // KEY_SEMICOLON
        187 => 13, // KEY_EQUAL
        188 => 51, // KEY_COMMA
        189 => 12, // KEY_MINUS
        190 => 52, // KEY_DOT
        191 => 53, // KEY_SLASH
        192 => 41, // KEY_GRAVE
        219 => 26, // KEY_LEFTBRACE
        220 => 43, // KEY_BACKSLASH
        221 => 27, // KEY_RIGHTBRACE
        222 => 40, // KEY_APOSTROPHE
        _ => {
            tracing::debug!("Unmapped web keycode: {}", web_code);
            return None;
        }
    };
    Some(evdev)
}

// ─── CoreGraphics Backend (macOS native) ─────────────────────────────

#[cfg(target_os = "macos")]
pub struct CoreGraphicsInputInjector {
    screen_width: u32,
    screen_height: u32,
    /// Bitmask of currently held mouse buttons (bit 0=left, bit 1=right, bit 2=middle)
    buttons_down: u8,
    /// Timestamp of the last mouse-down event (for double-click detection)
    last_click_time: Option<std::time::Instant>,
    /// Current click count (1=single, 2=double, 3=triple)
    click_count: i64,
    /// Current modifier flags bitmask (CGEventFlags)
    modifier_flags: u64,
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod cg_ffi {
    use std::ffi::c_void;

    pub type CGEventRef = *const c_void;
    pub type CGEventSourceRef = *const c_void;

    // CGEventType values
    pub const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
    pub const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
    pub const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
    pub const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
    pub const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
    pub const K_CG_EVENT_LEFT_MOUSE_DRAGGED: u32 = 6;
    pub const K_CG_EVENT_RIGHT_MOUSE_DRAGGED: u32 = 7;
    pub const K_CG_EVENT_KEY_DOWN: u32 = 10;
    pub const K_CG_EVENT_KEY_UP: u32 = 11;
    pub const K_CG_EVENT_SCROLL_WHEEL: u32 = 22;
    pub const K_CG_EVENT_OTHER_MOUSE_DOWN: u32 = 25;
    pub const K_CG_EVENT_OTHER_MOUSE_UP: u32 = 26;
    pub const K_CG_EVENT_OTHER_MOUSE_DRAGGED: u32 = 27;

    // CGEventField values for SetIntegerValueField
    pub const K_CG_MOUSE_EVENT_CLICK_STATE: u32 = 1;

    // CGEventFlags bitmask constants
    pub const K_CG_EVENT_FLAG_SHIFT: u64 = 0x00020000;
    pub const K_CG_EVENT_FLAG_CONTROL: u64 = 0x00040000;
    pub const K_CG_EVENT_FLAG_ALTERNATE: u64 = 0x00080000; // Option/Alt
    pub const K_CG_EVENT_FLAG_COMMAND: u64 = 0x00100000;

    // CGEventTapLocation
    pub const K_CG_HID_EVENT_TAP: u32 = 0;

    // CGMouseButton
    pub const K_CG_MOUSE_BUTTON_LEFT: u32 = 0;
    pub const K_CG_MOUSE_BUTTON_RIGHT: u32 = 1;
    pub const K_CG_MOUSE_BUTTON_CENTER: u32 = 2;

    // CGScrollEventUnit
    pub const K_CG_SCROLL_EVENT_UNIT_LINE: u32 = 1;

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGEventCreateMouseEvent(
            source: CGEventSourceRef,
            mouse_type: u32,
            mouse_cursor_position: CGPoint,
            mouse_button: u32,
        ) -> CGEventRef;

        pub fn CGEventCreateKeyboardEvent(
            source: CGEventSourceRef,
            virtual_key: u16,
            key_down: bool,
        ) -> CGEventRef;

        pub fn CGEventCreateScrollWheelEvent2(
            source: CGEventSourceRef,
            units: u32,
            wheel_count: u32,
            wheel1: i32,
            wheel2: i32,
            wheel3: i32,
        ) -> CGEventRef;

        pub fn CGEventPost(tap: u32, event: CGEventRef);

        pub fn CGEventSetFlags(event: CGEventRef, flags: u64);

        pub fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);

        pub fn CFRelease(cf: *const c_void);

        pub fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;

        pub fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    }
}

#[cfg(target_os = "macos")]
impl CoreGraphicsInputInjector {
    /// Double-click interval in milliseconds (macOS default is ~500ms).
    const DOUBLE_CLICK_INTERVAL_MS: u128 = 500;

    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        tracing::info!(
            screen_width,
            screen_height,
            "Creating CoreGraphicsInputInjector (native CG events)"
        );
        Self {
            screen_width,
            screen_height,
            buttons_down: 0,
            last_click_time: None,
            click_count: 1,
            modifier_flags: 0,
        }
    }

    pub fn handle_event(&mut self, event: &sc_protocol::InputEvent) {
        match &event.event {
            Some(input_event::Event::MouseMove(mv)) => {
                let x = mv.x * self.screen_width as f64;
                let y = mv.y * self.screen_height as f64;
                self.post_mouse_move(x, y);
            }
            Some(input_event::Event::MouseButton(btn)) => {
                let x = btn.x * self.screen_width as f64;
                let y = btn.y * self.screen_height as f64;
                // Track button state for drag event type selection
                let bit = match btn.button {
                    0 => 0x01, // left
                    2 => 0x02, // right
                    1 => 0x04, // middle
                    _ => 0x01,
                };
                if btn.pressed {
                    self.buttons_down |= bit;
                } else {
                    self.buttons_down &= !bit;
                }
                self.post_mouse_button(x, y, btn.button, btn.pressed);
            }
            Some(input_event::Event::MouseScroll(scroll)) => {
                let dy = if scroll.delta_y.abs() > 0.01 {
                    -(scroll.delta_y * 3.0) as i32 // Inverted: positive deltaY = scroll down
                } else {
                    0
                };
                let dx = if scroll.delta_x.abs() > 0.01 {
                    (scroll.delta_x * 3.0) as i32
                } else {
                    0
                };
                if dy != 0 || dx != 0 {
                    self.post_scroll(dy, dx);
                }
            }
            Some(input_event::Event::KeyEvent(key)) => {
                if let Some(vk) = map_web_keycode_to_macos(key.key_code) {
                    // Track modifier flags for mouse events (1c)
                    self.update_modifier_flags(vk, key.pressed);
                    self.post_key(vk, key.pressed);
                }
            }
            Some(input_event::Event::RelativeMouseMove(rel)) => {
                self.post_relative_mouse_move(rel.dx, rel.dy);
            }
            None => {}
        }
    }

    /// Update tracked modifier flags based on virtual key code.
    fn update_modifier_flags(&mut self, vk: u16, pressed: bool) {
        let flag = match vk {
            0x38 | 0x3C => cg_ffi::K_CG_EVENT_FLAG_SHIFT, // Shift L/R
            0x3B | 0x3E => cg_ffi::K_CG_EVENT_FLAG_CONTROL, // Control L/R
            0x3A | 0x3D => cg_ffi::K_CG_EVENT_FLAG_ALTERNATE, // Option L/R
            0x37 | 0x36 => cg_ffi::K_CG_EVENT_FLAG_COMMAND, // Command L/R
            _ => return,
        };
        if pressed {
            self.modifier_flags |= flag;
        } else {
            self.modifier_flags &= !flag;
        }
    }

    fn post_relative_mouse_move(&self, dx: i32, dy: i32) {
        unsafe {
            // Get current cursor position to warp from
            let mut point = cg_ffi::CGPoint { x: 0.0, y: 0.0 };
            // Use CGEventCreate+CGEventGetLocation to get current position
            let event = cg_ffi::CGEventCreate(std::ptr::null());
            if !event.is_null() {
                point = cg_ffi::CGEventGetLocation(event);
                cg_ffi::CFRelease(event as *const _);
            }
            let new_point = cg_ffi::CGPoint {
                x: point.x + dx as f64,
                y: point.y + dy as f64,
            };
            let event_type = if self.buttons_down & 0x01 != 0 {
                cg_ffi::K_CG_EVENT_LEFT_MOUSE_DRAGGED
            } else if self.buttons_down & 0x02 != 0 {
                cg_ffi::K_CG_EVENT_RIGHT_MOUSE_DRAGGED
            } else if self.buttons_down & 0x04 != 0 {
                cg_ffi::K_CG_EVENT_OTHER_MOUSE_DRAGGED
            } else {
                cg_ffi::K_CG_EVENT_MOUSE_MOVED
            };
            let cg_button = cg_ffi::K_CG_MOUSE_BUTTON_LEFT;
            let ev =
                cg_ffi::CGEventCreateMouseEvent(std::ptr::null(), event_type, new_point, cg_button);
            if !ev.is_null() {
                cg_ffi::CGEventPost(cg_ffi::K_CG_HID_EVENT_TAP, ev);
                cg_ffi::CFRelease(ev as *const _);
            }
        }
    }

    fn post_mouse_move(&self, x: f64, y: f64) {
        unsafe {
            let point = cg_ffi::CGPoint { x, y };
            // Fix 1a: Use correct event type based on held buttons (drag vs move)
            let (event_type, cg_button) = if self.buttons_down & 0x01 != 0 {
                (
                    cg_ffi::K_CG_EVENT_LEFT_MOUSE_DRAGGED,
                    cg_ffi::K_CG_MOUSE_BUTTON_LEFT,
                )
            } else if self.buttons_down & 0x02 != 0 {
                (
                    cg_ffi::K_CG_EVENT_RIGHT_MOUSE_DRAGGED,
                    cg_ffi::K_CG_MOUSE_BUTTON_RIGHT,
                )
            } else if self.buttons_down & 0x04 != 0 {
                (
                    cg_ffi::K_CG_EVENT_OTHER_MOUSE_DRAGGED,
                    cg_ffi::K_CG_MOUSE_BUTTON_CENTER,
                )
            } else {
                (
                    cg_ffi::K_CG_EVENT_MOUSE_MOVED,
                    cg_ffi::K_CG_MOUSE_BUTTON_LEFT,
                )
            };
            let event =
                cg_ffi::CGEventCreateMouseEvent(std::ptr::null(), event_type, point, cg_button);
            if !event.is_null() {
                // Fix 1c: Set modifier flags on mouse events
                if self.modifier_flags != 0 {
                    cg_ffi::CGEventSetFlags(event, self.modifier_flags);
                }
                cg_ffi::CGEventPost(cg_ffi::K_CG_HID_EVENT_TAP, event);
                cg_ffi::CFRelease(event);
            }
        }
    }

    fn post_mouse_button(&mut self, x: f64, y: f64, button: u32, pressed: bool) {
        // Fix 1b: Track click count for double/triple click
        if pressed {
            let now = std::time::Instant::now();
            if let Some(last) = self.last_click_time {
                if last.elapsed().as_millis() <= Self::DOUBLE_CLICK_INTERVAL_MS {
                    self.click_count += 1;
                } else {
                    self.click_count = 1;
                }
            } else {
                self.click_count = 1;
            }
            self.last_click_time = Some(now);
        }

        unsafe {
            let point = cg_ffi::CGPoint { x, y };
            let (event_type, cg_button) = match (button, pressed) {
                (0, true) => (
                    cg_ffi::K_CG_EVENT_LEFT_MOUSE_DOWN,
                    cg_ffi::K_CG_MOUSE_BUTTON_LEFT,
                ),
                (0, false) => (
                    cg_ffi::K_CG_EVENT_LEFT_MOUSE_UP,
                    cg_ffi::K_CG_MOUSE_BUTTON_LEFT,
                ),
                (1, true) => (
                    cg_ffi::K_CG_EVENT_OTHER_MOUSE_DOWN,
                    cg_ffi::K_CG_MOUSE_BUTTON_CENTER,
                ),
                (1, false) => (
                    cg_ffi::K_CG_EVENT_OTHER_MOUSE_UP,
                    cg_ffi::K_CG_MOUSE_BUTTON_CENTER,
                ),
                (2, true) => (
                    cg_ffi::K_CG_EVENT_RIGHT_MOUSE_DOWN,
                    cg_ffi::K_CG_MOUSE_BUTTON_RIGHT,
                ),
                (2, false) => (
                    cg_ffi::K_CG_EVENT_RIGHT_MOUSE_UP,
                    cg_ffi::K_CG_MOUSE_BUTTON_RIGHT,
                ),
                _ => (
                    cg_ffi::K_CG_EVENT_LEFT_MOUSE_DOWN,
                    cg_ffi::K_CG_MOUSE_BUTTON_LEFT,
                ),
            };
            // Move to position first (use correct drag type if button held)
            let move_type = if self.buttons_down & 0x01 != 0 {
                cg_ffi::K_CG_EVENT_LEFT_MOUSE_DRAGGED
            } else if self.buttons_down & 0x02 != 0 {
                cg_ffi::K_CG_EVENT_RIGHT_MOUSE_DRAGGED
            } else {
                cg_ffi::K_CG_EVENT_MOUSE_MOVED
            };
            let move_event =
                cg_ffi::CGEventCreateMouseEvent(std::ptr::null(), move_type, point, cg_button);
            if !move_event.is_null() {
                if self.modifier_flags != 0 {
                    cg_ffi::CGEventSetFlags(move_event, self.modifier_flags);
                }
                cg_ffi::CGEventPost(cg_ffi::K_CG_HID_EVENT_TAP, move_event);
                cg_ffi::CFRelease(move_event);
            }
            // Then click
            let click_event =
                cg_ffi::CGEventCreateMouseEvent(std::ptr::null(), event_type, point, cg_button);
            if !click_event.is_null() {
                // Fix 1b: Set click count for double/triple click
                if self.click_count > 1 {
                    cg_ffi::CGEventSetIntegerValueField(
                        click_event,
                        cg_ffi::K_CG_MOUSE_EVENT_CLICK_STATE,
                        self.click_count,
                    );
                }
                // Fix 1c: Set modifier flags on click events
                if self.modifier_flags != 0 {
                    cg_ffi::CGEventSetFlags(click_event, self.modifier_flags);
                }
                cg_ffi::CGEventPost(cg_ffi::K_CG_HID_EVENT_TAP, click_event);
                cg_ffi::CFRelease(click_event);
            }
        }
    }

    fn post_scroll(&self, dy: i32, dx: i32) {
        unsafe {
            let event = cg_ffi::CGEventCreateScrollWheelEvent2(
                std::ptr::null(),
                cg_ffi::K_CG_SCROLL_EVENT_UNIT_LINE,
                2, // wheelCount
                dy,
                dx,
                0,
            );
            if !event.is_null() {
                cg_ffi::CGEventPost(cg_ffi::K_CG_HID_EVENT_TAP, event);
                cg_ffi::CFRelease(event);
            }
        }
    }

    fn post_key(&self, virtual_key: u16, pressed: bool) {
        unsafe {
            let event = cg_ffi::CGEventCreateKeyboardEvent(std::ptr::null(), virtual_key, pressed);
            if !event.is_null() {
                cg_ffi::CGEventPost(cg_ffi::K_CG_HID_EVENT_TAP, event);
                cg_ffi::CFRelease(event);
            }
        }
        // Phase 4: 12ms busy-wait after keyboard events.
        // macOS `thread::sleep` is unreliable when running as a LaunchAgent
        // under launchctl. The kernel may suspend the thread for much longer
        // than requested, causing dropped keystrokes. A spin loop guarantees
        // the minimum delay that macOS needs between keyboard events.
        Self::key_sleep();
    }

    /// Busy-wait for 12ms to let macOS process the keyboard event.
    /// Using `Instant`-based spin loop instead of `thread::sleep` because
    /// sleep is unreliable under launchctl (may oversleep by 10-100x).
    #[inline]
    fn key_sleep() {
        let target = Instant::now() + Duration::from_micros(12_000);
        while Instant::now() < target {
            std::hint::spin_loop();
        }
    }

    /// Release all modifier keys (Shift, Ctrl, Option, Command).
    pub fn release_all_modifiers(&mut self) {
        tracing::info!("Releasing all modifier keys (CoreGraphics)");
        // macOS virtual key codes for modifiers
        for &vk in &[
            0x38u16, 0x3C, // Shift L/R
            0x3B, 0x3E, // Control L/R
            0x3A, 0x3D, // Option (Alt) L/R
            0x37, 0x36, // Command L/R
        ] {
            self.post_key(vk, false);
        }
        self.modifier_flags = 0;
        self.buttons_down = 0;
    }
}

/// Map web KeyboardEvent.keyCode to macOS virtual key code.
#[cfg(target_os = "macos")]
fn map_web_keycode_to_macos(web_code: u32) -> Option<u16> {
    let vk: u16 = match web_code {
        // Letters A-Z
        65 => 0x00, // kVK_ANSI_A
        66 => 0x0B, // kVK_ANSI_B
        67 => 0x08, // kVK_ANSI_C
        68 => 0x02, // kVK_ANSI_D
        69 => 0x0E, // kVK_ANSI_E
        70 => 0x03, // kVK_ANSI_F
        71 => 0x05, // kVK_ANSI_G
        72 => 0x04, // kVK_ANSI_H
        73 => 0x22, // kVK_ANSI_I
        74 => 0x26, // kVK_ANSI_J
        75 => 0x28, // kVK_ANSI_K
        76 => 0x25, // kVK_ANSI_L
        77 => 0x2E, // kVK_ANSI_M
        78 => 0x2D, // kVK_ANSI_N
        79 => 0x1F, // kVK_ANSI_O
        80 => 0x23, // kVK_ANSI_P
        81 => 0x0C, // kVK_ANSI_Q
        82 => 0x0F, // kVK_ANSI_R
        83 => 0x01, // kVK_ANSI_S
        84 => 0x11, // kVK_ANSI_T
        85 => 0x20, // kVK_ANSI_U
        86 => 0x09, // kVK_ANSI_V
        87 => 0x0D, // kVK_ANSI_W
        88 => 0x07, // kVK_ANSI_X
        89 => 0x10, // kVK_ANSI_Y
        90 => 0x06, // kVK_ANSI_Z
        // Digits 0-9
        48 => 0x1D, // kVK_ANSI_0
        49 => 0x12, // kVK_ANSI_1
        50 => 0x13, // kVK_ANSI_2
        51 => 0x14, // kVK_ANSI_3
        52 => 0x15, // kVK_ANSI_4
        53 => 0x17, // kVK_ANSI_5
        54 => 0x16, // kVK_ANSI_6
        55 => 0x1A, // kVK_ANSI_7
        56 => 0x1C, // kVK_ANSI_8
        57 => 0x19, // kVK_ANSI_9
        // Function keys
        112 => 0x7A, // kVK_F1
        113 => 0x78, // kVK_F2
        114 => 0x63, // kVK_F3
        115 => 0x76, // kVK_F4
        116 => 0x60, // kVK_F5
        117 => 0x61, // kVK_F6
        118 => 0x62, // kVK_F7
        119 => 0x64, // kVK_F8
        120 => 0x65, // kVK_F9
        121 => 0x6D, // kVK_F10
        122 => 0x67, // kVK_F11
        123 => 0x6F, // kVK_F12
        // Special keys
        8 => 0x33,  // kVK_Delete (Backspace)
        9 => 0x30,  // kVK_Tab
        13 => 0x24, // kVK_Return
        16 => 0x38, // kVK_Shift
        17 => 0x3B, // kVK_Control
        18 => 0x3A, // kVK_Option (Alt)
        20 => 0x39, // kVK_CapsLock
        27 => 0x35, // kVK_Escape
        32 => 0x31, // kVK_Space
        33 => 0x74, // kVK_PageUp
        34 => 0x79, // kVK_PageDown
        35 => 0x77, // kVK_End
        36 => 0x73, // kVK_Home
        37 => 0x7B, // kVK_LeftArrow
        38 => 0x7E, // kVK_UpArrow
        39 => 0x7C, // kVK_RightArrow
        40 => 0x7D, // kVK_DownArrow
        46 => 0x75, // kVK_ForwardDelete
        91 => 0x37, // kVK_Command (Meta/Super)
        // Numpad
        96 => 0x52,  // kVK_ANSI_Keypad0
        97 => 0x53,  // kVK_ANSI_Keypad1
        98 => 0x54,  // kVK_ANSI_Keypad2
        99 => 0x55,  // kVK_ANSI_Keypad3
        100 => 0x56, // kVK_ANSI_Keypad4
        101 => 0x57, // kVK_ANSI_Keypad5
        102 => 0x58, // kVK_ANSI_Keypad6
        103 => 0x59, // kVK_ANSI_Keypad7
        104 => 0x5B, // kVK_ANSI_Keypad8
        105 => 0x5C, // kVK_ANSI_Keypad9
        106 => 0x43, // kVK_ANSI_KeypadMultiply
        107 => 0x45, // kVK_ANSI_KeypadPlus
        109 => 0x4E, // kVK_ANSI_KeypadMinus
        110 => 0x41, // kVK_ANSI_KeypadDecimal
        111 => 0x4B, // kVK_ANSI_KeypadDivide
        // Punctuation
        186 => 0x29, // kVK_ANSI_Semicolon
        187 => 0x18, // kVK_ANSI_Equal
        188 => 0x2B, // kVK_ANSI_Comma
        189 => 0x1B, // kVK_ANSI_Minus
        190 => 0x2F, // kVK_ANSI_Period
        191 => 0x2C, // kVK_ANSI_Slash
        192 => 0x32, // kVK_ANSI_Grave
        219 => 0x21, // kVK_ANSI_LeftBracket
        220 => 0x2A, // kVK_ANSI_Backslash
        221 => 0x1E, // kVK_ANSI_RightBracket
        222 => 0x27, // kVK_ANSI_Quote
        _ => {
            tracing::debug!("Unmapped web keycode for macOS: {}", web_code);
            return None;
        }
    };
    Some(vk)
}

// ─── Enigo Backend (fallback for X11 / macOS / Windows) ────────────────

pub struct EnigoInputInjector {
    enigo: enigo::Enigo,
    screen_width: u32,
    screen_height: u32,
}

impl EnigoInputInjector {
    #[allow(dead_code)]
    pub fn new(screen_width: u32, screen_height: u32) -> anyhow::Result<Self> {
        use enigo::Settings;

        tracing::info!(
            screen_width,
            screen_height,
            "Creating EnigoInputInjector (X11/fallback)"
        );

        let enigo = enigo::Enigo::new(&Settings::default())
            .map_err(|e| anyhow::anyhow!("Failed to create Enigo instance: {:?}", e))?;

        tracing::info!("Enigo input injector created successfully");

        Ok(Self {
            enigo,
            screen_width,
            screen_height,
        })
    }

    pub fn handle_event(&mut self, event: &sc_protocol::InputEvent) {
        use enigo::{Axis, Button, Coordinate, Direction, Keyboard, Mouse};

        match &event.event {
            Some(input_event::Event::MouseMove(mv)) => {
                let x = (mv.x * self.screen_width as f64) as i32;
                let y = (mv.y * self.screen_height as f64) as i32;
                if let Err(e) = self.enigo.move_mouse(x, y, Coordinate::Abs) {
                    tracing::warn!("enigo move_mouse failed: {:?}", e);
                }
            }
            Some(input_event::Event::MouseButton(btn)) => {
                let x = (btn.x * self.screen_width as f64) as i32;
                let y = (btn.y * self.screen_height as f64) as i32;
                if let Err(e) = self.enigo.move_mouse(x, y, Coordinate::Abs) {
                    tracing::warn!("enigo move_mouse failed: {:?}", e);
                }
                let button = match btn.button {
                    0 => Button::Left,
                    1 => Button::Middle,
                    2 => Button::Right,
                    _ => Button::Left,
                };
                let direction = if btn.pressed {
                    Direction::Press
                } else {
                    Direction::Release
                };
                if let Err(e) = self.enigo.button(button, direction) {
                    tracing::warn!("enigo button failed: {:?}", e);
                }
            }
            Some(input_event::Event::MouseScroll(scroll)) => {
                if scroll.delta_y.abs() > 0.01 {
                    let clicks = (scroll.delta_y * 3.0) as i32;
                    if let Err(e) = self.enigo.scroll(clicks, Axis::Vertical) {
                        tracing::warn!("enigo scroll failed: {:?}", e);
                    }
                }
                if scroll.delta_x.abs() > 0.01 {
                    let clicks = (scroll.delta_x * 3.0) as i32;
                    if let Err(e) = self.enigo.scroll(clicks, Axis::Horizontal) {
                        tracing::warn!("enigo scroll failed: {:?}", e);
                    }
                }
            }
            Some(input_event::Event::KeyEvent(key)) => {
                if let Some(enigo_key) = map_key_code(key.key_code) {
                    let direction = if key.pressed {
                        Direction::Press
                    } else {
                        Direction::Release
                    };
                    if let Err(e) = self.enigo.key(enigo_key, direction) {
                        tracing::warn!("enigo key failed: {:?}", e);
                    }
                }
            }
            Some(input_event::Event::RelativeMouseMove(rel)) => {
                if let Err(e) = self.enigo.move_mouse(rel.dx, rel.dy, Coordinate::Rel) {
                    tracing::warn!("enigo relative move_mouse failed: {:?}", e);
                }
            }
            None => {}
        }
    }

    /// Release all modifier keys via Enigo.
    pub fn release_all_modifiers(&mut self) {
        use enigo::{Direction, Key, Keyboard};
        tracing::info!("Releasing all modifier keys (Enigo)");
        for key in &[Key::Shift, Key::Control, Key::Alt, Key::Meta] {
            let _ = self.enigo.key(*key, Direction::Release);
        }
    }
}

/// Map a platform-independent key code to an enigo `Key`.
/// Uses web KeyboardEvent.keyCode values.
fn map_key_code(code: u32) -> Option<enigo::Key> {
    use enigo::Key;

    match code {
        // Letters A-Z (65-90)
        65..=90 => {
            let ch = (code as u8) as char;
            Some(Key::Unicode(ch.to_ascii_lowercase()))
        }
        // Digits 0-9 (48-57)
        48..=57 => {
            let ch = (code as u8) as char;
            Some(Key::Unicode(ch))
        }
        // Function keys
        112 => Some(Key::F1),
        113 => Some(Key::F2),
        114 => Some(Key::F3),
        115 => Some(Key::F4),
        116 => Some(Key::F5),
        117 => Some(Key::F6),
        118 => Some(Key::F7),
        119 => Some(Key::F8),
        120 => Some(Key::F9),
        121 => Some(Key::F10),
        122 => Some(Key::F11),
        123 => Some(Key::F12),
        // Special keys
        8 => Some(Key::Backspace),
        9 => Some(Key::Tab),
        13 => Some(Key::Return),
        16 => Some(Key::Shift),
        17 => Some(Key::Control),
        18 => Some(Key::Alt),
        19 => None, // Pause — not directly available in enigo
        20 => Some(Key::CapsLock),
        27 => Some(Key::Escape),
        32 => Some(Key::Space),
        33 => Some(Key::PageUp),
        34 => Some(Key::PageDown),
        35 => Some(Key::End),
        36 => Some(Key::Home),
        37 => Some(Key::LeftArrow),
        38 => Some(Key::UpArrow),
        39 => Some(Key::RightArrow),
        40 => Some(Key::DownArrow),
        45 => None, // Insert — not directly available in enigo
        46 => Some(Key::Delete),
        91 => Some(Key::Meta), // Windows/Super key
        // Numpad
        96 => Some(Key::Unicode('0')),
        97 => Some(Key::Unicode('1')),
        98 => Some(Key::Unicode('2')),
        99 => Some(Key::Unicode('3')),
        100 => Some(Key::Unicode('4')),
        101 => Some(Key::Unicode('5')),
        102 => Some(Key::Unicode('6')),
        103 => Some(Key::Unicode('7')),
        104 => Some(Key::Unicode('8')),
        105 => Some(Key::Unicode('9')),
        106 => Some(Key::Unicode('*')),
        107 => Some(Key::Unicode('+')),
        109 => Some(Key::Unicode('-')),
        110 => Some(Key::Unicode('.')),
        111 => Some(Key::Unicode('/')),
        144 => None, // NumLock — not directly available in enigo
        145 => None, // ScrollLock — not directly available in enigo
        93 => None,  // ContextMenu — not directly available in enigo
        44 => None,  // PrintScreen — not directly available in enigo
        // Punctuation
        186 => Some(Key::Unicode(';')),
        187 => Some(Key::Unicode('=')),
        188 => Some(Key::Unicode(',')),
        189 => Some(Key::Unicode('-')),
        190 => Some(Key::Unicode('.')),
        191 => Some(Key::Unicode('/')),
        192 => Some(Key::Unicode('`')),
        219 => Some(Key::Unicode('[')),
        220 => Some(Key::Unicode('\\')),
        221 => Some(Key::Unicode(']')),
        222 => Some(Key::Unicode('\'')),
        _ => {
            tracing::debug!("Unmapped key code: {}", code);
            None
        }
    }
}

// ─── Win32 Backend (Windows native SendInput) ──────────────────────────

#[cfg(target_os = "windows")]
mod win32_ffi {
    //! Raw FFI bindings to user32.dll for input injection.
    //! Using raw FFI instead of the large `windows` crate to keep the binary small.

    use std::os::raw::c_int;

    // --- Constants ---
    pub const INPUT_MOUSE: u32 = 0;
    pub const INPUT_KEYBOARD: u32 = 1;

    // Mouse event flags
    pub const MOUSEEVENTF_MOVE: u32 = 0x0001;
    pub const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    pub const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    pub const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
    pub const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
    pub const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
    pub const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;
    pub const MOUSEEVENTF_WHEEL: u32 = 0x0800;
    pub const MOUSEEVENTF_HWHEEL: u32 = 0x1000;
    pub const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;
    pub const MOUSEEVENTF_VIRTUALDESKTOP: u32 = 0x4000;

    // Keyboard event flags
    pub const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
    pub const KEYEVENTF_KEYUP: u32 = 0x0002;

    // MapVirtualKey translation type
    pub const MAPVK_VK_TO_VSC: u32 = 0;

    pub const WHEEL_DELTA: i32 = 120;

    // --- Structures ---
    #[repr(C)]
    pub struct MOUSEINPUT {
        pub dx: i32,
        pub dy: i32,
        pub mouse_data: u32,
        pub dw_flags: u32,
        pub time: u32,
        pub dw_extra_info: usize,
    }

    #[repr(C)]
    pub struct KEYBDINPUT {
        pub w_vk: u16,
        pub w_scan: u16,
        pub dw_flags: u32,
        pub time: u32,
        pub dw_extra_info: usize,
    }

    /// `INPUT` struct — we use a union-style approach via `InputUnion`.
    #[repr(C)]
    pub struct INPUT {
        pub input_type: u32,
        pub data: InputUnion,
    }

    /// Union of the possible input types. We use the largest (MOUSEINPUT).
    #[repr(C)]
    pub union InputUnion {
        pub mi: std::mem::ManuallyDrop<MOUSEINPUT>,
        pub ki: std::mem::ManuallyDrop<KEYBDINPUT>,
    }

    extern "system" {
        pub fn SendInput(c_inputs: u32, p_inputs: *const INPUT, cb_size: c_int) -> u32;
        pub fn GetSystemMetrics(n_index: c_int) -> c_int;
        pub fn MapVirtualKeyW(u_code: u32, u_map_type: u32) -> u32;
        pub fn OpenDesktopW(
            lpsz_desktop: *const u16,
            dw_flags: u32,
            f_inherit: i32,
            dw_desired_access: u32,
        ) -> *mut std::ffi::c_void;
        pub fn SetThreadDesktop(h_desktop: *mut std::ffi::c_void) -> i32;
        pub fn CloseDesktop(h_desktop: *mut std::ffi::c_void) -> i32;
        pub fn GetThreadDesktop(dw_thread_id: u32) -> *mut std::ffi::c_void;
        pub fn GetCurrentThreadId() -> u32;
    }

    /// Desktop access rights for OpenDesktop
    pub const DESKTOP_SWITCHDESKTOP: u32 = 0x0100;
    pub const GENERIC_ALL: u32 = 0x10000000;

    pub const SM_CXSCREEN: c_int = 0;
    pub const SM_CYSCREEN: c_int = 1;
    pub const SM_XVIRTUALSCREEN: c_int = 76;
    pub const SM_YVIRTUALSCREEN: c_int = 77;
    pub const SM_CXVIRTUALSCREEN: c_int = 78;
    pub const SM_CYVIRTUALSCREEN: c_int = 79;

    /// Extra info value to tag our injected events (prevents feedback loops)
    pub const SC_INPUT_EXTRA: usize = 100;
}

#[cfg(target_os = "windows")]
pub struct Win32InputInjector {
    pub geometry: MonitorGeometry,
}

#[cfg(target_os = "windows")]
impl Win32InputInjector {
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        tracing::info!(
            screen_width,
            screen_height,
            "Creating Win32InputInjector (SendInput)"
        );
        Self {
            geometry: MonitorGeometry::single(screen_width, screen_height),
        }
    }

    pub fn handle_event(&mut self, event: &sc_protocol::InputEvent) {
        // Phase 3: Try to switch to the active desktop (UAC/login screen support)
        self.try_change_desktop();

        match &event.event {
            Some(input_event::Event::MouseMove(mv)) => {
                self.send_mouse_move(mv.x, mv.y);
            }
            Some(input_event::Event::MouseButton(btn)) => {
                self.send_mouse_move(btn.x, btn.y);
                self.send_mouse_button(btn.button, btn.pressed);
            }
            Some(input_event::Event::MouseScroll(scroll)) => {
                if scroll.x > 0.0 || scroll.y > 0.0 {
                    self.send_mouse_move(scroll.x, scroll.y);
                }
                self.send_mouse_scroll(scroll.delta_x, scroll.delta_y);
            }
            Some(input_event::Event::KeyEvent(key)) => {
                if let Some(vk) = map_web_keycode_to_win32_vk(key.key_code) {
                    self.send_key(vk, key.pressed);
                }
            }
            Some(input_event::Event::RelativeMouseMove(rel)) => {
                self.send_relative_mouse_move(rel.dx, rel.dy);
            }
            None => {}
        }
    }

    fn send_relative_mouse_move(&self, dx: i32, dy: i32) {
        #[cfg(target_os = "windows")]
        {
            let input = win32_ffi::INPUT {
                input_type: win32_ffi::INPUT_MOUSE,
                data: win32_ffi::InputUnion {
                    mi: std::mem::ManuallyDrop::new(win32_ffi::MOUSEINPUT {
                        dx,
                        dy,
                        mouse_data: 0,
                        dw_flags: win32_ffi::MOUSEEVENTF_MOVE,
                        time: 0,
                        dw_extra_info: win32_ffi::SC_INPUT_EXTRA,
                    }),
                },
            };

            unsafe {
                win32_ffi::SendInput(1, &input, std::mem::size_of::<win32_ffi::INPUT>() as i32);
            }
        }
    }

    fn send_mouse_move(&self, norm_x: f64, norm_y: f64) {
        // Fix 1d: Use virtual desktop metrics for multi-monitor support
        let (virt_x, virt_y, virt_w, virt_h) = unsafe {
            (
                win32_ffi::GetSystemMetrics(win32_ffi::SM_XVIRTUALSCREEN),
                win32_ffi::GetSystemMetrics(win32_ffi::SM_YVIRTUALSCREEN),
                win32_ffi::GetSystemMetrics(win32_ffi::SM_CXVIRTUALSCREEN),
                win32_ffi::GetSystemMetrics(win32_ffi::SM_CYVIRTUALSCREEN),
            )
        };

        // Map normalized [0,1] coords to virtual desktop absolute [0,65535]
        let pixel_x = (norm_x * self.geometry.total_width as f64) as i32;
        let pixel_y = (norm_y * self.geometry.total_height as f64) as i32;
        let abs_x = if virt_w > 0 {
            ((pixel_x - virt_x) * 65535) / virt_w
        } else {
            (norm_x * 65535.0) as i32
        };
        let abs_y = if virt_h > 0 {
            ((pixel_y - virt_y) * 65535) / virt_h
        } else {
            (norm_y * 65535.0) as i32
        };

        let input = win32_ffi::INPUT {
            input_type: win32_ffi::INPUT_MOUSE,
            data: win32_ffi::InputUnion {
                mi: std::mem::ManuallyDrop::new(win32_ffi::MOUSEINPUT {
                    dx: abs_x,
                    dy: abs_y,
                    mouse_data: 0,
                    dw_flags: win32_ffi::MOUSEEVENTF_MOVE
                        | win32_ffi::MOUSEEVENTF_ABSOLUTE
                        | win32_ffi::MOUSEEVENTF_VIRTUALDESKTOP,
                    time: 0,
                    dw_extra_info: win32_ffi::SC_INPUT_EXTRA,
                }),
            },
        };

        unsafe {
            win32_ffi::SendInput(1, &input, std::mem::size_of::<win32_ffi::INPUT>() as i32);
        }
    }

    fn send_mouse_button(&self, button: u32, pressed: bool) {
        let flags = match (button, pressed) {
            (0, true) => win32_ffi::MOUSEEVENTF_LEFTDOWN,
            (0, false) => win32_ffi::MOUSEEVENTF_LEFTUP,
            (1, true) => win32_ffi::MOUSEEVENTF_MIDDLEDOWN,
            (1, false) => win32_ffi::MOUSEEVENTF_MIDDLEUP,
            (2, true) => win32_ffi::MOUSEEVENTF_RIGHTDOWN,
            (2, false) => win32_ffi::MOUSEEVENTF_RIGHTUP,
            _ => win32_ffi::MOUSEEVENTF_LEFTDOWN,
        };

        let input = win32_ffi::INPUT {
            input_type: win32_ffi::INPUT_MOUSE,
            data: win32_ffi::InputUnion {
                mi: std::mem::ManuallyDrop::new(win32_ffi::MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouse_data: 0,
                    dw_flags: flags,
                    time: 0,
                    dw_extra_info: win32_ffi::SC_INPUT_EXTRA,
                }),
            },
        };

        unsafe {
            win32_ffi::SendInput(1, &input, std::mem::size_of::<win32_ffi::INPUT>() as i32);
        }
    }

    fn send_mouse_scroll(&self, delta_x: f64, delta_y: f64) {
        // Vertical scroll
        if delta_y.abs() > 0.01 {
            let amount = (-delta_y * win32_ffi::WHEEL_DELTA as f64) as i32;
            let input = win32_ffi::INPUT {
                input_type: win32_ffi::INPUT_MOUSE,
                data: win32_ffi::InputUnion {
                    mi: std::mem::ManuallyDrop::new(win32_ffi::MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouse_data: amount as u32,
                        dw_flags: win32_ffi::MOUSEEVENTF_WHEEL,
                        time: 0,
                        dw_extra_info: win32_ffi::SC_INPUT_EXTRA,
                    }),
                },
            };
            unsafe {
                win32_ffi::SendInput(1, &input, std::mem::size_of::<win32_ffi::INPUT>() as i32);
            }
        }

        // Horizontal scroll
        if delta_x.abs() > 0.01 {
            let amount = (delta_x * win32_ffi::WHEEL_DELTA as f64) as i32;
            let input = win32_ffi::INPUT {
                input_type: win32_ffi::INPUT_MOUSE,
                data: win32_ffi::InputUnion {
                    mi: std::mem::ManuallyDrop::new(win32_ffi::MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouse_data: amount as u32,
                        dw_flags: win32_ffi::MOUSEEVENTF_HWHEEL,
                        time: 0,
                        dw_extra_info: win32_ffi::SC_INPUT_EXTRA,
                    }),
                },
            };
            unsafe {
                win32_ffi::SendInput(1, &input, std::mem::size_of::<win32_ffi::INPUT>() as i32);
            }
        }
    }

    fn send_key(&self, vk: u16, pressed: bool) {
        // Fix 1e: Derive scan code from VK code
        let scan =
            unsafe { win32_ffi::MapVirtualKeyW(vk as u32, win32_ffi::MAPVK_VK_TO_VSC) } as u16;

        let mut flags = if pressed {
            0
        } else {
            win32_ffi::KEYEVENTF_KEYUP
        };

        // Fix 1e: Detect extended keys (scan codes with 0xE0 prefix)
        // Extended keys include: Insert, Delete, Home, End, PageUp, PageDown,
        // Arrow keys, Numlock, Break, PrintScreen, Divide, Enter (numpad),
        // Right-hand Ctrl and Alt, and Windows keys
        let is_extended = matches!(
            vk,
            0x21..=0x28  // PageUp, PageDown, End, Home, Arrows
            | 0x2C..=0x2E  // PrintScreen, Insert, Delete
            | 0x5B | 0x5C | 0x5D  // LWin, RWin, Apps
            | 0xA3 | 0xA5  // RControl, RMenu
            | 0x6F  // VK_DIVIDE (numpad /)
            | 0x90  // VK_NUMLOCK
        );
        if is_extended {
            flags |= win32_ffi::KEYEVENTF_EXTENDEDKEY;
        }

        let input = win32_ffi::INPUT {
            input_type: win32_ffi::INPUT_KEYBOARD,
            data: win32_ffi::InputUnion {
                ki: std::mem::ManuallyDrop::new(win32_ffi::KEYBDINPUT {
                    w_vk: vk,
                    w_scan: scan,
                    dw_flags: flags,
                    time: 0,
                    dw_extra_info: win32_ffi::SC_INPUT_EXTRA,
                }),
            },
        };

        unsafe {
            win32_ffi::SendInput(1, &input, std::mem::size_of::<win32_ffi::INPUT>() as i32);
        }
    }

    /// Release all modifier keys to prevent stuck keys on session end.
    pub fn release_all_modifiers(&mut self) {
        tracing::info!("Releasing all modifier keys (Win32)");
        // VK codes for all modifier keys
        let modifiers: &[u16] = &[
            0x10, 0xA0, 0xA1, // VK_SHIFT, VK_LSHIFT, VK_RSHIFT
            0x11, 0xA2, 0xA3, // VK_CONTROL, VK_LCONTROL, VK_RCONTROL
            0x12, 0xA4, 0xA5, // VK_MENU (Alt), VK_LMENU, VK_RMENU
            0x5B, 0x5C, // VK_LWIN, VK_RWIN
        ];
        for &vk in modifiers {
            self.send_key(vk, false);
        }
    }

    /// Attempt to switch to the currently active desktop.
    /// This is needed for UAC prompts and login screens on Windows,
    /// which run on a separate "Winlogon" desktop. Without this,
    /// SendInput events won't reach the secure desktop.
    fn try_change_desktop(&self) {
        unsafe {
            // Try "Winlogon" desktop first (UAC / login screen)
            let winlogon_name: Vec<u16> = "Winlogon\0".encode_utf16().collect();
            let h_desk = win32_ffi::OpenDesktopW(
                winlogon_name.as_ptr(),
                0,
                0, // fInherit = FALSE
                win32_ffi::GENERIC_ALL,
            );
            if !h_desk.is_null() {
                let result = win32_ffi::SetThreadDesktop(h_desk);
                if result != 0 {
                    tracing::trace!("Switched to Winlogon desktop");
                }
                win32_ffi::CloseDesktop(h_desk);
                if result != 0 {
                    return; // Successfully switched
                }
            }

            // Fall back to "Default" desktop
            let default_name: Vec<u16> = "Default\0".encode_utf16().collect();
            let h_desk =
                win32_ffi::OpenDesktopW(default_name.as_ptr(), 0, 0, win32_ffi::GENERIC_ALL);
            if !h_desk.is_null() {
                let result = win32_ffi::SetThreadDesktop(h_desk);
                if result != 0 {
                    tracing::trace!("Switched to Default desktop");
                }
                win32_ffi::CloseDesktop(h_desk);
            }
        }
    }
}

/// Map web KeyboardEvent.keyCode to Windows Virtual-Key code.
#[cfg(target_os = "windows")]
fn map_web_keycode_to_win32_vk(web_code: u32) -> Option<u16> {
    // Most web keyCodes align directly with Windows VK codes for alphanumeric keys
    let vk: u16 = match web_code {
        // Letters A-Z: web keyCode 65-90 == VK_A-VK_Z
        65..=90 => web_code as u16,
        // Digits 0-9: web keyCode 48-57 == VK_0-VK_9
        48..=57 => web_code as u16,
        // Numpad 0-9: web keyCode 96-105 == VK_NUMPAD0-VK_NUMPAD9
        96..=105 => web_code as u16,
        // Function keys: web keyCode 112-123 == VK_F1-VK_F12
        112..=123 => web_code as u16,
        // Special keys (direct VK mappings)
        8 => 0x08,  // VK_BACK
        9 => 0x09,  // VK_TAB
        13 => 0x0D, // VK_RETURN
        16 => 0x10, // VK_SHIFT
        17 => 0x11, // VK_CONTROL
        18 => 0x12, // VK_MENU (Alt)
        19 => 0x13, // VK_PAUSE
        20 => 0x14, // VK_CAPITAL (CapsLock)
        27 => 0x1B, // VK_ESCAPE
        32 => 0x20, // VK_SPACE
        33 => 0x21, // VK_PRIOR (PageUp)
        34 => 0x22, // VK_NEXT (PageDown)
        35 => 0x23, // VK_END
        36 => 0x24, // VK_HOME
        37 => 0x25, // VK_LEFT
        38 => 0x26, // VK_UP
        39 => 0x27, // VK_RIGHT
        40 => 0x28, // VK_DOWN
        44 => 0x2C, // VK_SNAPSHOT (PrintScreen)
        45 => 0x2D, // VK_INSERT
        46 => 0x2E, // VK_DELETE
        91 => 0x5B, // VK_LWIN (Meta)
        93 => 0x5D, // VK_APPS (ContextMenu)
        // Numpad operators
        106 => 0x6A, // VK_MULTIPLY
        107 => 0x6B, // VK_ADD
        109 => 0x6D, // VK_SUBTRACT
        110 => 0x6E, // VK_DECIMAL
        111 => 0x6F, // VK_DIVIDE
        // Lock keys
        144 => 0x90, // VK_NUMLOCK
        145 => 0x91, // VK_SCROLL
        // Punctuation (OEM keys)
        186 => 0xBA, // VK_OEM_1 (;:)
        187 => 0xBB, // VK_OEM_PLUS (=+)
        188 => 0xBC, // VK_OEM_COMMA (,<)
        189 => 0xBD, // VK_OEM_MINUS (-_)
        190 => 0xBE, // VK_OEM_PERIOD (.>)
        191 => 0xBF, // VK_OEM_2 (/?)
        192 => 0xC0, // VK_OEM_3 (`~)
        219 => 0xDB, // VK_OEM_4 ([{)
        220 => 0xDC, // VK_OEM_5 (\|)
        221 => 0xDD, // VK_OEM_6 (]})
        222 => 0xDE, // VK_OEM_7 ('")
        _ => {
            tracing::debug!("Unmapped web keycode for Win32: {}", web_code);
            return None;
        }
    };
    Some(vk)
}
