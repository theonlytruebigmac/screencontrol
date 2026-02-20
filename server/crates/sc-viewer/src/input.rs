//! SDL2 input event conversion to protobuf InputEvent messages.
//!
//! Maps SDL2 keyboard/mouse events to the ScreenControl protobuf protocol.

use sc_protocol::proto::{
    input_event, InputEvent, KeyEvent, MouseButton as ProtoMouseButton, MouseMove, MouseScroll,
    RelativeMouseMove,
};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;

/// Convert an SDL2 event to a protobuf InputEvent.
/// Returns None if the event is not an input event we handle.
pub fn sdl_event_to_input(
    event: &Event,
    window_width: u32,
    window_height: u32,
    relative_mouse: bool,
) -> Option<InputEvent> {
    match event {
        Event::MouseMotion {
            x, y, xrel, yrel, ..
        } => {
            if relative_mouse {
                // Send pixel deltas in relative mode
                Some(InputEvent {
                    event: Some(input_event::Event::RelativeMouseMove(RelativeMouseMove {
                        dx: *xrel,
                        dy: *yrel,
                    })),
                })
            } else {
                // Normalize coordinates to 0.0..1.0
                let nx = *x as f64 / window_width as f64;
                let ny = *y as f64 / window_height as f64;
                Some(InputEvent {
                    event: Some(input_event::Event::MouseMove(MouseMove { x: nx, y: ny })),
                })
            }
        }

        Event::MouseButtonDown {
            mouse_btn, x, y, ..
        } => {
            let button = sdl_button_to_proto(mouse_btn);
            let nx = *x as f64 / window_width as f64;
            let ny = *y as f64 / window_height as f64;

            Some(InputEvent {
                event: Some(input_event::Event::MouseButton(ProtoMouseButton {
                    button,
                    pressed: true,
                    x: nx,
                    y: ny,
                })),
            })
        }

        Event::MouseButtonUp {
            mouse_btn, x, y, ..
        } => {
            let button = sdl_button_to_proto(mouse_btn);
            let nx = *x as f64 / window_width as f64;
            let ny = *y as f64 / window_height as f64;

            Some(InputEvent {
                event: Some(input_event::Event::MouseButton(ProtoMouseButton {
                    button,
                    pressed: false,
                    x: nx,
                    y: ny,
                })),
            })
        }

        Event::MouseWheel { x, y, .. } => {
            Some(InputEvent {
                event: Some(input_event::Event::MouseScroll(MouseScroll {
                    delta_x: *x as f64,
                    delta_y: *y as f64,
                    x: 0.0, // scroll events don't always have position
                    y: 0.0,
                })),
            })
        }

        Event::KeyDown {
            keycode: Some(key),
            keymod,
            ..
        } => Some(InputEvent {
            event: Some(input_event::Event::KeyEvent(KeyEvent {
                key_code: sdl_keycode_to_web(*key),
                pressed: true,
                ctrl: keymod.contains(sdl2::keyboard::Mod::LCTRLMOD)
                    || keymod.contains(sdl2::keyboard::Mod::RCTRLMOD),
                alt: keymod.contains(sdl2::keyboard::Mod::LALTMOD)
                    || keymod.contains(sdl2::keyboard::Mod::RALTMOD),
                shift: keymod.contains(sdl2::keyboard::Mod::LSHIFTMOD)
                    || keymod.contains(sdl2::keyboard::Mod::RSHIFTMOD),
                meta: keymod.contains(sdl2::keyboard::Mod::LGUIMOD)
                    || keymod.contains(sdl2::keyboard::Mod::RGUIMOD),
            })),
        }),

        Event::KeyUp {
            keycode: Some(key),
            keymod,
            ..
        } => Some(InputEvent {
            event: Some(input_event::Event::KeyEvent(KeyEvent {
                key_code: sdl_keycode_to_web(*key),
                pressed: false,
                ctrl: keymod.contains(sdl2::keyboard::Mod::LCTRLMOD)
                    || keymod.contains(sdl2::keyboard::Mod::RCTRLMOD),
                alt: keymod.contains(sdl2::keyboard::Mod::LALTMOD)
                    || keymod.contains(sdl2::keyboard::Mod::RALTMOD),
                shift: keymod.contains(sdl2::keyboard::Mod::LSHIFTMOD)
                    || keymod.contains(sdl2::keyboard::Mod::RSHIFTMOD),
                meta: keymod.contains(sdl2::keyboard::Mod::LGUIMOD)
                    || keymod.contains(sdl2::keyboard::Mod::RGUIMOD),
            })),
        }),

        _ => None,
    }
}

