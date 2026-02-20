//! ScreenControl Native Viewer
//!
//! Toolbar modeled after ConnectWise ScreenConnect:
//!   - Thin row of small square icon buttons at top
//!   - Clicking an icon opens a tile-grid panel overlay
//!   - Mouse input gated: toolbar/panel clicks stay local

mod audio;
mod clipboard;
mod connection;
mod decoder;
mod input;
mod session;

use anyhow::{Context, Result};
use clap::Parser;
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Mod};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::{error, info};

use connection::{ConnectionState, IncomingMessage, OutgoingMessage};
use sc_protocol::proto::{input_event, InputEvent, KeyEvent};

const YELLOW: Color = Color::RGB(0xf0, 0xc0, 0x30);
const RED: Color = Color::RGB(0xe0, 0x52, 0x46);

// ─── Colors ─────────────────────────────────────────────────────
const BG: Color = Color::RGB(0x0d, 0x0d, 0x0d);
const TB_BG: Color = Color::RGB(0x18, 0x18, 0x18);
const TB_BORDER: Color = Color::RGB(0x2a, 0x2a, 0x2a);
const ICON_BG: Color = Color::RGB(0x22, 0x22, 0x22);
const ICON_HOVER: Color = Color::RGB(0x33, 0x33, 0x33);
const ICON_ACTIVE: Color = Color::RGB(0x3a, 0x8f, 0xd6); // blue highlight for active icon
const PANEL_BG: Color = Color::RGB(0x1a, 0x1a, 0x1a);
const PANEL_BORDER: Color = Color::RGB(0x3a, 0x3a, 0x3a);
const TILE_BG: Color = Color::RGB(0x24, 0x24, 0x24);
const TILE_HOVER: Color = Color::RGB(0x3a, 0x3a, 0x3a);
const TILE_ACTIVE: Color = Color::RGB(0x3a, 0x8f, 0xd6);
const TEXT_DIM: Color = Color::RGB(0x88, 0x88, 0x88);
const TEXT_NORM: Color = Color::RGB(0xbb, 0xbb, 0xbb);
const TEXT_HI: Color = Color::RGB(0xee, 0xee, 0xee);
const SECTION_HD: Color = Color::RGB(0x99, 0x99, 0x99);
const GREEN: Color = Color::RGB(0x34, 0xd3, 0x99);
const _ACCENT: Color = Color::RGB(0xe0, 0x52, 0x46);

// ─── Layout ─────────────────────────────────────────────────────
const TB_H: i32 = 26; // toolbar bar height
const SB_H: i32 = 20; // status bar height
const ICON_SZ: i32 = 22; // icon button square size
const ICON_Y: i32 = 2; // icon button y offset
const ICON_GAP: i32 = 2; // gap between icon buttons
const ICON_X_START: i32 = 4; // first icon x offset

const TILE_W: i32 = 90; // tile width
const TILE_H: i32 = 70; // tile height
const TILE_GAP: i32 = 6; // gap between tiles
const TILE_PAD: i32 = 10; // padding inside panel
const SECTION_H: i32 = 20; // section header height

// ─── Quality ────────────────────────────────────────────────────
const QUALITY_PRESETS: &[(&str, u32, u32, u32)] = &[
    ("Low", 25, 15, 1500),
    ("Medium", 50, 24, 3000),
    ("High", 75, 30, 5000),
    ("Ultra", 95, 30, 8000),
];

// ─── Special Keys ───────────────────────────────────────────────
struct SpecialKey {
    label: &'static str,
    symbol: &'static str,
    keys: &'static [(u32, &'static str)],
}

const SPECIAL_KEYS: &[SpecialKey] = &[
    SpecialKey {
        label: "Ctrl+Alt\n+Del",
        symbol: "CAD",
        keys: &[(17, "ControlLeft"), (18, "AltLeft"), (46, "Delete")],
    },
    SpecialKey {
        label: "Alt+Tab",
        symbol: "Tab",
        keys: &[(18, "AltLeft"), (9, "Tab")],
    },
    SpecialKey {
        label: "Alt+F4",
        symbol: "x",
        keys: &[(18, "AltLeft"), (115, "F4")],
    },
    SpecialKey {
        label: "Win Key",
        symbol: "Win",
        keys: &[(91, "MetaLeft")],
    },
    SpecialKey {
        label: "Task\nMgr",
        symbol: "Tsk",
        keys: &[(17, "ControlLeft"), (16, "ShiftLeft"), (27, "Escape")],
    },
];

// ─── Toolbar Icon Definitions ───────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq)]
enum Panel {
    None,
    View,
    Commands,
}

struct ToolbarIcon {
    symbol: &'static str,
    panel: Panel,
    _tooltip: &'static str,
}

const TOOLBAR_ICONS: &[ToolbarIcon] = &[
    ToolbarIcon {
        symbol: "V",
        panel: Panel::View,
        _tooltip: "View",
    },
    ToolbarIcon {
        symbol: "C",
        panel: Panel::Commands,
        _tooltip: "Commands",
    },
];

// ─── CLI ────────────────────────────────────────────────────────
#[derive(Parser, Debug)]
#[command(name = "sc-viewer", about = "ScreenControl native desktop viewer")]
struct Args {
    #[arg(long)]
    server: String,
    #[arg(long)]
    session: Option<String>,
    #[arg(long)]
    token: Option<String>,
    #[arg(long)]
    agent: Option<String>,
    #[arg(long)]
    email: Option<String>,
    #[arg(long)]
    password: Option<String>,
}

// ─── Viewer State ───────────────────────────────────────────────

