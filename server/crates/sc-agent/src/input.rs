//! Input injection for remote desktop sessions.
//!
//! Translates protobuf `InputEvent` messages into native mouse/keyboard
//! actions using the `enigo` crate.

use enigo::{Axis, Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};

use sc_protocol::input_event;

/// Handles input injection for desktop sessions.
pub struct InputInjector {
    enigo: Enigo,
    screen_width: u32,
    screen_height: u32,
}

impl InputInjector {
    pub fn new(screen_width: u32, screen_height: u32) -> anyhow::Result<Self> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| anyhow::anyhow!("Failed to create Enigo instance: {:?}", e))?;

        Ok(Self {
            enigo,
            screen_width,
            screen_height,
        })
    }

    /// Update screen dimensions (e.g. if the monitored display changes).
    #[allow(dead_code)]
    pub fn set_screen_size(&mut self, width: u32, height: u32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// Dispatch a protobuf `InputEvent` to native input simulation.
    pub fn handle_event(&mut self, event: &sc_protocol::InputEvent) {
        match &event.event {
            Some(input_event::Event::MouseMove(mv)) => {
                let x = (mv.x * self.screen_width as f64) as i32;
                let y = (mv.y * self.screen_height as f64) as i32;
                if let Err(e) = self.enigo.move_mouse(x, y, Coordinate::Abs) {
                    tracing::warn!("enigo move_mouse({}, {}) failed: {:?}", x, y, e);
                }
            }
            Some(input_event::Event::MouseButton(btn)) => {
                let x = (btn.x * self.screen_width as f64) as i32;
                let y = (btn.y * self.screen_height as f64) as i32;

                // Move to position first
                if let Err(e) = self.enigo.move_mouse(x, y, Coordinate::Abs) {
                    tracing::warn!("enigo move_mouse({}, {}) failed: {:?}", x, y, e);
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
                    tracing::warn!(
                        "enigo button({:?}, {:?}) failed: {:?}",
                        button,
                        direction,
                        e
                    );
                }
            }
            Some(input_event::Event::MouseScroll(scroll)) => {
                if scroll.delta_y.abs() > 0.01 {
                    let clicks = (scroll.delta_y * 3.0) as i32;
                    if let Err(e) = self.enigo.scroll(clicks, Axis::Vertical) {
                        tracing::warn!("enigo scroll vertical({}) failed: {:?}", clicks, e);
                    }
                }
                if scroll.delta_x.abs() > 0.01 {
                    let clicks = (scroll.delta_x * 3.0) as i32;
                    if let Err(e) = self.enigo.scroll(clicks, Axis::Horizontal) {
                        tracing::warn!("enigo scroll horizontal({}) failed: {:?}", clicks, e);
                    }
                }
            }
            Some(input_event::Event::KeyEvent(key)) => {
                // Map key_code to enigo Key
                // For now, use raw key codes — a full keymap table
                // would be built out as the protocol stabilises.
                if let Some(enigo_key) = map_key_code(key.key_code) {
                    let direction = if key.pressed {
                        Direction::Press
                    } else {
                        Direction::Release
                    };
                    if let Err(e) = self.enigo.key(enigo_key, direction) {
                        tracing::warn!(
                            "enigo key({:?}, {:?}) failed: {:?}",
                            enigo_key,
                            direction,
                            e
                        );
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
