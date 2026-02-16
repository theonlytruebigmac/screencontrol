//! Input injection for remote desktop sessions.
//!
//! On Linux/Wayland (GNOME), uses the Mutter RemoteDesktop D-Bus API to inject
//! input events directly through the compositor — no portal consent dialog, no
//! hardware cursor hijacking.
//!
//! On other platforms (or X11 fallback), uses the `enigo` crate.

use sc_protocol::input_event;

/// Handles input injection for desktop sessions.
/// Supports two backends:
/// - Mutter D-Bus (Linux/Wayland) — zero-overhead, no consent dialog
/// - Enigo (fallback for X11 / macOS / Windows)
pub enum InputInjector {
    #[cfg(target_os = "linux")]
    Mutter(MutterInputInjector),
    Enigo(EnigoInputInjector),
}

impl InputInjector {
    /// Dispatch a protobuf `InputEvent` to native input simulation.
    pub fn handle_event(&mut self, event: &sc_protocol::InputEvent) {
        match self {
            #[cfg(target_os = "linux")]
            InputInjector::Mutter(m) => m.handle_event(event),
            InputInjector::Enigo(e) => e.handle_event(event),
        }
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

// ─── Enigo Backend (fallback for X11 / macOS / Windows) ────────────────

pub struct EnigoInputInjector {
    enigo: enigo::Enigo,
    screen_width: u32,
    screen_height: u32,
}

impl EnigoInputInjector {
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
            None => {}
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
        19 => None, // Pause — not available in enigo 0.2
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
        45 => None, // Insert — not available in enigo 0.2
        46 => Some(Key::Delete),
        91 => Some(Key::Meta), // Windows/Super key
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