/// Map SDL2 mouse button to protocol button index
fn sdl_button_to_proto(btn: &MouseButton) -> u32 {
    match btn {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::X1 => 3,
        MouseButton::X2 => 4,
        _ => 0,
    }
}

/// Map SDL2 keycode to web KeyboardEvent.keyCode equivalent.
/// The agent already expects web-style keycodes.
fn sdl_keycode_to_web(key: Keycode) -> u32 {
    match key {
        // Letters A-Z → 65-90
        Keycode::A => 65,
        Keycode::B => 66,
        Keycode::C => 67,
        Keycode::D => 68,
        Keycode::E => 69,
        Keycode::F => 70,
        Keycode::G => 71,
        Keycode::H => 72,
        Keycode::I => 73,
        Keycode::J => 74,
        Keycode::K => 75,
        Keycode::L => 76,
        Keycode::M => 77,
        Keycode::N => 78,
        Keycode::O => 79,
        Keycode::P => 80,
        Keycode::Q => 81,
        Keycode::R => 82,
        Keycode::S => 83,
        Keycode::T => 84,
        Keycode::U => 85,
        Keycode::V => 86,
        Keycode::W => 87,
        Keycode::X => 88,
        Keycode::Y => 89,
        Keycode::Z => 90,

        // Numbers 0-9 → 48-57
        Keycode::Num0 => 48,
        Keycode::Num1 => 49,
        Keycode::Num2 => 50,
        Keycode::Num3 => 51,
        Keycode::Num4 => 52,
        Keycode::Num5 => 53,
        Keycode::Num6 => 54,
        Keycode::Num7 => 55,
        Keycode::Num8 => 56,
        Keycode::Num9 => 57,

        // Function keys F1-F12 → 112-123
        Keycode::F1 => 112,
        Keycode::F2 => 113,
        Keycode::F3 => 114,
        Keycode::F4 => 115,
        Keycode::F5 => 116,
        Keycode::F6 => 117,
        Keycode::F7 => 118,
        Keycode::F8 => 119,
        Keycode::F9 => 120,
        Keycode::F10 => 121,
        Keycode::F11 => 122,
        Keycode::F12 => 123,

        // Special keys
        Keycode::Return => 13,
        Keycode::Escape => 27,
        Keycode::Backspace => 8,
        Keycode::Tab => 9,
        Keycode::Space => 32,
        Keycode::Delete => 46,
        Keycode::Insert => 45,
        Keycode::Home => 36,
        Keycode::End => 35,
        Keycode::PageUp => 33,
        Keycode::PageDown => 34,

        // Arrow keys
        Keycode::Left => 37,
        Keycode::Up => 38,
        Keycode::Right => 39,
        Keycode::Down => 40,

        // Modifiers
        Keycode::LShift | Keycode::RShift => 16,
        Keycode::LCtrl | Keycode::RCtrl => 17,
        Keycode::LAlt | Keycode::RAlt => 18,
        Keycode::LGui | Keycode::RGui => 91,
        Keycode::CapsLock => 20,
        Keycode::NumLockClear => 144,

        // Punctuation
        Keycode::Minus => 189,
        Keycode::Equals => 187,
        Keycode::LeftBracket => 219,
        Keycode::RightBracket => 221,
        Keycode::Backslash => 220,
        Keycode::Semicolon => 186,
        Keycode::Quote => 222,
        Keycode::Backquote => 192,
        Keycode::Comma => 188,
        Keycode::Period => 190,
        Keycode::Slash => 191,

        // Numpad
        Keycode::Kp0 => 96,
        Keycode::Kp1 => 97,
        Keycode::Kp2 => 98,
        Keycode::Kp3 => 99,
        Keycode::Kp4 => 100,
        Keycode::Kp5 => 101,
        Keycode::Kp6 => 102,
        Keycode::Kp7 => 103,
        Keycode::Kp8 => 104,
        Keycode::Kp9 => 105,
        Keycode::KpMultiply => 106,
        Keycode::KpPlus => 107,
        Keycode::KpMinus => 109,
        Keycode::KpDecimal => 110,
        Keycode::KpDivide => 111,
        Keycode::KpEnter => 13,

        // Print Screen, Scroll Lock, Pause
        Keycode::PrintScreen => 44,
        Keycode::ScrollLock => 145,
        Keycode::Pause => 19,

        // Default: use the raw SDL scancode value
        _ => 0, // Unknown key — drop it
    }
}