struct ViewerState {
    open_panel: Panel,
    quality_preset: usize,
    resolution: (u32, u32),
    fps: u32,
    latency_ms: u64,
    is_fullscreen: bool,
    monitors: Vec<connection::MonitorInfo>,
    active_monitor: usize,
    hover_tile: Option<(usize, usize)>, // (section_idx, tile_idx)
    mouse_x: i32,
    mouse_y: i32,
    // Remote cursor state
    cursor_shapes: HashMap<u64, CachedCursor>,
    cursor_pos: Option<(f64, f64, u64, bool)>, // (x, y, cursor_id, visible)
    relative_mouse: bool,
    audio_muted: bool,
    connection_state: ConnectionState,
    codec_name: &'static str,
    toast: Option<(String, Instant)>,
    auto_quality: bool,
}

/// Cached remote cursor shape data.
struct CachedCursor {
    width: u32,
    height: u32,
    hotspot_x: i32,
    hotspot_y: i32,
    rgba_data: Vec<u8>,
}

impl ViewerState {
    fn new() -> Self {
        Self {
            open_panel: Panel::None,
            quality_preset: 3,
            resolution: (0, 0),
            fps: 0,
            latency_ms: 0,
            is_fullscreen: false,
            monitors: Vec::new(),
            active_monitor: 0,
            hover_tile: None,
            mouse_x: 0,
            mouse_y: 0,
            cursor_shapes: HashMap::new(),
            cursor_pos: None,
            relative_mouse: false,
            audio_muted: false,
            connection_state: ConnectionState::Connected,
            codec_name: "—",
            toast: None,
            auto_quality: true,
        }
    }

    fn quality_label(&self) -> &str {
        QUALITY_PRESETS[self.quality_preset].0
    }

    fn toggle_panel(&mut self, panel: Panel) {
        if self.open_panel == panel {
            self.open_panel = Panel::None;
        } else {
            self.open_panel = panel;
        }
        self.hover_tile = None;
    }
}

// ─── Tile definition for panel contents ─────────────────────────

struct TileSection {
    header: &'static str,
    tiles: Vec<Tile>,
}

struct Tile {
    symbol: String,
    label: String,
    action: TileAction,
    is_active: bool,
}

enum TileAction {
    SetQuality(usize),
    SelectMonitor(usize),
    Fullscreen,
    SendKeys(usize), // index into SPECIAL_KEYS
    Clipboard,
    Screenshot,
    ToggleRelativeMouse,
    ToggleAudioMute,
    ToggleAutoQuality,
}

fn build_view_sections(st: &ViewerState) -> Vec<TileSection> {
    let mut secs = Vec::new();

    // Quality section
    let quality_tiles: Vec<Tile> = QUALITY_PRESETS
        .iter()
        .enumerate()
        .map(|(i, (label, _, _, _))| Tile {
            symbol: label.chars().next().unwrap_or('?').to_string(),
            label: label.to_string(),
            action: TileAction::SetQuality(i),
            is_active: st.quality_preset == i,
        })
        .collect();
    // Add "Auto" toggle tile
    let mut quality_tiles = quality_tiles;
    quality_tiles.push(Tile {
        symbol: "A".to_string(),
        label: "Auto".to_string(),
        action: TileAction::ToggleAutoQuality,
        is_active: st.auto_quality,
    });
    secs.push(TileSection {
        header: "Select Quality",
        tiles: quality_tiles,
    });

    // Monitor section
    if !st.monitors.is_empty() {
        let mon_tiles: Vec<Tile> = st
            .monitors
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let name = if m.name.is_empty() {
                    format!("Display {}", i + 1)
                } else {
                    m.name.clone()
                };
                Tile {
                    symbol: format!("{}", i + 1),
                    label: name,
                    action: TileAction::SelectMonitor(i),
                    is_active: st.active_monitor == i,
                }
            })
            .collect();
        secs.push(TileSection {
            header: "Select Monitor",
            tiles: mon_tiles,
        });
    }

    // View controls
    secs.push(TileSection {
        header: "Display",
        tiles: vec![Tile {
            symbol: "F".into(),
            label: "Fullscreen".into(),
            action: TileAction::Fullscreen,
            is_active: st.is_fullscreen,
        }],
    });

    secs
}

fn build_command_sections(st: &ViewerState) -> Vec<TileSection> {
    let mut secs = Vec::new();

    // Essentials
    let key_tiles: Vec<Tile> = SPECIAL_KEYS
        .iter()
        .enumerate()
        .map(|(i, sk)| Tile {
            symbol: sk.symbol.to_string(),
            label: sk.label.replace('\n', " "),
            action: TileAction::SendKeys(i),
            is_active: false,
        })
        .collect();
    secs.push(TileSection {
        header: "Essentials",
        tiles: key_tiles,
    });

    // Tools
    secs.push(TileSection {
        header: "Tools",
        tiles: vec![
            Tile {
                symbol: "Cb".into(),
                label: "Send\nClipboard".into(),
                action: TileAction::Clipboard,
                is_active: false,
            },
            Tile {
                symbol: "Sc".into(),
                label: "Screenshot".into(),
                action: TileAction::Screenshot,
                is_active: false,
            },
            Tile {
                symbol: "Rm".into(),
                label: "Relative\nMouse".into(),
                action: TileAction::ToggleRelativeMouse,
                is_active: st.relative_mouse,
            },
            Tile {
                symbol: if st.audio_muted { "🔇" } else { "🔊" }.into(),
                label: if st.audio_muted {
                    "Unmute\nAudio"
                } else {
                    "Mute\nAudio"
                }
                .into(),
                action: TileAction::ToggleAudioMute,
                is_active: !st.audio_muted,
            },
        ],
    });

    secs
}

// ─── Main ───────────────────────────────────────────────────────

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    info!("ScreenControl Viewer starting...");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Failed to build tokio runtime")?;

    let (session_id, token) = rt.block_on(async { resolve_session(&args).await })?;
    info!("Session: {}", session_id);

    let ws_base = args
        .server
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let (outgoing_tx, mut incoming_rx) =
        rt.block_on(async { connection::connect(&ws_base, &session_id, &token).await })?;

    // ─── SDL2 ───────────────────────────────────────────────
    let sdl_context = sdl2::init().map_err(|e| anyhow::anyhow!("SDL2: {}", e))?;
    let video_sub = sdl_context
        .video()
        .map_err(|e| anyhow::anyhow!("SDL2 video: {}", e))?;
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "best");

    let window = video_sub
        .window("ScreenControl Viewer", 1280, 720)
        .position_centered()
        .resizable()
        .build()
        .context("Failed to create window")?;

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .build()
        .context("Canvas")?;
    let tc = canvas.texture_creator();
    let mut video_tex: Option<sdl2::render::Texture> = None;
    let mut tex_w: u32 = 0;
    let mut tex_h: u32 = 0;
    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // ─── TTF ────────────────────────────────────────────────
    let ttf = sdl2::ttf::init().map_err(|e| anyhow::anyhow!("TTF: {}", e))?;
    let font_paths = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    ];
    let fp = font_paths
        .iter()
        .find(|p| Path::new(p).exists())
        .context("No monospace font")?;
    let font = ttf
        .load_font(fp, 11)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let font_sm = ttf.load_font(fp, 9).map_err(|e| anyhow::anyhow!("{}", e))?;
    let font_lg = ttf
        .load_font(fp, 14)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // ─── Decoder + Clipboard ────────────────────────────────
    let mut decoder = decoder::H264Decoder::new().context("H.264 decoder")?;
    let mut clip_mgr = clipboard::ClipboardManager::new();
    let mut st = ViewerState::new();

    // ─── Audio Player ────────────────────────────────────────
    let audio_sub = sdl_context.audio().ok();
    let mut audio_player = audio_sub
        .as_ref()
        .and_then(|sub| match audio::AudioPlayer::new(sub) {
            Ok(player) => {
                info!("Audio player initialized");
                Some(player)
            }
            Err(e) => {
                tracing::warn!("Audio player not available: {}", e);
                None
            }
        });

    let mut frame_count: u64 = 0;
    let mut fps_timer = Instant::now();
    let mut last_ping = Instant::now();

    send_quality(&outgoing_tx, &rt, st.quality_preset);
    info!("Viewer ready - entering main loop");

    // ─── Main Loop ──────────────────────────────────────────
    'mainloop: loop {
        // Build panel sections for hit testing
        let panel_sections = match st.open_panel {
            Panel::View => build_view_sections(&st),
            Panel::Commands => build_command_sections(&st),
            Panel::None => vec![],
        };
        let _panel_rect = calc_panel_rect(&panel_sections, &st);

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'mainloop,

                Event::MouseMotion { x, y, .. } => {
                    st.mouse_x = x;
                    st.mouse_y = y;

                    // Update hover state for tiles
                    if st.open_panel != Panel::None {
                        st.hover_tile = hit_test_tile(x, y, &panel_sections, &st);
                    }

                    // Only forward to remote if in video area and no panel open
                    if y > TB_H && st.open_panel == Panel::None {
                        forward_input(&event, &canvas, &outgoing_tx, &rt, st.relative_mouse);
                    }
                }

                Event::MouseButtonDown { x, y, .. } => {
                    if y < TB_H {
                        // Toolbar icon clicks
                        handle_icon_click(x, &mut st);
                    } else if st.open_panel != Panel::None {
                        // Panel tile clicks
                        if let Some((sec, tile)) = hit_test_tile(x, y, &panel_sections, &st) {
                            handle_tile_click(
                                &mut st,
                                sec,
                                tile,
                                &panel_sections,
                                &outgoing_tx,
                                &rt,
                                &mut canvas,
                                &mut audio_player,
                            );
                        }
                        st.open_panel = Panel::None;
                    } else {
                        // Forward to remote
                        forward_input(&event, &canvas, &outgoing_tx, &rt, st.relative_mouse);
                    }
                }

                Event::MouseButtonUp { y, .. } => {
                    if y > TB_H && st.open_panel == Panel::None {
                        forward_input(&event, &canvas, &outgoing_tx, &rt, st.relative_mouse);
                    }
                }

                Event::MouseWheel { .. } => {
                    if st.mouse_y > TB_H && st.open_panel == Panel::None {
                        forward_input(&event, &canvas, &outgoing_tx, &rt, st.relative_mouse);
                    }
                }

                Event::KeyDown {
                    keycode: Some(key),
                    keymod,
                    ..
                } => match key {
                    Keycode::Escape if st.open_panel != Panel::None => {
                        st.open_panel = Panel::None;
                    }
                    Keycode::Escape => break 'mainloop,
                    Keycode::F11 => toggle_fullscreen(&mut st, &mut canvas),
                    Keycode::F12 => take_screenshot(&canvas, &mut st),
                    Keycode::Q if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD) => {
                        break 'mainloop
                    }
                    _ => forward_input(&event, &canvas, &outgoing_tx, &rt, st.relative_mouse),
                },

                _ => forward_input(&event, &canvas, &outgoing_tx, &rt, st.relative_mouse),
            }
        }

        // ── Incoming messages ───────────────────────────────
        while let Ok(msg) = incoming_rx.try_recv() {
            match msg {
                IncomingMessage::DesktopFrame {
                    data,
                    codec,
                    is_keyframe,
                    ..
                } => {
                    st.codec_name = if codec == 1 { "H.264" } else { "JPEG" };
                    if codec == 1 {
                        match decoder.decode(&data) {
                            Ok(frames) => {
                                for frame in frames {
                                    if video_tex.is_none()
                                        || tex_w != frame.width
                                        || tex_h != frame.height
                                    {
                                        tex_w = frame.width;
                                        tex_h = frame.height;
                                        st.resolution = (frame.width, frame.height);
                                        video_tex = Some(
                                            tc.create_texture_streaming(
                                                sdl2::pixels::PixelFormatEnum::IYUV,
                                                frame.width,
                                                frame.height,
                                            )
                                            .expect("texture"),
                                        );
                                        info!("Video: {}x{}", frame.width, frame.height);
                                    }
                                    if let Some(ref mut t) = video_tex {
                                        let _ = t.update_yuv(
                                            None,
                                            &frame.y,
                                            frame.y_stride,
                                            &frame.u,
                                            frame.uv_stride,
                                            &frame.v,
                                            frame.uv_stride,
                                        );
                                    }
                                    frame_count += 1;
                                }
                            }
                            Err(e) => {
                                if is_keyframe {
                                    error!("Decode: {}", e);
                                }
                            }
                        }
                    }
                }
                IncomingMessage::Clipboard { text } => clip_mgr.set_remote_content(&text),
                IncomingMessage::Pong { timestamp } => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    st.latency_ms = now.saturating_sub(timestamp);
                }
                IncomingMessage::SessionEnd { reason } => {
                    info!("Session ended: {}", reason);
                    break 'mainloop;
                }
                IncomingMessage::ScreenInfo { monitors, .. } => {
                    info!("Screen info: {} monitors", monitors.len());
                    st.monitors = monitors;
                }
                IncomingMessage::CursorData {
                    cursor_id,
                    width,
                    height,
                    hotspot_x,
                    hotspot_y,
                    data,
                } => {
                    st.cursor_shapes.insert(
                        cursor_id,
                        CachedCursor {
                            width,
                            height,
                            hotspot_x: hotspot_x as i32,
                            hotspot_y: hotspot_y as i32,
                            rgba_data: data,
                        },
                    );
                    tracing::debug!("Cached cursor shape id={} {}x{}", cursor_id, width, height);
                }
                IncomingMessage::CursorPosition {
                    x,
                    y,
                    cursor_id,
                    visible,
                } => {
                    st.cursor_pos = Some((x, y, cursor_id, visible));
                }
                IncomingMessage::AudioFrame { data } => {
                    if let Some(ref mut player) = audio_player {
                        player.play_frame(&data);
                    }
                }
                IncomingMessage::ConnectionStateChanged(new_state) => {
                    info!("Connection state: {:?}", new_state);
                    st.connection_state = new_state;
                }
            }
        }

        // ── Render ──────────────────────────────────────────
        let (win_w, win_h) = canvas.window().size();
        canvas.set_draw_color(BG);
        canvas.clear();

        // Video area: between toolbar and status bar
        let vid_h = (win_h as i32 - TB_H - SB_H).max(0) as u32;
        if let Some(ref t) = video_tex {
            if vid_h > 0 {
                let dst = fit_rect(tex_w, tex_h, win_w, vid_h);
                let _ = canvas.copy(
                    t,
                    None,
                    Some(Rect::new(
                        dst.x(),
                        dst.y() + TB_H,
                        dst.width(),
                        dst.height(),
                    )),
                );
            }
        }

        // Remote cursor overlay
        if let Some((cx, cy, cursor_id, visible)) = st.cursor_pos {
            if visible {
                if let Some(shape) = st.cursor_shapes.get(&cursor_id) {
                    if !shape.rgba_data.is_empty() && shape.width > 0 && shape.height > 0 {
                        // Map normalized cursor coords to the video viewport
                        if let Some(ref _t) = video_tex {
                            let vid_rect = fit_rect(tex_w, tex_h, win_w, vid_h);
                            let px = vid_rect.x() + (cx * vid_rect.width() as f64) as i32
                                - shape.hotspot_x;
                            let py = vid_rect.y() + TB_H + (cy * vid_rect.height() as f64) as i32
                                - shape.hotspot_y;

                            // Create an RGBA surface and texture for the cursor
                            if let Ok(surface) = sdl2::surface::Surface::from_data(
                                &mut shape.rgba_data.clone(),
                                shape.width,
                                shape.height,
                                shape.width * 4,
                                sdl2::pixels::PixelFormatEnum::ABGR8888,
                            ) {
                                if let Ok(cursor_tex) = tc.create_texture_from_surface(&surface) {
                                    let _ = canvas.copy(
                                        &cursor_tex,
                                        None,
                                        Some(Rect::new(px, py, shape.width, shape.height)),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Reconnection overlay
        if st.connection_state != ConnectionState::Connected {
            // Semi-transparent dark overlay
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 180));
            canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
            let overlay_rect = Rect::new(0, TB_H, win_w, vid_h);
            canvas.fill_rect(overlay_rect).ok();
            canvas.set_blend_mode(sdl2::render::BlendMode::None);

            // Status text in center
            let overlay_text = match &st.connection_state {
                ConnectionState::Reconnecting {
                    attempt,
                    next_retry_secs,
                } => {
                    format!(
                        "Reconnecting... (attempt {}, retry in {}s)",
                        attempt, next_retry_secs
                    )
                }
                ConnectionState::Disconnected { reason } => {
                    format!("Disconnected: {}", reason)
                }
                ConnectionState::Connected => unreachable!(),
            };
            let tw = overlay_text.len() as i32 * 7;
            let tx = ((win_w as i32 - tw) / 2).max(0);
            let ty = TB_H + (vid_h as i32 / 2);
            txt(&mut canvas, &tc, &font_lg, &overlay_text, TEXT_HI, tx, ty);
        }

        // ── Toolbar bar ─────────────────────────────────────
        canvas.set_draw_color(TB_BG);
        canvas.fill_rect(Rect::new(0, 0, win_w, TB_H as u32)).ok();
        canvas.set_draw_color(TB_BORDER);
        canvas
            .draw_line((0, TB_H - 1), (win_w as i32, TB_H - 1))
            .ok();

        // Icon buttons
        for (i, icon) in TOOLBAR_ICONS.iter().enumerate() {
            let ix = ICON_X_START + i as i32 * (ICON_SZ + ICON_GAP);
            let rect = Rect::new(ix, ICON_Y, ICON_SZ as u32, ICON_SZ as u32);
            let hovered = st.mouse_x >= ix
                && st.mouse_x < ix + ICON_SZ
                && st.mouse_y >= ICON_Y
                && st.mouse_y < ICON_Y + ICON_SZ;
            let active = st.open_panel == icon.panel;

            canvas.set_draw_color(if active {
                ICON_ACTIVE
            } else if hovered {
                ICON_HOVER
            } else {
                ICON_BG
            });
            canvas.fill_rect(rect).ok();
            if active {
                canvas.set_draw_color(ICON_ACTIVE);
            } else {
                canvas.set_draw_color(TB_BORDER);
            }
            canvas.draw_rect(rect).ok();

            let tc_ref = &tc;
            let c = if active || hovered {
                TEXT_HI
            } else {
                TEXT_NORM
            };
            let tw = icon.symbol.len() as i32 * 7;
            txt(
                &mut canvas,
                tc_ref,
                &font_sm,
                icon.symbol,
                c,
                ix + (ICON_SZ - tw) / 2,
                ICON_Y + 6,
            );
        }

        // Separator after icons
        let sep_x = ICON_X_START + TOOLBAR_ICONS.len() as i32 * (ICON_SZ + ICON_GAP) + 4;
        canvas.set_draw_color(TB_BORDER);
        canvas.draw_line((sep_x, 4), (sep_x, TB_H - 5)).ok();

        // Right side: session name placeholder
        let label = "ScreenControl";
        let lw = label.len() as i32 * 6;
        txt(
            &mut canvas,
            &tc,
            &font_sm,
            label,
            TEXT_DIM,
            (win_w as i32 - lw - 8).max(0),
            8,
        );

        // ── Status bar ───────────────────────────────────────
        let sb_y = win_h as i32 - SB_H;
        canvas.set_draw_color(TB_BG);
        canvas
            .fill_rect(Rect::new(0, sb_y, win_w, SB_H as u32))
            .ok();
        canvas.set_draw_color(TB_BORDER);
        canvas.draw_line((0, sb_y), (win_w as i32, sb_y)).ok();

        // Connection indicator dot
        let dot_color = match &st.connection_state {
            ConnectionState::Connected => GREEN,
            ConnectionState::Reconnecting { .. } => YELLOW,
            ConnectionState::Disconnected { .. } => RED,
        };
        canvas.set_draw_color(dot_color);
        canvas.fill_rect(Rect::new(6, sb_y + 7, 6, 6)).ok();

        // Connection label
        let conn_label = match &st.connection_state {
            ConnectionState::Connected => "Connected",
            ConnectionState::Reconnecting { .. } => "Reconnecting...",
            ConnectionState::Disconnected { .. } => "Disconnected",
        };
        txt(
            &mut canvas,
            &tc,
            &font_sm,
            conn_label,
            TEXT_DIM,
            16,
            sb_y + 5,
        );

        // Separator
        let mut sb_x = 16 + conn_label.len() as i32 * 6 + 8;
        canvas.set_draw_color(TB_BORDER);
        canvas
            .draw_line((sb_x, sb_y + 4), (sb_x, sb_y + SB_H - 4))
            .ok();
        sb_x += 8;

        // Resolution
        let res_str = format!("{}×{}", st.resolution.0, st.resolution.1);
        txt(
            &mut canvas,
            &tc,
            &font_sm,
            &res_str,
            TEXT_NORM,
            sb_x,
            sb_y + 5,
        );
        sb_x += res_str.len() as i32 * 6 + 12;

        // FPS
        let fps_str = format!("{}fps", st.fps);
        txt(
            &mut canvas,
            &tc,
            &font_sm,
            &fps_str,
            TEXT_NORM,
            sb_x,
            sb_y + 5,
        );
        sb_x += fps_str.len() as i32 * 6 + 12;

        // Latency (color-coded)
        let lat_str = format!("{}ms", st.latency_ms);
        let lat_color = if st.latency_ms < 50 {
            GREEN
        } else if st.latency_ms < 150 {
            YELLOW
        } else {
            RED
        };
        txt(
            &mut canvas,
            &tc,
            &font_sm,
            &lat_str,
            lat_color,
            sb_x,
            sb_y + 5,
        );
        sb_x += lat_str.len() as i32 * 6 + 12;

        // Codec
        txt(
            &mut canvas,
            &tc,
            &font_sm,
            st.codec_name,
            TEXT_DIM,
            sb_x,
            sb_y + 5,
        );
        sb_x += st.codec_name.len() as i32 * 6 + 12;

        // Audio state
        let audio_str = if st.audio_muted { "🔇" } else { "🔊" };
        txt(
            &mut canvas,
            &tc,
            &font_sm,
            audio_str,
            TEXT_DIM,
            sb_x,
            sb_y + 5,
        );

        // Right-aligned quality preset
        let q_str = if st.auto_quality {
            format!("{} (Auto)", st.quality_label())
        } else {
            st.quality_label().to_string()
        };
        let qw = q_str.len() as i32 * 6;
        txt(
            &mut canvas,
            &tc,
            &font_sm,
            &q_str,
            TEXT_DIM,
            win_w as i32 - qw - 8,
            sb_y + 5,
        );

        // Toast notification (left of quality, fades after 3s)
        if let Some((ref msg, ts)) = st.toast {
            if ts.elapsed() < Duration::from_secs(3) {
                let tw = msg.len() as i32 * 6;
                txt(
                    &mut canvas,
                    &tc,
                    &font_sm,
                    msg,
                    GREEN,
                    win_w as i32 - qw - tw - 20,
                    sb_y + 5,
                );
            } else {
                st.toast = None;
            }
        }

        // ── Panel overlay ───────────────────────────────────
        if st.open_panel != Panel::None {
            draw_panel(
                &mut canvas,
                &tc,
                &font,
                &font_sm,
                &font_lg,
                &panel_sections,
                &st,
            );
        }

        canvas.present();

        // ── FPS ─────────────────────────────────────────────
        if fps_timer.elapsed() >= Duration::from_secs(1) {
            st.fps = (frame_count as f64 / fps_timer.elapsed().as_secs_f64()) as u32;
            frame_count = 0;
            fps_timer = Instant::now();
            let title = format!(
                "ScreenControl - {}x{} | {} FPS | {}ms",
                st.resolution.0, st.resolution.1, st.fps, st.latency_ms
            );
            let _ = canvas.window_mut().set_title(&title);

            // Auto quality tuning based on latency
            if st.auto_quality && st.latency_ms > 0 {
                let target_preset = if st.latency_ms < 40 {
                    3 // Ultra
                } else if st.latency_ms < 80 {
                    2 // High
                } else if st.latency_ms < 150 {
                    1 // Medium
                } else {
                    0 // Low
                };
                if target_preset != st.quality_preset {
                    st.quality_preset = target_preset;
                    send_quality(&outgoing_tx, &rt, target_preset);
                    info!(
                        "Auto quality → {} (latency {}ms)",
                        QUALITY_PRESETS[target_preset].0, st.latency_ms
                    );
                }
            }
        }

        // ── Ping ────────────────────────────────────────────
        if last_ping.elapsed() >= Duration::from_secs(5) {
            last_ping = Instant::now();
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            let tx = outgoing_tx.clone();
            rt.spawn(async move {
                let _ = tx.send(OutgoingMessage::Ping(ts)).await;
            });
        }

        // ── Clipboard ───────────────────────────────────────
        if let Some(text) = clip_mgr.poll_local_changes() {
            let tx = outgoing_tx.clone();
            rt.spawn(async move {
                let _ = tx
                    .send(OutgoingMessage::Clipboard(
                        sc_protocol::proto::ClipboardData {
                            text,
                            mime_type: "text/plain".into(),
                        },
                    ))
                    .await;
            });
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    info!("Viewer shutting down");
    Ok(())
}

// ─── Input Forwarding ───────────────────────────────────────────

fn forward_input(
    event: &Event,
    canvas: &sdl2::render::Canvas<sdl2::video::Window>,
    tx: &tokio::sync::mpsc::Sender<OutgoingMessage>,
    rt: &tokio::runtime::Runtime,
    relative_mouse: bool,
) {
    let (w, h) = canvas.window().size();
    if let Some(ie) = input::sdl_event_to_input(event, w, h, relative_mouse) {
        let tx = tx.clone();
        rt.spawn(async move {
            let _ = tx.send(OutgoingMessage::Input(ie)).await;
        });
    }
}

// ─── Toolbar Icon Click ────────────────────────────────────────

fn handle_icon_click(x: i32, st: &mut ViewerState) {
    for (i, icon) in TOOLBAR_ICONS.iter().enumerate() {
        let ix = ICON_X_START + i as i32 * (ICON_SZ + ICON_GAP);
        if x >= ix && x < ix + ICON_SZ {
            st.toggle_panel(icon.panel);
            return;
        }
    }
    st.open_panel = Panel::None;
}

// ─── Panel Layout Calculation ───────────────────────────────────

fn calc_panel_rect(sections: &[TileSection], st: &ViewerState) -> Rect {
    if sections.is_empty() {
        return Rect::new(0, 0, 0, 0);
    }

    let icon_idx = match st.open_panel {
        Panel::View => 0,
        Panel::Commands => 1,
        Panel::None => 0,
    };
    let px = ICON_X_START + icon_idx as i32 * (ICON_SZ + ICON_GAP);

    // Calculate panel dimensions from sections
    let mut total_h = TILE_PAD;
    let mut max_row_w: i32 = 0;
    for sec in sections {
        total_h += SECTION_H;
        let cols = sec.tiles.len().min(4) as i32;
        let rows = ((sec.tiles.len() as i32 + cols - 1) / cols).max(1);
        let row_w = cols * TILE_W + (cols - 1) * TILE_GAP;
        max_row_w = max_row_w.max(row_w);
        total_h += rows * TILE_H + (rows - 1).max(0) * TILE_GAP + TILE_GAP;
    }
    total_h += TILE_PAD / 2;

    let pw = max_row_w + TILE_PAD * 2;
    Rect::new(px, TB_H, pw as u32, total_h as u32)
}

fn tile_rect_at(
    sections: &[TileSection],
    sec_idx: usize,
    tile_idx: usize,
    st: &ViewerState,
) -> Rect {
    let icon_idx = match st.open_panel {
        Panel::View => 0,
        Panel::Commands => 1,
        Panel::None => 0,
    };
    let px = ICON_X_START + icon_idx as i32 * (ICON_SZ + ICON_GAP) + TILE_PAD;

    let mut y = TB_H + TILE_PAD;
    for (si, sec) in sections.iter().enumerate() {
        y += SECTION_H;
        let cols = sec.tiles.len().min(4) as i32;
        if si == sec_idx {
            let row = tile_idx as i32 / cols;
            let col = tile_idx as i32 % cols;
            let tx = px + col * (TILE_W + TILE_GAP);
            let ty = y + row * (TILE_H + TILE_GAP);
            return Rect::new(tx, ty, TILE_W as u32, TILE_H as u32);
        }
        let rows = ((sec.tiles.len() as i32 + cols - 1) / cols).max(1);
        y += rows * TILE_H + (rows - 1).max(0) * TILE_GAP + TILE_GAP;
    }
    Rect::new(0, 0, 0, 0)
}

fn hit_test_tile(
    mx: i32,
    my: i32,
    sections: &[TileSection],
    st: &ViewerState,
) -> Option<(usize, usize)> {
    for (si, sec) in sections.iter().enumerate() {
        for ti in 0..sec.tiles.len() {
            let r = tile_rect_at(sections, si, ti, st);
            if mx >= r.x()
                && mx < r.x() + r.width() as i32
                && my >= r.y()
                && my < r.y() + r.height() as i32
            {
                return Some((si, ti));
            }
        }
    }
    None
}

// ─── Tile Action Handler ────────────────────────────────────────

fn handle_tile_click(
    st: &mut ViewerState,
    sec_idx: usize,
    tile_idx: usize,
    sections: &[TileSection],
    tx: &tokio::sync::mpsc::Sender<OutgoingMessage>,
    rt: &tokio::runtime::Runtime,
    canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
    audio_player: &mut Option<audio::AudioPlayer>,
) {
    let tile = match sections.get(sec_idx).and_then(|s| s.tiles.get(tile_idx)) {
        Some(t) => t,
        None => return,
    };

    match &tile.action {
        TileAction::SetQuality(idx) => {
            st.quality_preset = *idx;
            st.auto_quality = false; // manual override disables auto
            send_quality(tx, rt, *idx);
            info!("Quality: {}", QUALITY_PRESETS[*idx].0);
        }
        TileAction::SelectMonitor(idx) => {
            st.active_monitor = *idx;
            info!("Monitor: {}", idx + 1);
            let tx = tx.clone();
            let sw = sc_protocol::proto::MonitorSwitch {
                monitor_index: *idx as u32,
            };
            rt.spawn(async move {
                let _ = tx.send(OutgoingMessage::MonitorSwitch(sw)).await;
            });
        }
        TileAction::Fullscreen => toggle_fullscreen(st, canvas),
        TileAction::SendKeys(idx) => {
            let sk = &SPECIAL_KEYS[*idx];
            info!("Sending: {}", sk.label.replace('\n', " "));
            for &(kc, _) in sk.keys {
                let ie = InputEvent {
                    event: Some(input_event::Event::KeyEvent(KeyEvent {
                        key_code: kc,
                        pressed: true,
                        ctrl: false,
                        alt: false,
                        shift: false,
                        meta: false,
                    })),
                };
                let tx = tx.clone();
                rt.spawn(async move {
                    let _ = tx.send(OutgoingMessage::Input(ie)).await;
                });
            }
            let keys: Vec<u32> = sk.keys.iter().map(|&(kc, _)| kc).collect();
            let tx = tx.clone();
            rt.spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                for kc in keys.into_iter().rev() {
                    let ie = InputEvent {
                        event: Some(input_event::Event::KeyEvent(KeyEvent {
                            key_code: kc,
                            pressed: false,
                            ctrl: false,
                            alt: false,
                            shift: false,
                            meta: false,
                        })),
                    };
                    let _ = tx.send(OutgoingMessage::Input(ie)).await;
                }
            });
        }
        TileAction::Clipboard => {
            info!("Sending clipboard");
            if let Ok(mut c) = arboard::Clipboard::new() {
                if let Ok(text) = c.get_text() {
                    let tx = tx.clone();
                    rt.spawn(async move {
                        let _ = tx
                            .send(OutgoingMessage::Clipboard(
                                sc_protocol::proto::ClipboardData {
                                    text,
                                    mime_type: "text/plain".into(),
                                },
                            ))
                            .await;
                    });
                }
            }
        }
        TileAction::Screenshot => take_screenshot(canvas, st),
        TileAction::ToggleRelativeMouse => {
            st.relative_mouse = !st.relative_mouse;
            unsafe {
                sdl2::sys::SDL_SetRelativeMouseMode(if st.relative_mouse {
                    sdl2::sys::SDL_bool::SDL_TRUE
                } else {
                    sdl2::sys::SDL_bool::SDL_FALSE
                });
            }
            info!("Relative mouse mode: {}", st.relative_mouse);
        }
        TileAction::ToggleAudioMute => {
            if let Some(ref mut player) = audio_player {
                st.audio_muted = player.toggle_mute();
            } else {
                st.audio_muted = !st.audio_muted;
            }
            info!("Audio muted: {}", st.audio_muted);
        }
        TileAction::ToggleAutoQuality => {
            st.auto_quality = !st.auto_quality;
            info!("Auto quality: {}", st.auto_quality);
        }
    }
}

// ─── Drawing ────────────────────────────────────────────────────

fn draw_panel(
    canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
    tc: &sdl2::render::TextureCreator<sdl2::video::WindowContext>,
    _font: &sdl2::ttf::Font,
    font_sm: &sdl2::ttf::Font,
    font_lg: &sdl2::ttf::Font,
    sections: &[TileSection],
    st: &ViewerState,
) {
    let prect = calc_panel_rect(sections, st);

    // Panel background with border
    canvas.set_draw_color(PANEL_BG);
    canvas.fill_rect(prect).ok();
    canvas.set_draw_color(PANEL_BORDER);
    canvas.draw_rect(prect).ok();

    // Draw sections and tiles
    for (si, sec) in sections.iter().enumerate() {
        // Section header
        let first_tile = tile_rect_at(sections, si, 0, st);
        let header_y = first_tile.y() - SECTION_H + 2;
        txt(
            canvas,
            tc,
            font_sm,
            sec.header,
            SECTION_HD,
            first_tile.x(),
            header_y,
        );

        // Tiles
        for (ti, tile) in sec.tiles.iter().enumerate() {
            let r = tile_rect_at(sections, si, ti, st);
            let hovered = st.hover_tile == Some((si, ti));

            let bg = if tile.is_active {
                TILE_ACTIVE
            } else if hovered {
                TILE_HOVER
            } else {
                TILE_BG
            };
            canvas.set_draw_color(bg);
            canvas.fill_rect(r).ok();

            // Tile border
            canvas.set_draw_color(if tile.is_active {
                TILE_ACTIVE
            } else {
                Color::RGB(0x33, 0x33, 0x33)
            });
            canvas.draw_rect(r).ok();

            // Symbol (centered, large)
            let sym_c = if tile.is_active || hovered {
                TEXT_HI
            } else {
                TEXT_NORM
            };
            let sym_w = tile.symbol.len() as i32 * 8;
            txt(
                canvas,
                tc,
                font_lg,
                &tile.symbol,
                sym_c,
                r.x() + (TILE_W - sym_w) / 2,
                r.y() + 12,
            );

            // Label (centered, small)
            let lbl = tile.label.lines().next().unwrap_or("");
            let lbl_c = if tile.is_active || hovered {
                TEXT_NORM
            } else {
                TEXT_DIM
            };
            let lbl_w = lbl.len() as i32 * 6;
            txt(
                canvas,
                tc,
                font_sm,
                lbl,
                lbl_c,
                r.x() + (TILE_W - lbl_w) / 2,
                r.y() + TILE_H - 18,
            );
        }
    }
}

fn txt(
    canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
    tc: &sdl2::render::TextureCreator<sdl2::video::WindowContext>,
    font: &sdl2::ttf::Font,
    text: &str,
    color: Color,
    x: i32,
    y: i32,
) {
    if text.is_empty() {
        return;
    }
    let surface = match font.render(text).blended(color) {
        Ok(s) => s,
        Err(_) => return,
    };
    let tex = match tc.create_texture_from_surface(&surface) {
        Ok(t) => t,
        Err(_) => return,
    };
    let q = tex.query();
    let _ = canvas.copy(&tex, None, Some(Rect::new(x, y, q.width, q.height)));
}

fn fit_rect(sw: u32, sh: u32, dw: u32, dh: u32) -> Rect {
    let sa = sw as f64 / sh as f64;
    let da = dw as f64 / dh as f64;
    let (w, h) = if sa > da {
        (dw, (dw as f64 / sa) as u32)
    } else {
        ((dh as f64 * sa) as u32, dh)
    };
    Rect::new(((dw - w) / 2) as i32, ((dh - h) / 2) as i32, w, h)
}

// ─── Helpers ────────────────────────────────────────────────────

fn send_quality(
    tx: &tokio::sync::mpsc::Sender<OutgoingMessage>,
    rt: &tokio::runtime::Runtime,
    idx: usize,
) {
    let (_, quality, max_fps, bitrate) = QUALITY_PRESETS[idx];
    let tx = tx.clone();
    rt.spawn(async move {
        let _ = tx
            .send(OutgoingMessage::Quality(
                sc_protocol::proto::QualitySettings {
                    quality,
                    max_fps,
                    bitrate_kbps: bitrate,
                },
            ))
            .await;
    });
}

fn toggle_fullscreen(st: &mut ViewerState, canvas: &mut sdl2::render::Canvas<sdl2::video::Window>) {
    st.is_fullscreen = !st.is_fullscreen;
    let mode = if st.is_fullscreen {
        sdl2::video::FullscreenType::Desktop
    } else {
        sdl2::video::FullscreenType::Off
    };
    let _ = canvas.window_mut().set_fullscreen(mode);
}

fn take_screenshot<T: sdl2::render::RenderTarget>(
    canvas: &sdl2::render::Canvas<T>,
    st: &mut ViewerState,
) {
    let (w, h) = canvas.output_size().unwrap_or((1, 1));
    match canvas.read_pixels(None, sdl2::pixels::PixelFormatEnum::RGB24) {
        Ok(data) => {
            // Ensure ~/Pictures/ScreenControl/ exists
            let dir = dirs_next_or_cwd();
            let _ = std::fs::create_dir_all(&dir);

            let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let fname = format!(
                "screenshot_{}x{}_{}.png",
                st.resolution.0, st.resolution.1, ts
            );
            let path = std::path::Path::new(&dir).join(&fname);

            match image::save_buffer(&path, &data, w, h, image::ColorType::Rgb8) {
                Ok(_) => {
                    info!("Screenshot saved: {}", path.display());
                    st.toast = Some((format!("Saved: {}", fname), Instant::now()));
                }
                Err(e) => {
                    error!("Screenshot failed: {}", e);
                    st.toast = Some((format!("Screenshot failed: {}", e), Instant::now()));
                }
            }
        }
        Err(e) => error!("read_pixels: {}", e),
    }
}

fn dirs_next_or_cwd() -> String {
    if let Some(pic_dir) = dirs::picture_dir() {
        pic_dir.join("ScreenControl").to_string_lossy().to_string()
    } else {
        "./screenshots".to_string()
    }
}

async fn resolve_session(args: &Args) -> Result<(String, String)> {
    if let (Some(s), Some(t)) = (&args.session, &args.token) {
        return Ok((s.clone(), t.clone()));
    }
    let email = args.email.as_ref().context("--email required")?;
    let password = args.password.as_ref().context("--password required")?;
    let agent_id = args.agent.as_ref().context("--agent required")?;
    let token = session::login(&args.server, email, password).await?;
    let info = session::create_session(&args.server, &token, agent_id).await?;
    Ok((info.id, token))
}
