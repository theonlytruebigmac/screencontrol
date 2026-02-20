//! Screen capture for remote desktop sessions.
//!
//! Platform-specific implementations:
//! - **Linux (GNOME)**: Uses the Mutter ScreenCast D-Bus API + GStreamer pipeline
//!   for silent, unattended capture — no portal dialog needed.
//! - **macOS**: Placeholder — will use ScreenCaptureKit or CGDisplayStream.
//! - **Windows**: Placeholder — will use DXGI Desktop Duplication.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use prost::Message as ProstMessage;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use uuid::Uuid;

use sc_protocol::{envelope, Envelope, MonitorInfo, ScreenInfo};

/// Handle for Mutter RemoteDesktop D-Bus input injection.
/// Provides the D-Bus connection and session paths needed to call
/// NotifyPointerMotionAbsolute, NotifyPointerButton, etc.
#[cfg(target_os = "linux")]
pub struct MutterInputHandle {
    pub connection: zbus::Connection,
    pub rd_session_path: zbus::zvariant::OwnedObjectPath,
    pub stream_path: zbus::zvariant::OwnedObjectPath,
    pub screen_width: u32,
    pub screen_height: u32,
}

/// Platform-gated return type for start_capture's input handle.
#[cfg(target_os = "linux")]
pub type InputHandleResult = MutterInputHandle;
#[cfg(not(target_os = "linux"))]
pub type InputHandleResult = ();

/// Manages screen capture sessions.
pub struct DesktopCapturer {
    sessions: HashMap<String, CaptureHandle>,
}

/// Shared quality/fps configuration, updated atomically from the WS handler
/// and read by the capture loop each frame.
pub struct QualityConfig {
    /// JPEG quality (0–100). 0 means "use default".
    pub quality: AtomicU32,
    /// Maximum FPS cap. 0 means "use default".
    pub max_fps: AtomicU32,
    /// H264 bitrate in kbps. 0 means "use default (4000)".
    pub bitrate_kbps: AtomicU32,
}

impl QualityConfig {
    pub fn new() -> Self {
        Self {
            quality: AtomicU32::new(0),
            max_fps: AtomicU32::new(0),
            bitrate_kbps: AtomicU32::new(0),
        }
    }

    /// Get the effective JPEG quality, falling back to the platform default.
    pub fn effective_quality(&self, platform_default: u32) -> u32 {
        let v = self.quality.load(Ordering::Relaxed);
        if v == 0 {
            platform_default
        } else {
            v.min(100)
        }
    }

    /// Get the effective max FPS, falling back to the platform default.
    pub fn effective_fps(&self, platform_default: u32) -> u32 {
        let v = self.max_fps.load(Ordering::Relaxed);
        if v == 0 {
            platform_default
        } else {
            v.min(60)
        }
    }

    /// Get the effective H264 bitrate in kbps, falling back to the platform default.
    pub fn effective_bitrate(&self, platform_default: u32) -> u32 {
        let v = self.bitrate_kbps.load(Ordering::Relaxed);
        if v == 0 {
            platform_default
        } else {
            v.clamp(500, 20_000)
        }
    }
}

struct CaptureHandle {
    handle: JoinHandle<()>,
    /// Send on this channel to trigger Mutter ScreenCast D-Bus session cleanup.
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Shared quality configuration for dynamic adjustment.
    quality: Arc<QualityConfig>,
}

impl DesktopCapturer {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Get available monitors and return a `ScreenInfo` envelope.
    pub fn get_screen_info(session_id: &str) -> Option<Vec<u8>> {
        let monitor_infos = enumerate_monitors();

        let envelope = Envelope {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            timestamp: None,
            payload: Some(envelope::Payload::ScreenInfo(ScreenInfo {
                monitors: monitor_infos,
                active_monitor: 0,
            })),
        };

        let mut buf = Vec::new();
        envelope.encode(&mut buf).ok()?;
        Some(buf)
    }

    /// Start screen capture using the platform-specific implementation.
    /// On Linux, returns a receiver that resolves with the Mutter D-Bus
    /// input handle once the RemoteDesktop session is established.
    /// Set runtime quality/fps on an active capture session.
    pub fn set_quality(&self, session_id: &str, quality: u32, max_fps: u32, bitrate_kbps: u32) {
        if let Some(handle) = self.sessions.get(session_id) {
            handle.quality.quality.store(quality, Ordering::Relaxed);
            handle.quality.max_fps.store(max_fps, Ordering::Relaxed);
            handle
                .quality
                .bitrate_kbps
                .store(bitrate_kbps, Ordering::Relaxed);
            tracing::info!(
                session_id,
                quality,
                max_fps,
                bitrate_kbps,
                "Quality config updated on capture session"
            );
        } else {
            tracing::warn!(session_id, "set_quality: no active capture session found");
        }
    }

    pub fn start_capture(
        &mut self,
        session_id: &str,
        monitor_index: u32,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Option<oneshot::Receiver<InputHandleResult>> {
        let sid = session_id.to_string();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let quality_config = Arc::new(QualityConfig::new());
        let qc = quality_config.clone();

        #[cfg(target_os = "linux")]
        let (input_handle_tx, input_handle_rx) = oneshot::channel::<MutterInputHandle>();

        #[cfg(target_os = "linux")]
        let handle = tokio::spawn(linux::capture_session(
            sid.clone(),
            monitor_index,
            ws_tx,
            shutdown_rx,
            input_handle_tx,
            qc.clone(),
        ));

        #[cfg(target_os = "macos")]
        let handle = tokio::spawn(macos::capture_session(
            sid.clone(),
            monitor_index,
            ws_tx,
            shutdown_rx,
            qc.clone(),
        ));

        #[cfg(target_os = "windows")]
        let handle = tokio::spawn(windows::capture_session(
            sid.clone(),
            monitor_index,
            ws_tx,
            shutdown_rx,
            qc.clone(),
        ));

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            drop(shutdown_rx);
            let handle = tokio::spawn(async move {
                tracing::error!("Screen capture not supported on this platform");
                let _ = (ws_tx, monitor_index);
            });
        }

        self.sessions.insert(
            session_id.to_string(),
            CaptureHandle {
                handle,
                shutdown_tx: Some(shutdown_tx),
                quality: quality_config,
            },
        );

        #[cfg(target_os = "linux")]
        return Some(input_handle_rx);

        #[cfg(not(target_os = "linux"))]
        return None;
    }

    /// Stop an active capture session.
    pub fn stop_capture(&mut self, session_id: &str) {
        if let Some(mut handle) = self.sessions.remove(session_id) {
            // Signal the capture task to stop gracefully
            if let Some(tx) = handle.shutdown_tx.take() {
                let _ = tx.send(());
                tracing::info!("Sent shutdown signal for capture session: {}", session_id);
            }
            // Give the task a moment to shut down cleanly before aborting
            let sid = session_id.to_string();
            tokio::spawn(async move {
                tokio::select! {
                    _ = &mut handle.handle => {
                        tracing::info!("Capture session exited cleanly: {}", sid);
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {
                        tracing::warn!("Capture session did not exit in 3s, aborting: {}", sid);
                        handle.handle.abort();
                    }
                }
            });
        }
    }

    /// Stop all capture sessions.
    pub fn stop_all(&mut self) {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        for id in ids {
            self.stop_capture(&id);
        }
    }
}

/// Enumerate connected monitors using platform-specific tools.
pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    #[cfg(target_os = "linux")]
    {
        if let Some(monitors) = enumerate_monitors_xrandr() {
            if !monitors.is_empty() {
                return monitors;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let monitors = macos::enumerate_monitors_cg();
        if !monitors.is_empty() {
            return monitors;
        }
    }

    // Fallback: single primary display
    vec![MonitorInfo {
        index: 0,
        name: "Primary Display".to_string(),
        width: 0,
        height: 0,
        primary: true,
        x: 0,
        y: 0,
        scale_factor: 1.0,
    }]
}

/// Get the total bounding box of all monitors (for input coordinate mapping).
pub fn get_total_screen_size() -> (u32, u32) {
    let monitors = enumerate_monitors();
    let mut max_w: u32 = 0;
    let mut max_h: u32 = 0;
    for m in &monitors {
        let right = m.x as u32 + m.width;
        let bottom = m.y as u32 + m.height;
        if right > max_w {
            max_w = right;
        }
        if bottom > max_h {
            max_h = bottom;
        }
    }
    if max_w == 0 || max_h == 0 {
        (1920, 1080) // fallback
    } else {
        (max_w, max_h)
    }
}

/// Parse `xrandr --query` to enumerate connected monitors.
#[cfg(target_os = "linux")]
fn enumerate_monitors_xrandr() -> Option<Vec<MonitorInfo>> {
    use std::process::Command;

    let output = Command::new("xrandr").arg("--query").output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut monitors = Vec::new();
    let mut index = 0u32;

    // Parse lines like:
    // HDMI-1 connected primary 1920x1080+0+0 (normal left inverted right x axis y axis) 527mm x 296mm
    // DP-2 connected 2560x1440+1920+0 (normal left inverted right x axis y axis) 597mm x 336mm
    for line in stdout.lines() {
        if !line.contains(" connected") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let name = parts[0].to_string();
        let is_primary = line.contains("primary");

        // Find the resolution+position token (WxH+X+Y)
        let mut width = 0u32;
        let mut height = 0u32;
        let mut pos_x = 0i32;
        let mut pos_y = 0i32;

        for part in &parts[2..] {
            // Match pattern like "1920x1080+0+0"
            if part.contains('x') && part.contains('+') {
                if let Some((res, pos)) = part.split_once('+') {
                    if let Some((w, h)) = res.split_once('x') {
                        width = w.parse().unwrap_or(0);
                        height = h.parse().unwrap_or(0);
                    }
                    // Parse position: remaining is "X+Y"
                    if let Some((x, y)) = pos.split_once('+') {
                        pos_x = x.parse().unwrap_or(0);
                        pos_y = y.parse().unwrap_or(0);
                    }
                }
                break;
            }
        }

        monitors.push(MonitorInfo {
            index,
            name,
            width,
            height,
            primary: is_primary,
            x: pos_x,
            y: pos_y,
            scale_factor: 1.0,
        });

        index += 1;
    }

    if monitors.is_empty() {
        None
    } else {
        Some(monitors)
    }
}

// ─── Shared H264 helpers ────────────────────────────────────────────

/// Extract a complete H.264 access unit from the buffer.
///
/// x264enc inserts an AUD (Access Unit Delimiter, NAL type 9) at the start
/// of each access unit. We use AUD→AUD boundaries to correctly group all
/// NALs of a single picture — including multi-slice frames where the
/// encoder splits one picture into several VCL NALs (important for large
/// resolutions like 5120×1440).
///
/// If no AUDs are found (e.g. from encoders that don't emit them), falls
/// back to grouping non-VCL NALs with the next VCL NAL.
fn extract_h264_access_unit(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let first = find_h264_start_code(buffer, 0)?;

    // ── Strategy 1: AUD-based delimiting (preferred) ──
    // Find the first AUD in the buffer
    let first_aud = find_nal_of_type(buffer, first, 9);
    if let Some(aud_start) = first_aud {
        // Find the NEXT AUD after this one (marks the end of the current AU)
        // We need to skip past the current AUD's start code first
        let sc_len = start_code_len(buffer, aud_start);
        let after_aud = aud_start + sc_len + 1; // past the NAL header byte
        let next_aud = find_nal_of_type(buffer, after_aud, 9);

        match next_aud {
            Some(next_aud_start) => {
                // Complete AU: everything from first AUD to just before next AUD
                let au = buffer[aud_start..next_aud_start].to_vec();
                buffer.drain(..next_aud_start);
                return Some(au);
            }
            None => {
                // No second AUD yet — the current AU is incomplete
                // (still waiting for more data from the encoder)
                return None;
            }
        }
    }

    // ── Strategy 2: VCL-based delimiting (fallback for no-AUD encoders) ──
    let mut pos = first;
    loop {
        let sc = find_h264_start_code(buffer, pos)?;
        let sc_l = start_code_len(buffer, sc);
        let nal_header_pos = sc + sc_l;
        if nal_header_pos >= buffer.len() {
            return None;
        }

        let nal_type = buffer[nal_header_pos] & 0x1F;
        let next_sc = find_h264_start_code(buffer, nal_header_pos + 1);

        if nal_type >= 1 && nal_type <= 5 {
            match next_sc {
                Some(next_pos) => {
                    let au = buffer[first..next_pos].to_vec();
                    buffer.drain(..next_pos);
                    return Some(au);
                }
                None => return None,
            }
        } else {
            match next_sc {
                Some(next_pos) => {
                    pos = next_pos;
                }
                None => return None,
            }
        }
    }
}

/// Find the first NAL unit of a specific type starting from `from`.
fn find_nal_of_type(data: &[u8], from: usize, target_type: u8) -> Option<usize> {
    let mut pos = from;
    loop {
        let sc = find_h264_start_code(data, pos)?;
        let sc_l = start_code_len(data, sc);
        let header_pos = sc + sc_l;
        if header_pos >= data.len() {
            return None;
        }
        let nal_type = data[header_pos] & 0x1F;
        if nal_type == target_type {
            return Some(sc);
        }
        pos = header_pos + 1;
    }
}

/// Return the length of the start code at position `pos` (3 or 4 bytes).
fn start_code_len(data: &[u8], pos: usize) -> usize {
    if pos + 3 < data.len()
        && data[pos] == 0x00
        && data[pos + 1] == 0x00
        && data[pos + 2] == 0x00
        && data[pos + 3] == 0x01
    {
        4
    } else {
        3
    }
}

/// Find a H264 Annex B start code (00 00 00 01 or 00 00 01) starting at `from`.
fn find_h264_start_code(data: &[u8], from: usize) -> Option<usize> {
    if data.len() < from + 4 {
        return None;
    }
    for i in from..data.len().saturating_sub(3) {
        if data[i] == 0x00 && data[i + 1] == 0x00 && data[i + 2] == 0x00 && data[i + 3] == 0x01 {
            return Some(i);
        }
        if data[i] == 0x00 && data[i + 1] == 0x00 && data[i + 2] == 0x01 {
            return Some(i);
        }
    }
    None
}

/// Check if an H264 NAL unit is a keyframe (IDR slice, NAL type 5)
/// or contains SPS (type 7) which precedes keyframes.
fn is_h264_keyframe(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len().saturating_sub(4) {
        let is_4byte =
            data[i] == 0x00 && data[i + 1] == 0x00 && data[i + 2] == 0x00 && data[i + 3] == 0x01;
        let is_3byte = data[i] == 0x00 && data[i + 1] == 0x00 && data[i + 2] == 0x01;
        if is_4byte || is_3byte {
            let nal_byte_offset = if is_4byte { i + 4 } else { i + 3 };
            if nal_byte_offset < data.len() {
                let nal_type = data[nal_byte_offset] & 0x1F;
                if nal_type == 5 || nal_type == 7 {
                    return true;
                }
            }
            i = nal_byte_offset + 1;
        } else {
            i += 1;
        }
    }
    false
}

/// Parse H264 SPS NAL unit to extract resolution (width, height).
/// Returns None if no SPS is found or parsing fails.
fn parse_h264_sps_resolution(data: &[u8]) -> Option<(u32, u32)> {
    // Find an SPS NAL unit (type 7)
    let mut i = 0;
    while i < data.len().saturating_sub(4) {
        let is_4byte =
            data[i] == 0x00 && data[i + 1] == 0x00 && data[i + 2] == 0x00 && data[i + 3] == 0x01;
        let is_3byte = !is_4byte && data[i] == 0x00 && data[i + 1] == 0x00 && data[i + 2] == 0x01;
        if is_4byte || is_3byte {
            let nal_offset = if is_4byte { i + 4 } else { i + 3 };
            if nal_offset < data.len() {
                let nal_type = data[nal_offset] & 0x1F;
                if nal_type == 7 {
                    // Parse SPS starting after the NAL header byte
                    return parse_sps_dimensions(&data[nal_offset + 1..]);
                }
            }
            i = nal_offset + 1;
        } else {
            i += 1;
        }
    }
    None
}

/// Parse SPS NAL unit payload to extract pic_width and pic_height in pixels.
/// Uses minimal exp-Golomb decoding — only reads fields up to the resolution.
fn parse_sps_dimensions(sps: &[u8]) -> Option<(u32, u32)> {
    if sps.len() < 4 {
        return None;
    }
    let mut bit_pos: usize = 0;

    let read_bits = |pos: &mut usize, n: usize| -> Option<u32> {
        if *pos + n > sps.len() * 8 {
            return None;
        }
        let mut val: u32 = 0;
        for _ in 0..n {
            val = (val << 1) | ((sps[*pos / 8] >> (7 - (*pos % 8))) as u32 & 1);
            *pos += 1;
        }
        Some(val)
    };

    let read_ue = |pos: &mut usize| -> Option<u32> {
        let mut leading_zeros: u32 = 0;
        loop {
            if *pos >= sps.len() * 8 {
                return None;
            }
            let bit = (sps[*pos / 8] >> (7 - (*pos % 8))) & 1;
            *pos += 1;
            if bit == 1 {
                break;
            }
            leading_zeros += 1;
            if leading_zeros > 31 {
                return None;
            }
        }
        if leading_zeros == 0 {
            return Some(0);
        }
        let val = read_bits(pos, leading_zeros as usize)?;
        Some((1 << leading_zeros) - 1 + val)
    };

    // profile_idc (8 bits)
    let profile_idc = read_bits(&mut bit_pos, 8)?;
    // constraint flags + reserved (8 bits) + level_idc (8 bits)
    let _ = read_bits(&mut bit_pos, 16)?;
    // seq_parameter_set_id
    let _ = read_ue(&mut bit_pos)?;

    // High profiles have extra chroma/scaling fields
    if matches!(
        profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134
    ) {
        let chroma_format_idc = read_ue(&mut bit_pos)?;
        if chroma_format_idc == 3 {
            let _ = read_bits(&mut bit_pos, 1)?; // separate_colour_plane_flag
        }
        let _ = read_ue(&mut bit_pos)?; // bit_depth_luma_minus8
        let _ = read_ue(&mut bit_pos)?; // bit_depth_chroma_minus8
        let _ = read_bits(&mut bit_pos, 1)?; // qpprime_y_zero_transform_bypass_flag
        let seq_scaling_matrix_present = read_bits(&mut bit_pos, 1)?;
        if seq_scaling_matrix_present == 1 {
            let count = if chroma_format_idc != 3 { 8 } else { 12 };
            for _ in 0..count {
                let present = read_bits(&mut bit_pos, 1)?;
                if present == 1 {
                    let size = if count < 6 { 16 } else { 64 };
                    // Skip scaling list (just need to advance the bit position)
                    let mut last_scale: i32 = 8;
                    let mut next_scale: i32 = 8;
                    for _ in 0..size {
                        if next_scale != 0 {
                            // delta_scale is a signed exp-Golomb (se)
                            let code = read_ue(&mut bit_pos)?;
                            let delta: i32 = if code % 2 == 0 {
                                -(code as i32 / 2)
                            } else {
                                (code as i32 + 1) / 2
                            };
                            next_scale = (last_scale + delta + 256) % 256;
                        }
                        if next_scale != 0 {
                            last_scale = next_scale;
                        }
                    }
                }
            }
        }
    }

    // log2_max_frame_num_minus4
    let _ = read_ue(&mut bit_pos)?;
    // pic_order_cnt_type
    let poc_type = read_ue(&mut bit_pos)?;
    if poc_type == 0 {
        let _ = read_ue(&mut bit_pos)?; // log2_max_pic_order_cnt_lsb_minus4
    } else if poc_type == 1 {
        let _ = read_bits(&mut bit_pos, 1)?; // delta_pic_order_always_zero_flag
                                             // offset_for_non_ref_pic (se)
        let _ = read_ue(&mut bit_pos)?;
        // offset_for_top_to_bottom_field (se)
        let _ = read_ue(&mut bit_pos)?;
        let num_ref = read_ue(&mut bit_pos)?;
        for _ in 0..num_ref {
            let _ = read_ue(&mut bit_pos)?; // offset_for_ref_frame
        }
    }

    // max_num_ref_frames
    let _ = read_ue(&mut bit_pos)?;
    // gaps_in_frame_num_value_allowed_flag
    let _ = read_bits(&mut bit_pos, 1)?;

    // pic_width_in_mbs_minus1
    let pic_width_mbs = read_ue(&mut bit_pos)? + 1;
    // pic_height_in_map_units_minus1
    let pic_height_map_units = read_ue(&mut bit_pos)? + 1;

    let width = pic_width_mbs * 16;
    let height = pic_height_map_units * 16;

    Some((width, height))
}

/// Shared H264 stream reader: reads NAL units from a byte stream,
/// wraps them in protobuf envelopes, and sends them over the WS channel.
/// Returns the number of frames sent, or an error.

/// Send cursor position (and shape on first appearance) alongside a frame.
///
/// This queries the platform cursor API, checks for shape changes via
/// `CursorTracker`, and sends `CursorData` + `CursorPosition` protobuf
/// messages over the WebSocket channel.
#[allow(dead_code)]
fn send_cursor_update(
    ws_tx: &mpsc::UnboundedSender<Vec<u8>>,
    session_id: &str,
    tracker: &mut crate::cursor::CursorTracker,
    monitor_x: i32,
    monitor_y: i32,
    monitor_w: u32,
    monitor_h: u32,
) {
    use sc_protocol::{CursorData, CursorPosition};

    // Query cursor position from platform API
    let pos = match crate::cursor::platform::get_cursor_position(
        monitor_x, monitor_y, monitor_w, monitor_h,
    ) {
        Some(p) => p,
        None => return, // Cursor position not available on this platform
    };

    // Check if cursor shape changed and needs to be sent
    if tracker.needs_shape_update(pos.cursor_id) {
        if let Some(shape) = crate::cursor::platform::get_cursor_shape() {
            let cursor_msg = Envelope {
                id: "c".to_string(),
                session_id: session_id.to_string(),
                timestamp: None,
                payload: Some(envelope::Payload::CursorData(CursorData {
                    cursor_id: shape.cursor_id,
                    width: shape.width,
                    height: shape.height,
                    hotspot_x: shape.hotspot_x,
                    hotspot_y: shape.hotspot_y,
                    data: shape.data,
                })),
            };
            let mut buf = Vec::with_capacity(512);
            if cursor_msg.encode(&mut buf).is_ok() {
                let _ = ws_tx.send(buf);
            }
            tracker.mark_shape_sent(shape.cursor_id);
        } else {
            // No shape data available — mark as sent to avoid repeated queries
            tracker.mark_shape_sent(pos.cursor_id);
        }
    }

    // Send position (skip if not meaningfully changed)
    if !tracker.position_changed(pos.x, pos.y) {
        return;
    }

    let pos_msg = Envelope {
        id: "p".to_string(),
        session_id: session_id.to_string(),
        timestamp: None,
        payload: Some(envelope::Payload::CursorPosition(CursorPosition {
            x: pos.x,
            y: pos.y,
            cursor_id: pos.cursor_id,
            visible: pos.visible,
        })),
    };
    let mut buf = Vec::with_capacity(64);
    if pos_msg.encode(&mut buf).is_ok() {
        let _ = ws_tx.send(buf);
    }
    tracker.update_position(pos.x, pos.y, pos.cursor_id);
}

/// Shared H264 stream reader: reads NAL units from a byte stream,
/// wraps them in protobuf envelopes, and sends them over the WS channel.
/// Returns the number of frames sent, or an error.
fn read_h264_stream(
    stdout: &mut dyn std::io::Read,
    session_id: &str,
    ws_tx: &mpsc::UnboundedSender<Vec<u8>>,
    frame_seq: &std::sync::atomic::AtomicU32,
    quality: u32,
    quality_config: Option<&std::sync::Arc<QualityConfig>>,
) -> std::io::Result<u32> {
    use sc_protocol::{envelope, DesktopFrame, Envelope, FrameCodec};
    use std::sync::atomic::Ordering;

    let mut buffer = Vec::with_capacity(1024 * 1024);
    let mut read_buf = [0u8; 65536];
    let mut total_bytes: u64 = 0;
    let mut frames_sent: u32 = 0;
    let mut stream_width: u32 = 0;
    let mut stream_height: u32 = 0;
    let sid = session_id.to_string();
    let mut last_frame_sent_time = std::time::Instant::now();
    let mut last_read_time = std::time::Instant::now();

    // Phase 5a: Frame deduplication — hash-based skip for unchanged frames
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut prev_frame_hash: u64 = 0;
    let mut dedup_skips: u64 = 0;

    // Phase 5b: Backpressure — skip frames when WS send queue is full
    let mut backpressure_skips: u64 = 0;

    // Phase 5c: Dynamic bitrate adjustment
    let mut bitrate_window_start = std::time::Instant::now();
    let mut bitrate_window_bytes: u64 = 0;
    const BITRATE_WINDOW_SECS: f64 = 2.0;
    const MIN_BITRATE_KBPS: u32 = 2000;
    const MAX_BITRATE_KBPS: u32 = 20_000;

    // Phase 5d: Keyframe-first guard — drop P-frames until first keyframe
    let mut seen_keyframe = false;

    loop {
        if ws_tx.is_closed() {
            tracing::info!("WS channel closed, stopping H264 reader");
            return Ok(frames_sent);
        }

        match stdout.read(&mut read_buf) {
            Ok(0) => {
                tracing::info!(total_bytes, frames_sent, "H264 stream EOF");
                return Ok(frames_sent);
            }
            Ok(n) => {
                total_bytes += n as u64;
                last_read_time = std::time::Instant::now();
                if total_bytes == n as u64 {
                    tracing::info!(
                        bytes = n,
                        first_bytes = ?&read_buf[..std::cmp::min(n, 20)],
                        "H264 reader: first data received"
                    );
                }
                buffer.extend_from_slice(&read_buf[..n]);

                if frames_sent < 5 {
                    tracing::info!(
                        read_bytes = n,
                        buffer_len = buffer.len(),
                        total_bytes,
                        "H264 reader: buffer state before extraction"
                    );
                }

                while let Some(nal_unit) = extract_h264_access_unit(&mut buffer) {
                    let seq = frame_seq.fetch_add(1, Ordering::Relaxed);
                    let nal_len = nal_unit.len();
                    let is_keyframe = is_h264_keyframe(&nal_unit);

                    if seq < 5 {
                        // Log first few NAL types in the access unit for debugging
                        let mut nal_types = Vec::new();
                        let mut i = 0;
                        while i < nal_unit.len().saturating_sub(4) {
                            let is_4byte = nal_unit[i] == 0x00
                                && nal_unit[i + 1] == 0x00
                                && nal_unit[i + 2] == 0x00
                                && nal_unit[i + 3] == 0x01;
                            let is_3byte = nal_unit[i] == 0x00
                                && nal_unit[i + 1] == 0x00
                                && nal_unit[i + 2] == 0x01;
                            if is_4byte || is_3byte {
                                let off = if is_4byte { i + 4 } else { i + 3 };
                                if off < nal_unit.len() {
                                    nal_types.push(nal_unit[off] & 0x1F);
                                }
                                i = off + 1;
                            } else {
                                i += 1;
                            }
                        }
                        tracing::info!(
                            seq,
                            nal_len,
                            is_keyframe,
                            ?nal_types,
                            remaining_buffer = buffer.len(),
                            "H264: extracted access unit"
                        );
                    }

                    // Parse resolution from SPS on first keyframe
                    if is_keyframe && stream_width == 0 {
                        if let Some((w, h)) = parse_h264_sps_resolution(&nal_unit) {
                            stream_width = w;
                            stream_height = h;
                            tracing::info!(
                                width = w,
                                height = h,
                                "H264 stream resolution from SPS"
                            );
                        }
                    }

                    // Phase 5d: Drop P-frames until first keyframe arrives
                    if !is_keyframe && !seen_keyframe {
                        tracing::debug!(seq, "Dropping P-frame before first keyframe");
                        continue;
                    }
                    if is_keyframe && !seen_keyframe {
                        seen_keyframe = true;
                        tracing::info!(seq, "First keyframe received, stream ready");
                    }

                    // Phase 5a: Frame deduplication — skip unchanged P-frames
                    if !is_keyframe {
                        let hash_len = std::cmp::min(nal_unit.len(), 4096);
                        let mut hasher = DefaultHasher::new();
                        nal_unit[..hash_len].hash(&mut hasher);
                        let frame_hash = hasher.finish();
                        if frame_hash == prev_frame_hash {
                            dedup_skips += 1;
                            if dedup_skips % 30 == 1 {
                                tracing::debug!(
                                    dedup_skips,
                                    "Frame dedup: skipping unchanged frame"
                                );
                            }
                            continue;
                        }
                        prev_frame_hash = frame_hash;
                    } else {
                        prev_frame_hash = 0; // Reset hash on keyframes
                    }

                    // Phase 5b: Backpressure — rate-limit to ~60fps max
                    // Skip non-keyframes if we're sending faster than 60fps
                    // to prevent overwhelming slow WebSocket connections
                    if !is_keyframe && frames_sent > 0 {
                        let elapsed = last_frame_sent_time.elapsed();
                        let expected_interval = std::time::Duration::from_micros(16_667); // ~60fps
                        if elapsed < expected_interval {
                            backpressure_skips += 1;
                            if backpressure_skips % 60 == 1 {
                                tracing::debug!(
                                    backpressure_skips,
                                    "Backpressure: rate-limiting P-frame"
                                );
                            }
                            continue;
                        }
                    }

                    if ws_tx.is_closed() {
                        return Ok(frames_sent);
                    }

                    let envelope = Envelope {
                        id: "f".to_string(),
                        session_id: sid.clone(),
                        timestamp: None,
                        payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                            width: stream_width,
                            height: stream_height,
                            data: nal_unit,
                            sequence: seq,
                            quality,
                            codec: FrameCodec::H264.into(),
                            is_keyframe,
                        })),
                    };

                    let mut buf = Vec::with_capacity(nal_len + 64);
                    if envelope.encode(&mut buf).is_ok() {
                        let buf_len = buf.len();
                        if ws_tx.send(buf).is_err() {
                            return Ok(frames_sent);
                        }
                        frames_sent += 1;
                        last_frame_sent_time = std::time::Instant::now();

                        // Phase 5c: Track throughput for adaptive bitrate
                        bitrate_window_bytes += buf_len as u64;
                        let window_elapsed = bitrate_window_start.elapsed().as_secs_f64();
                        if window_elapsed >= BITRATE_WINDOW_SECS {
                            if let Some(qc) = quality_config {
                                let actual_kbps =
                                    ((bitrate_window_bytes * 8) as f64 / window_elapsed / 1000.0)
                                        as u32;
                                let configured_kbps =
                                    qc.bitrate_kbps.load(std::sync::atomic::Ordering::Relaxed);
                                let effective_kbps = if configured_kbps == 0 {
                                    4000
                                } else {
                                    configured_kbps
                                };

                                // If actual throughput < 80% of configured → reduce bitrate
                                if actual_kbps > 0 && actual_kbps < effective_kbps * 80 / 100 {
                                    let new_bitrate =
                                        (effective_kbps * 90 / 100).max(MIN_BITRATE_KBPS);
                                    qc.bitrate_kbps
                                        .store(new_bitrate, std::sync::atomic::Ordering::Relaxed);
                                    tracing::info!(
                                        actual_kbps,
                                        configured_kbps = effective_kbps,
                                        new_bitrate,
                                        "Adaptive bitrate: reducing (congestion detected)"
                                    );
                                } else if actual_kbps >= effective_kbps * 95 / 100
                                    && effective_kbps < MAX_BITRATE_KBPS
                                {
                                    // Throughput is healthy → increase towards target
                                    let new_bitrate =
                                        (effective_kbps * 115 / 100).min(MAX_BITRATE_KBPS);
                                    qc.bitrate_kbps
                                        .store(new_bitrate, std::sync::atomic::Ordering::Relaxed);
                                    tracing::debug!(
                                        actual_kbps,
                                        new_bitrate,
                                        "Adaptive bitrate: increasing (healthy throughput)"
                                    );
                                }
                            }
                            bitrate_window_start = std::time::Instant::now();
                            bitrate_window_bytes = 0;
                        }

                        if seq % 60 == 0 {
                            tracing::info!(
                                seq,
                                h264_bytes = nal_len,
                                is_keyframe,
                                "Sent H264 frame"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                // Check for watchdog timeout (no data for 10 seconds)
                if last_read_time.elapsed() > std::time::Duration::from_secs(10) {
                    tracing::warn!("H264 reader: no data for 10s, treating as stall");
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "H264 stream stalled",
                    ));
                }
                return Err(e);
            }
        }
    }
}

// ─── Linux: GNOME Mutter D-Bus + GStreamer ──────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    use tokio::sync::mpsc;
    use zbus::Connection;

    const DEFAULT_FPS: u32 = 30;
    const DEFAULT_QUALITY: u32 = 50;

    /// Get the session user UID/GID stored by adopt_graphical_session_env
    fn get_session_user() -> Option<(u32, u32)> {
        let uid: u32 = std::env::var("SC_SESSION_UID").ok()?.parse().ok()?;
        let gid: u32 = std::env::var("SC_SESSION_GID").ok()?.parse().ok()?;
        Some((uid, gid))
    }

    /// Temporarily switch effective UID/GID to the session user.
    /// Returns the original euid/egid so they can be restored.
    fn switch_to_session_user(uid: u32, gid: u32) -> Option<(u32, u32)> {
        let orig_euid = unsafe { libc::geteuid() };
        let orig_egid = unsafe { libc::getegid() };

        // Set GID first (must happen before dropping root UID)
        if unsafe { libc::setegid(gid) } != 0 {
            tracing::warn!("Failed to setegid({})", gid);
            return None;
        }
        if unsafe { libc::seteuid(uid) } != 0 {
            tracing::warn!("Failed to seteuid({})", uid);
            // Restore GID
            unsafe { libc::setegid(orig_egid) };
            return None;
        }
        tracing::debug!("Switched euid/egid to {}/{}", uid, gid);
        Some((orig_euid, orig_egid))
    }

    /// Restore original effective UID/GID.
    fn restore_root(orig_euid: u32, orig_egid: u32) {
        // Restore UID first (need root to change GID back)
        unsafe { libc::seteuid(orig_euid) };
        unsafe { libc::setegid(orig_egid) };
        tracing::debug!("Restored euid/egid to {}/{}", orig_euid, orig_egid);
    }

    struct MutterScreenCast {
        node_id: u32,
        connection: Connection,
        session_path: zbus::zvariant::OwnedObjectPath,
        rd_session_path: zbus::zvariant::OwnedObjectPath,
        stream_path: zbus::zvariant::OwnedObjectPath,
    }

    pub async fn capture_session(
        session_id: String,
        monitor_index: u32,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
        input_handle_tx: tokio::sync::oneshot::Sender<super::MutterInputHandle>,
        quality_config: std::sync::Arc<super::QualityConfig>,
    ) {
        tracing::info!(
            "Starting Mutter ScreenCast capture for session {} (monitor {})",
            session_id,
            monitor_index
        );

        let screencast_info = match request_mutter_screencast().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Mutter ScreenCast D-Bus request failed: {}", e);
                tracing::warn!("Falling back to ScreenCast portal (may show dialog)");
                return;
            }
        };

        let node_id = screencast_info.node_id;
        tracing::info!(
            node_id,
            "Mutter RemoteDesktop+ScreenCast session started silently (no user dialog)"
        );

        let dbus_conn = screencast_info.connection;
        let _session_path = screencast_info.session_path;
        let rd_session_path = screencast_info.rd_session_path;
        let stream_path = screencast_info.stream_path;

        // Send the D-Bus input handle back to the caller so it can
        // create a MutterInputInjector
        let (sw, sh) = super::get_total_screen_size();
        let _ = input_handle_tx.send(super::MutterInputHandle {
            connection: dbus_conn.clone(),
            rd_session_path: rd_session_path.clone(),
            stream_path: stream_path.clone(),
            screen_width: sw,
            screen_height: sh,
        });

        // IMPORTANT: We must stop GStreamer BEFORE stopping the Mutter session.
        // If we stop Mutter first, PipeWire destroys the ScreenCast node and
        // reroutes pipewiresrc to the default video source (webcam) — causing
        // webcam activation and privacy violation.
        //
        // Flow: shutdown_rx fires → ws_tx closes → GStreamer sees closed channel
        // and exits → spawn_blocking completes → THEN we stop Mutter.
        let sid = session_id.clone();
        let eff_fps = quality_config.effective_fps(DEFAULT_FPS);
        let eff_quality = quality_config.effective_quality(DEFAULT_QUALITY);
        let qc = quality_config.clone();

        // Spawn the GStreamer capture in a blocking thread
        let mut gst_handle = tokio::task::spawn_blocking(move || {
            run_gst_capture(node_id, eff_fps, eff_quality, &sid, ws_tx, qc);
        });

        // Wait for shutdown signal (ws_tx closes when session ends, which
        // causes run_gst_capture to exit naturally via ws_tx.is_closed()).
        // We also wait for the GStreamer task to finish.
        tokio::select! {
            _ = shutdown_rx => {
                tracing::info!("Shutdown signal received, waiting for GStreamer to exit...");
                // Give GStreamer up to 3 seconds to exit cleanly
                match tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    gst_handle,
                ).await {
                    Ok(Ok(())) => tracing::info!("GStreamer capture exited cleanly"),
                    Ok(Err(e)) => tracing::error!("GStreamer capture thread panicked: {}", e),
                    Err(_) => tracing::warn!("GStreamer did not exit in 3s after shutdown signal"),
                }
            }
            result = &mut gst_handle => {
                // GStreamer exited on its own (e.g., pipeline error, ws closed)
                if let Err(e) = result {
                    tracing::error!("GStreamer capture thread panicked: {}", e);
                }
            }
        }

        // NOW it's safe to stop Mutter — GStreamer is no longer using the PipeWire node
        tracing::info!("Stopping Mutter RemoteDesktop D-Bus session...");
        let keeper_conn = dbus_conn.clone();
        let keeper_rd_path = rd_session_path.clone();
        match keeper_conn
            .call_method(
                Some("org.gnome.Mutter.RemoteDesktop"),
                keeper_rd_path.as_ref(),
                Some("org.gnome.Mutter.RemoteDesktop.Session"),
                "Stop",
                &(),
            )
            .await
        {
            Ok(_) => tracing::info!("Mutter RemoteDesktop session stopped via D-Bus"),
            Err(e) => tracing::warn!("Failed to stop Mutter RemoteDesktop session: {}", e),
        }
    }

    async fn request_mutter_screencast() -> anyhow::Result<MutterScreenCast> {
        // When running as root, we must temporarily switch to the session
        // user's UID/GID because the D-Bus session bus rejects connections
        // from UIDs other than the owning user. We keep the session user's
        // euid for ALL D-Bus method calls (not just the initial connection)
        // because Mutter validates the caller's process identity on each call.
        let switched = get_session_user().and_then(|(uid, gid)| {
            tracing::info!(
                "Switching to session user uid={} gid={} for D-Bus",
                uid,
                gid
            );
            switch_to_session_user(uid, gid)
        });

        // Run all D-Bus work inside a block so we can always restore root
        let result = request_mutter_screencast_inner().await;

        // Always restore root after D-Bus work, whether it succeeded or failed
        if let Some((eu, eg)) = switched {
            restore_root(eu, eg);
        }

        result
    }

    async fn request_mutter_screencast_inner() -> anyhow::Result<MutterScreenCast> {
        let connection = Connection::session().await?;

        // ── Step 1: Create a RemoteDesktop session ──
        // This provides input injection via D-Bus without going through
        // the portal (no consent dialog).
        let rd_session_path: zbus::zvariant::OwnedObjectPath = connection
            .call_method(
                Some("org.gnome.Mutter.RemoteDesktop"),
                "/org/gnome/Mutter/RemoteDesktop",
                Some("org.gnome.Mutter.RemoteDesktop"),
                "CreateSession",
                &(),
            )
            .await?
            .body()
            .deserialize()?;

        tracing::info!(rd_session_path = %rd_session_path, "Mutter RemoteDesktop session created");

        // ── Step 2: Create a ScreenCast session paired to the RemoteDesktop session ──
        // The 'remote-desktop-session-id' property links the two sessions.
        // IMPORTANT: The session ID is a random UUID stored internally by Mutter,
        // NOT the D-Bus object path suffix. We must read the "SessionId" property
        // from the RemoteDesktop session object to get the correct UUID.
        let rd_session_id_val: zbus::zvariant::OwnedValue = connection
            .call_method(
                Some("org.gnome.Mutter.RemoteDesktop"),
                rd_session_path.as_ref(),
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.gnome.Mutter.RemoteDesktop.Session", "SessionId"),
            )
            .await?
            .body()
            .deserialize()?;
        let rd_session_id: String = rd_session_id_val
            .try_into()
            .map_err(|e| anyhow::anyhow!("Failed to extract SessionId string: {}", e))?;

        tracing::info!(rd_session_id = %rd_session_id, "Got RemoteDesktop SessionId UUID");

        let mut session_props = std::collections::HashMap::<String, zbus::zvariant::Value>::new();
        session_props.insert(
            "disable-animations".to_string(),
            zbus::zvariant::Value::Bool(true),
        );
        session_props.insert(
            "remote-desktop-session-id".to_string(),
            zbus::zvariant::Value::Str(rd_session_id.clone().into()),
        );

        let sc_session_path: zbus::zvariant::OwnedObjectPath = connection
            .call_method(
                Some("org.gnome.Mutter.ScreenCast"),
                "/org/gnome/Mutter/ScreenCast",
                Some("org.gnome.Mutter.ScreenCast"),
                "CreateSession",
                &(session_props,),
            )
            .await?
            .body()
            .deserialize()?;

        tracing::info!(
            sc_session_path = %sc_session_path,
            rd_session_id = %rd_session_id,
            "Mutter ScreenCast session created (paired with RemoteDesktop)"
        );

        // Stream properties — suppress recording indicator and embed cursor
        let mut stream_props = std::collections::HashMap::<String, zbus::zvariant::Value>::new();
        stream_props.insert(
            "is-recording".to_string(),
            zbus::zvariant::Value::Bool(false),
        );
        stream_props.insert(
            "cursor-mode".to_string(),
            zbus::zvariant::Value::U32(1), // 1 = embedded in stream
        );

        let stream_path: zbus::zvariant::OwnedObjectPath = connection
            .call_method(
                Some("org.gnome.Mutter.ScreenCast"),
                sc_session_path.as_ref(),
                Some("org.gnome.Mutter.ScreenCast.Session"),
                "RecordMonitor",
                &("", stream_props),
            )
            .await?
            .body()
            .deserialize()?;

        tracing::info!(stream_path = %stream_path, "Mutter ScreenCast stream created");

        // ── Step 3: Listen for PipeWireStreamAdded, then Start the RemoteDesktop session ──
        use tokio::sync::oneshot;
        let (node_tx, node_rx) = oneshot::channel::<u32>();

        let signal_conn = connection.clone();
        let _signal_stream_path = stream_path.clone();
        let signal_task = tokio::spawn(async move {
            use futures_util::StreamExt;

            let mut stream = zbus::MessageStream::from(&signal_conn);

            while let Some(msg) = stream.next().await {
                if let Ok(msg) = msg {
                    let header = msg.header();

                    // Only match actual Signal messages (not method calls/returns)
                    if header.message_type() != zbus::message::Type::Signal {
                        continue;
                    }

                    let member_match =
                        header.member().map(|m| m.as_str()) == Some("PipeWireStreamAdded");
                    let iface_match = header.interface().map(|i| i.as_str())
                        == Some("org.gnome.Mutter.ScreenCast.Stream");

                    if member_match && iface_match {
                        tracing::debug!(
                            path = ?header.path(),
                            interface = ?header.interface(),
                            "Received PipeWireStreamAdded signal"
                        );
                        // The signal body is (u) — a single u32 node_id
                        if let Ok(node_id) = msg.body().deserialize::<u32>() {
                            tracing::info!(
                                node_id,
                                "Parsed PipeWire node_id from PipeWireStreamAdded signal"
                            );
                            let _ = node_tx.send(node_id);
                            return;
                        } else {
                            tracing::warn!(
                                "Failed to deserialize PipeWireStreamAdded body as u32, \
                                 trying (u) tuple...",
                            );
                            // Try as a tuple (u,) in case zbus wraps it
                            if let Ok((node_id,)) = msg.body().deserialize::<(u32,)>() {
                                tracing::info!(
                                    node_id,
                                    "Parsed PipeWire node_id from tuple variant"
                                );
                                let _ = node_tx.send(node_id);
                                return;
                            }
                        }
                    } else if member_match {
                        // Log non-matching PipeWireStreamAdded from wrong interface
                        tracing::debug!(
                            interface = ?header.interface(),
                            path = ?header.path(),
                            "Ignoring PipeWireStreamAdded from non-ScreenCast interface"
                        );
                    }
                }
            }
        });

        // Start the RemoteDesktop session — this also starts the paired ScreenCast session
        connection
            .call_method(
                Some("org.gnome.Mutter.RemoteDesktop"),
                rd_session_path.as_ref(),
                Some("org.gnome.Mutter.RemoteDesktop.Session"),
                "Start",
                &(),
            )
            .await?;

        tracing::info!(
            "Mutter RemoteDesktop+ScreenCast session started, waiting for PipeWire node_id..."
        );

        let node_id = tokio::time::timeout(std::time::Duration::from_secs(5), node_rx)
            .await
            .map_err(|_| anyhow::anyhow!("Timeout waiting for PipeWireStreamAdded signal"))?
            .map_err(|_| anyhow::anyhow!("Signal task dropped"))?;

        signal_task.abort();

        tracing::info!(node_id, "Mutter ScreenCast PipeWire node ready");

        Ok(MutterScreenCast {
            node_id,
            connection,
            session_path: sc_session_path,
            rd_session_path,
            stream_path,
        })
    }

    fn run_gst_capture(
        node_id: u32,
        fps: u32,
        quality: u32,
        session_id: &str,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
        quality_config: std::sync::Arc<super::QualityConfig>,
    ) {
        let sid = session_id.to_string();
        let frame_seq = std::sync::atomic::AtomicU32::new(0);
        let default_bitrate: u32 = 4000;
        let mut current_bitrate = quality_config.effective_bitrate(default_bitrate);

        // Auto-restart state: retry up to 3 times with exponential backoff
        const MAX_RETRIES: u32 = 3;
        let mut retry_count: u32 = 0;

        // Outer loop: restarts encoder when bitrate changes or pipeline fails
        'encoder_loop: loop {
            if ws_tx.is_closed() {
                tracing::info!("WS channel closed before starting GStreamer pipeline");
                return;
            }

            let bitrate = current_bitrate;

            // Pipeline definitions
            // Key: always-copy=true on pipewiresrc forces system-memory copies,
            //      avoiding DMABuf renegotiation that causes not-negotiated errors
            //      (learned from RustDesk's PipeWire implementation)
            // Key: videoconvert handles pixel format conversion after pipewiresrc
            //      copies buffers to system memory
            let vaapi_pipeline = format!(
                "pipewiresrc path={node_id} do-timestamp=true keepalive-time=1000 always-copy=true \
                 ! queue max-size-buffers=3 leaky=downstream \
                 ! videoconvert \
                 ! video/x-raw,format=NV12 \
                 ! videorate \
                 ! video/x-raw,framerate={fps}/1 \
                 ! vaapih264enc rate-control=cbr bitrate={bitrate} keyframe-period=60 \
                 ! video/x-h264,stream-format=byte-stream,alignment=au \
                 ! fdsink fd=1",
            );

            let x264_pipeline = format!(
                "pipewiresrc path={node_id} do-timestamp=true keepalive-time=1000 always-copy=true \
                 ! queue max-size-buffers=3 leaky=downstream \
                 ! videoconvert \
                 ! video/x-raw,format=I420 \
                 ! videorate \
                 ! video/x-raw,framerate={fps}/1 \
                 ! x264enc tune=zerolatency speed-preset=ultrafast \
                   bitrate={bitrate} key-int-max=60 bframes=0 \
                   option-string=\"repeat-headers=1:annexb=1\" \
                 ! video/x-h264,stream-format=byte-stream,alignment=au \
                 ! fdsink fd=1",
            );

            let openh264_pipeline = format!(
                "pipewiresrc path={node_id} do-timestamp=true keepalive-time=1000 always-copy=true \
                 ! queue max-size-buffers=3 leaky=downstream \
                 ! videoconvert \
                 ! video/x-raw,format=I420 \
                 ! videorate \
                 ! video/x-raw,framerate={fps}/1 \
                 ! openh264enc bitrate={bitrate_bps} complexity=low \
                   gop-size=60 rate-control=bitrate \
                 ! video/x-h264,stream-format=byte-stream,alignment=au \
                 ! fdsink fd=1",
                bitrate_bps = bitrate * 1000, // openh264enc uses bps, not kbps
            );

            // Helper: configure env + pre_exec for GStreamer subprocess
            let session_user = get_session_user();
            let setup_cmd = |cmd: &mut Command| {
                // Set environment variables needed for PipeWire and VAAPI access
                if let Some((uid, _gid)) = session_user {
                    let runtime_dir = format!("/run/user/{}", uid);
                    cmd.env("XDG_RUNTIME_DIR", &runtime_dir);
                    cmd.env("PIPEWIRE_RUNTIME_DIR", &runtime_dir);
                    // Tell VAAPI to use DRM render node directly (no X11 display needed)
                    cmd.env("GST_VAAPI_DRM_DEVICE", "/dev/dri/renderD128");
                    // Look up and set HOME directory
                    if let Ok(passwd) = std::fs::read_to_string("/etc/passwd") {
                        if let Some(home) = passwd
                            .lines()
                            .find(|line| {
                                line.split(':')
                                    .nth(2)
                                    .and_then(|u| u.parse::<u32>().ok())
                                    .map_or(false, |u| u == uid)
                            })
                            .and_then(|line| line.split(':').nth(5))
                        {
                            cmd.env("HOME", home);
                        }
                    }
                }
                // Run GStreamer as the session user so it can access PipeWire
                // Also set PR_SET_PDEATHSIG so GStreamer dies when the agent dies
                // (prevents orphaned processes holding webcam/PipeWire resources)
                if let Some((uid, gid)) = session_user {
                    unsafe {
                        cmd.pre_exec(move || {
                            // Kill this process when parent dies
                            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                            libc::setgid(gid);
                            libc::setuid(uid);
                            Ok(())
                        });
                    }
                } else {
                    unsafe {
                        cmd.pre_exec(move || {
                            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                            Ok(())
                        });
                    }
                }
            };

            // Try encoders in priority order: VAAPI (HW) → x264 (SW) → openh264 (SW light)
            let pipelines: Vec<(&str, &str)> = vec![
                ("vaapih264enc", &vaapi_pipeline),
                ("x264enc", &x264_pipeline),
                ("openh264enc", &openh264_pipeline),
            ];

            let mut child: Option<std::process::Child> = None;
            let mut pipeline_name = "";

            for (name, pipeline) in &pipelines {
                let mut cmd = Command::new("sh");
                cmd.args(["-c", &format!("gst-launch-1.0 -q -e {}", pipeline)])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                setup_cmd(&mut cmd);

                match cmd.spawn() {
                    Ok(mut c) => {
                        tracing::info!("Trying {} encoder (bitrate={}kbps)...", name, bitrate);

                        // Wait briefly to see if GStreamer pipeline starts successfully.
                        let start = std::time::Instant::now();
                        let timeout = std::time::Duration::from_secs(3);
                        let mut got_data = false;

                        loop {
                            if start.elapsed() >= timeout {
                                break;
                            }
                            match c.try_wait() {
                                Ok(Some(status)) => {
                                    tracing::warn!(
                                        "{} pipeline exited early with status: {}",
                                        name,
                                        status
                                    );
                                    break;
                                }
                                Ok(None) => {
                                    std::thread::sleep(std::time::Duration::from_millis(500));
                                    if start.elapsed() >= std::time::Duration::from_millis(1500) {
                                        got_data = true;
                                        break;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("{} try_wait error: {}", name, e);
                                    break;
                                }
                            }
                        }

                        if got_data || c.try_wait().ok().flatten().is_none() {
                            tracing::info!("Using {} encoder", name);
                            pipeline_name = name;
                            child = Some(c);
                            break;
                        } else {
                            tracing::warn!("{} encoder failed, trying next...", name);
                            let _ = c.kill();
                            let _ = c.wait();
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to spawn {} pipeline: {}", name, e);
                    }
                }
            }

            let mut child = match child {
                Some(c) => c,
                None => {
                    tracing::error!("All GStreamer encoder pipelines failed");
                    return;
                }
            };

            tracing::info!(
                "Launched GStreamer H264 pipeline (encoder: {}, bitrate: {}kbps, retry: {}/{})",
                pipeline_name,
                bitrate,
                retry_count,
                MAX_RETRIES
            );

            let mut stdout = match child.stdout.take() {
                Some(s) => s,
                None => {
                    tracing::error!("Failed to capture GStreamer stdout");
                    let _ = child.kill();
                    return;
                }
            };

            // Log GStreamer stderr in a separate thread for diagnostics
            if let Some(stderr) = child.stderr.take() {
                std::thread::spawn(move || {
                    use std::io::BufRead;
                    let reader = std::io::BufReader::new(stderr);
                    for line in reader.lines() {
                        match line {
                            Ok(l) => tracing::warn!("GStreamer stderr: {}", l),
                            Err(_) => break,
                        }
                    }
                });
            }

            tracing::info!("GStreamer pipeline started, reading H264 stream via shared reader");

            // Bitrate monitoring: check every second, kill GStreamer if bitrate changes ≥20%
            let bitrate_changed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let bitrate_changed_clone = bitrate_changed.clone();
            let qc_clone = quality_config.clone();
            let child_id = child.id();
            let ws_closed_check = ws_tx.clone();
            let monitor_thread = std::thread::spawn(move || {
                let mut check_bitrate = bitrate;
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    if ws_closed_check.is_closed() {
                        break;
                    }
                    let new_bitrate = qc_clone.effective_bitrate(default_bitrate);
                    let ratio = if new_bitrate > check_bitrate {
                        new_bitrate as f64 / check_bitrate as f64
                    } else {
                        check_bitrate as f64 / new_bitrate as f64
                    };
                    if ratio >= 1.2 {
                        tracing::info!(
                            old_bitrate = check_bitrate,
                            new_bitrate,
                            "Bitrate changed ≥20%, killing GStreamer for restart"
                        );
                        bitrate_changed_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                        // Kill the GStreamer process to trigger read_h264_stream EOF
                        unsafe {
                            libc::kill(child_id as i32, libc::SIGTERM);
                        }
                        break;
                    }
                    check_bitrate = new_bitrate;
                }
            });

            // Use the shared H264 reader (handles NAL parsing, SPS resolution, WS sending)
            let reader_result = super::read_h264_stream(
                &mut stdout,
                &sid,
                &ws_tx,
                &frame_seq,
                quality,
                Some(&quality_config),
            );

            // Wait for the monitor thread to finish
            let _ = monitor_thread.join();

            // Cleanup the GStreamer process
            let _ = child.kill();
            let _ = child.wait();

            // Check if this was a bitrate-triggered restart
            let was_bitrate_restart = bitrate_changed.load(std::sync::atomic::Ordering::Relaxed);

            match reader_result {
                Ok(frames_sent) => {
                    tracing::info!(frames_sent, "GStreamer capture ended for session {}", sid);

                    // Bitrate-triggered restart — update bitrate and loop
                    if was_bitrate_restart {
                        current_bitrate = quality_config.effective_bitrate(default_bitrate);
                        retry_count = 0; // Not a failure, reset retry counter
                        tracing::info!(
                            new_bitrate = current_bitrate,
                            "Restarting encoder with new bitrate"
                        );
                        continue 'encoder_loop;
                    }

                    // If we got some frames before stopping, this was a normal exit
                    // (or the WS channel closed). Don't retry.
                    if frames_sent > 0 || ws_tx.is_closed() {
                        break;
                    }

                    // 0 frames = pipeline started but produced nothing.
                    // Retry with backoff.
                    retry_count += 1;
                    if retry_count > MAX_RETRIES {
                        tracing::error!(
                            "GStreamer pipeline produced 0 frames after {} retries, giving up",
                            MAX_RETRIES
                        );
                        break;
                    }

                    let backoff_secs = 1u64 << retry_count.min(4); // 2, 4, 8...
                    tracing::warn!(
                        retry = retry_count,
                        backoff_secs,
                        "GStreamer pipeline produced 0 frames, retrying..."
                    );
                    std::thread::sleep(std::time::Duration::from_secs(backoff_secs));
                    continue 'encoder_loop;
                }
                Err(e) => {
                    if ws_tx.is_closed() {
                        tracing::info!("WS closed during H264 read, stopping");
                        break;
                    }

                    // Pipeline error — retry with backoff
                    retry_count += 1;
                    if retry_count > MAX_RETRIES {
                        tracing::error!(
                            "GStreamer pipeline failed after {} retries: {}",
                            MAX_RETRIES,
                            e
                        );
                        break;
                    }

                    let backoff_secs = 1u64 << retry_count.min(4);
                    tracing::warn!(
                        retry = retry_count,
                        backoff_secs,
                        error = %e,
                        "GStreamer pipeline failed, retrying..."
                    );
                    std::thread::sleep(std::time::Duration::from_secs(backoff_secs));

                    // Update bitrate in case it changed while we were running
                    current_bitrate = quality_config.effective_bitrate(default_bitrate);
                    continue 'encoder_loop;
                }
            }
        }

        tracing::info!("GStreamer capture fully stopped for session {}", sid);
    }
}

// ─── macOS: ScreenCaptureKit capture ────────────────────────────────

#[cfg(target_os = "macos")]
mod macos {
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU32, Ordering};

    use image::codecs::jpeg::JpegEncoder;
    use image::ImageEncoder;
    use prost::Message as ProstMessage;
    use tokio::sync::mpsc;

    use sc_protocol::{envelope, DesktopFrame, Envelope, FrameCodec};

    const DEFAULT_FPS: u32 = 30;
    const DEFAULT_QUALITY: u8 = 50;
    /// Buffer capacity for the async frame queue — drop oldest when full
    const FRAME_BUFFER_CAPACITY: usize = 2;

    // CoreGraphics FFI bindings (used for fallback + display enumeration)
    type CGDirectDisplayID = u32;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> CGDirectDisplayID;
        #[allow(dead_code)]
        fn CGDisplayPixelsWide(display: CGDirectDisplayID) -> usize;
        #[allow(dead_code)]
        fn CGDisplayPixelsHigh(display: CGDirectDisplayID) -> usize;
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGGetActiveDisplayList(
            max_displays: u32,
            active_displays: *mut CGDirectDisplayID,
            display_count: *mut u32,
        ) -> i32;
    }

    /// Enumerate connected macOS monitors via CoreGraphics.
    pub fn enumerate_monitors_cg() -> Vec<super::MonitorInfo> {
        let mut monitors = Vec::new();
        let mut display_ids: [CGDirectDisplayID; 16] = [0; 16];
        let mut count: u32 = 0;

        let result = unsafe { CGGetActiveDisplayList(16, display_ids.as_mut_ptr(), &mut count) };

        if result != 0 || count == 0 {
            return monitors;
        }

        let main_id = unsafe { CGMainDisplayID() };

        for i in 0..count as usize {
            let display_id = display_ids[i];
            let width = unsafe { CGDisplayPixelsWide(display_id) } as u32;
            let height = unsafe { CGDisplayPixelsHigh(display_id) } as u32;

            monitors.push(super::MonitorInfo {
                index: i as u32,
                name: format!("Display {}", display_id),
                width,
                height,
                primary: display_id == main_id,
                x: 0, // CoreGraphics bounds would require more FFI
                y: 0,
                scale_factor: 1.0,
            });
        }

        monitors
    }

    /// Convert BGRA pixel data to RGB, using fast chunked iteration.
    /// Pre-allocated `rgb_out` buffer is resized to fit.
    fn bgra_to_rgb(bgra: &[u8], rgb_out: &mut Vec<u8>) {
        let pixel_count = bgra.len() / 4;
        rgb_out.clear();
        rgb_out.reserve(pixel_count * 3);
        for chunk in bgra.chunks_exact(4) {
            rgb_out.push(chunk[2]); // R (BGRA: B=0, G=1, R=2, A=3)
            rgb_out.push(chunk[1]); // G
            rgb_out.push(chunk[0]); // B
        }
    }

    /// Primary capture path: ScreenCaptureKit + FFmpeg VideoToolbox H264 encoding.
    /// Feeds raw BGRA frames from SCK to FFmpeg's hardware H264 encoder via stdin,
    /// reads Annex B H264 stream from stdout in a background thread.
    /// Falls back to JPEG encoding if FFmpeg is not available.
    #[cfg(feature = "sck")]
    async fn capture_with_screencapturekit(
        session_id: &str,
        _monitor_index: u32,
        ws_tx: &mpsc::UnboundedSender<Vec<u8>>,
        quality_config: &std::sync::Arc<super::QualityConfig>,
    ) -> Result<(), String> {
        use screencapturekit::async_api::{AsyncSCShareableContent, AsyncSCStream};
        use screencapturekit::cv::CVPixelBufferLockFlags;
        use screencapturekit::prelude::PixelFormat;
        use screencapturekit::stream::configuration::SCStreamConfiguration;
        use screencapturekit::stream::content_filter::SCContentFilter;
        use screencapturekit::stream::output_type::SCStreamOutputType;
        use std::io::Write;
        use std::process::{Command, Stdio};

        let sck_timeout = std::time::Duration::from_secs(5);

        // Discover available displays (with timeout — can hang in daemon context)
        tracing::info!("SCK: requesting shareable content...");
        let content = match tokio::time::timeout(sck_timeout, AsyncSCShareableContent::get()).await
        {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => return Err(format!("Failed to get shareable content: {}", e)),
            Err(_) => return Err("Timed out getting shareable content (5s)".to_string()),
        };

        let displays = content.displays();
        tracing::info!("SCK: found {} display(s)", displays.len());
        if displays.is_empty() {
            return Err("No displays found via ScreenCaptureKit".to_string());
        }

        // Select display (use first one; monitor_index for future multi-monitor)
        let display = &displays[0];
        let disp_width = display.width();
        let disp_height = display.height();

        let disp_id = display.display_id();
        tracing::info!(
            display_id = disp_id,
            width = disp_width,
            height = disp_height,
            "SCK: display selected"
        );

        // Configure capture stream
        tracing::info!("SCK: creating content filter and stream config...");
        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build();

        let config = SCStreamConfiguration::new()
            .with_width(disp_width)
            .with_height(disp_height)
            .with_pixel_format(PixelFormat::BGRA)
            .with_shows_cursor(true);

        // Create async stream with small buffer (backpressure: drop oldest)
        tracing::info!("SCK: creating async stream...");
        let stream = AsyncSCStream::new(
            &filter,
            &config,
            FRAME_BUFFER_CAPACITY,
            SCStreamOutputType::Screen,
        );

        tracing::info!("SCK: starting capture...");
        stream
            .start_capture()
            .map_err(|e| format!("Failed to start ScreenCaptureKit capture: {}", e))?;

        // Validate first frame to confirm SCK is actually producing output
        tracing::info!("SCK: waiting for first frame (5s timeout)...");
        match tokio::time::timeout(sck_timeout, stream.next()).await {
            Ok(Some(_)) => {
                tracing::info!("SCK: first frame received — stream is working");
            }
            Ok(None) => {
                let _ = stream.stop_capture();
                return Err("SCK stream closed before producing first frame".to_string());
            }
            Err(_) => {
                let _ = stream.stop_capture();
                return Err("Timed out waiting for first frame from SCK (5s)".to_string());
            }
        }

        tracing::info!(
            "ScreenCaptureKit capture started for session {} at {}x{} (target {} FPS)",
            session_id,
            disp_width,
            disp_height,
            DEFAULT_FPS
        );

        // Resolve FFmpeg path (system PATH → cached → auto-download)
        let ffmpeg_path = crate::ffmpeg::ensure_ffmpeg().await;

        // Try to spawn FFmpeg for hardware H264 encoding
        let default_bitrate: u32 = 4000;
        let mut current_bitrate = quality_config.effective_bitrate(default_bitrate);

        // Helper to spawn FFmpeg with a given bitrate
        fn spawn_ffmpeg(
            ffmpeg_path: &std::path::Path,
            disp_width: u32,
            disp_height: u32,
            bitrate: u32,
        ) -> std::io::Result<std::process::Child> {
            let bitrate_str = format!("{}k", bitrate);
            let maxrate_str = format!("{}k", bitrate * 3 / 2);
            Command::new(ffmpeg_path)
                .args([
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "bgra",
                    "-s",
                    &format!("{}x{}", disp_width, disp_height),
                    "-r",
                    &format!("{}", super::DEFAULT_FPS),
                    "-i",
                    "pipe:0",
                    "-c:v",
                    "h264_videotoolbox",
                    "-realtime",
                    "1",
                    "-prio_speed",
                    "1",
                    "-b:v",
                    &bitrate_str,
                    "-maxrate",
                    &maxrate_str,
                    "-g",
                    "60", // keyframe every 60 frames
                    "-bf",
                    "0", // no B-frames for lowest latency
                    "-flags",
                    "+global_header",
                    "-bsf:v",
                    "dump_extra", // ensure SPS/PPS in stream
                    "-f",
                    "h264", // Annex B output
                    "-",    // stdout
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
        }

        let ffmpeg_result = match ffmpeg_path {
            Some(ref p) => spawn_ffmpeg(p, disp_width, disp_height, current_bitrate),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "FFmpeg not available",
            )),
        };

        let use_h264 = ffmpeg_result.is_ok();

        if use_h264 {
            tracing::info!(
                "FFmpeg h264_videotoolbox encoder started — using H264 (bitrate={}kbps)",
                current_bitrate
            );

            let mut ffmpeg = ffmpeg_result.unwrap();
            let mut ffmpeg_stdin = ffmpeg.stdin.take().unwrap();
            let ffmpeg_stdout = ffmpeg.stdout.take().unwrap();

            // Background thread: read H264 NAL units from FFmpeg stdout and send via WS
            let ws_tx_h264 = ws_tx.clone();
            let sid = session_id.to_string();
            let frame_seq = std::sync::Arc::new(AtomicU32::new(0));
            let frame_seq_reader = frame_seq.clone();

            let mut reader_handle = std::thread::spawn(move || {
                use std::io::Read;
                let mut stdout = ffmpeg_stdout;
                let mut buffer = Vec::with_capacity(512 * 1024);
                let mut read_buf = [0u8; 65536];

                loop {
                    if ws_tx_h264.is_closed() {
                        break;
                    }
                    match stdout.read(&mut read_buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            buffer.extend_from_slice(&read_buf[..n]);
                            while let Some(nal_unit) = super::extract_h264_access_unit(&mut buffer)
                            {
                                let seq = frame_seq_reader.fetch_add(1, Ordering::Relaxed);
                                let nal_len = nal_unit.len();
                                let is_keyframe = super::is_h264_keyframe(&nal_unit);

                                let envelope = Envelope {
                                    id: "f".to_string(),
                                    session_id: sid.clone(),
                                    timestamp: None,
                                    payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                                        width: disp_width,
                                        height: disp_height,
                                        data: nal_unit,
                                        sequence: seq,
                                        quality: 0,
                                        codec: FrameCodec::H264.into(),
                                        is_keyframe,
                                    })),
                                };

                                let mut buf = Vec::with_capacity(nal_len + 64);
                                if envelope.encode(&mut buf).is_ok() {
                                    if ws_tx_h264.send(buf).is_err() {
                                        return;
                                    }
                                    if seq % 60 == 0 {
                                        tracing::info!(
                                            seq,
                                            h264_bytes = nal_len,
                                            is_keyframe,
                                            "Sent H264 frame (macOS SCK)"
                                        );
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            let frame_interval = std::time::Duration::from_millis(1000 / DEFAULT_FPS as u64);
            let mut next_frame_time = tokio::time::Instant::now();
            let mut frames_since_bitrate_check: u32 = 0;

            // Main async loop: feed BGRA frames to FFmpeg stdin
            loop {
                if ws_tx.is_closed() {
                    tracing::info!("WS channel closed, stopping SCK H264 capture");
                    break;
                }

                let sample = match stream.next().await {
                    Some(s) => s,
                    None => {
                        tracing::warn!("ScreenCaptureKit stream ended");
                        break;
                    }
                };

                let now = tokio::time::Instant::now();
                if now < next_frame_time {
                    continue;
                }
                next_frame_time = now + frame_interval;

                let pixel_buffer = match sample.image_buffer() {
                    Some(pb) => pb,
                    None => continue,
                };

                let width = pixel_buffer.width() as u32;
                let height = pixel_buffer.height() as u32;
                if width == 0 || height == 0 {
                    continue;
                }

                let guard = match pixel_buffer.lock(CVPixelBufferLockFlags::READ_ONLY) {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::warn!("Failed to lock pixel buffer: {}", e);
                        continue;
                    }
                };

                let bgra_slice = guard.as_slice();
                let bytes_per_row = pixel_buffer.bytes_per_row();
                let expected_stride = width as usize * 4;

                // Write raw BGRA data to FFmpeg stdin (strip row padding if needed)
                let write_result = if bytes_per_row == expected_stride {
                    ffmpeg_stdin.write_all(bgra_slice)
                } else {
                    // Row padding — write row by row
                    let mut ok = true;
                    for row in 0..height as usize {
                        let row_start = row * bytes_per_row;
                        let row_end = row_start + expected_stride;
                        if row_end <= bgra_slice.len() {
                            if ffmpeg_stdin
                                .write_all(&bgra_slice[row_start..row_end])
                                .is_err()
                            {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        Ok(())
                    } else {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "write failed",
                        ))
                    }
                };

                drop(guard);

                if write_result.is_err() {
                    tracing::warn!("FFmpeg stdin write failed, encoder may have crashed");
                    break;
                }

                // Check for bitrate change every 30 frames
                frames_since_bitrate_check += 1;
                if frames_since_bitrate_check >= 30 {
                    frames_since_bitrate_check = 0;
                    let new_bitrate = quality_config.effective_bitrate(default_bitrate);
                    let ratio = if new_bitrate > current_bitrate {
                        new_bitrate as f64 / current_bitrate as f64
                    } else {
                        current_bitrate as f64 / new_bitrate as f64
                    };
                    if ratio >= 1.2 {
                        if let Some(ref p) = ffmpeg_path {
                            tracing::info!(
                                old_bitrate = current_bitrate,
                                new_bitrate,
                                "macOS: Bitrate changed, restarting FFmpeg encoder"
                            );
                            current_bitrate = new_bitrate;

                            // Kill current FFmpeg and wait for reader thread
                            drop(ffmpeg_stdin);
                            let _ = ffmpeg.kill();
                            let _ = ffmpeg.wait();
                            let _ = reader_handle.join();

                            // Respawn FFmpeg with new bitrate
                            match spawn_ffmpeg(p, disp_width, disp_height, current_bitrate) {
                                Ok(mut new_ffmpeg) => {
                                    ffmpeg_stdin = new_ffmpeg.stdin.take().unwrap();
                                    let new_stdout = new_ffmpeg.stdout.take().unwrap();
                                    ffmpeg = new_ffmpeg;

                                    // Spawn new reader thread
                                    let ws_tx_h264 = ws_tx.clone();
                                    let sid = session_id.to_string();
                                    let fsr = frame_seq.clone();
                                    let dw = disp_width;
                                    let dh = disp_height;
                                    reader_handle = std::thread::spawn(move || {
                                        use std::io::Read;
                                        let mut stdout = new_stdout;
                                        let mut buffer = Vec::with_capacity(512 * 1024);
                                        let mut read_buf = [0u8; 65536];
                                        loop {
                                            if ws_tx_h264.is_closed() {
                                                break;
                                            }
                                            match stdout.read(&mut read_buf) {
                                                Ok(0) => break,
                                                Ok(n) => {
                                                    buffer.extend_from_slice(&read_buf[..n]);
                                                    while let Some(nal_unit) =
                                                        super::extract_h264_access_unit(&mut buffer)
                                                    {
                                                        let seq =
                                                            fsr.fetch_add(1, Ordering::Relaxed);
                                                        let nal_len = nal_unit.len();
                                                        let is_keyframe =
                                                            super::is_h264_keyframe(&nal_unit);
                                                        let envelope = Envelope {
                                                            id: "f".to_string(),
                                                            session_id: sid.clone(),
                                                            timestamp: None,
                                                            payload: Some(
                                                                envelope::Payload::DesktopFrame(
                                                                    DesktopFrame {
                                                                        width: dw,
                                                                        height: dh,
                                                                        data: nal_unit,
                                                                        sequence: seq,
                                                                        quality: 0,
                                                                        codec: FrameCodec::H264
                                                                            .into(),
                                                                        is_keyframe,
                                                                    },
                                                                ),
                                                            ),
                                                        };
                                                        let mut buf =
                                                            Vec::with_capacity(nal_len + 64);
                                                        if envelope.encode(&mut buf).is_ok() {
                                                            if ws_tx_h264.send(buf).is_err() {
                                                                return;
                                                            }
                                                        }
                                                    }
                                                }
                                                Err(_) => break,
                                            }
                                        }
                                    });

                                    tracing::info!(
                                        "FFmpeg restarted with bitrate={}kbps",
                                        current_bitrate
                                    );
                                }
                                Err(e) => {
                                    tracing::error!("Failed to restart FFmpeg: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Clean up FFmpeg
            drop(ffmpeg_stdin); // close stdin to signal EOF
            let _ = ffmpeg.kill();
            let _ = ffmpeg.wait();
            let _ = reader_handle.join();
        } else if let Ok(mut sw_encoder) = crate::encoder::SoftwareEncoder::new(
            disp_width,
            disp_height,
            current_bitrate,
            DEFAULT_FPS,
        ) {
            // Phase 6b: OpenH264 in-process encoder fallback (no FFmpeg required)
            tracing::info!(
                "FFmpeg not available, using OpenH264 software encoder (bitrate={}kbps)",
                current_bitrate
            );

            let frame_seq = AtomicU32::new(0);
            let frame_interval = std::time::Duration::from_millis(1000 / DEFAULT_FPS as u64);
            let mut next_frame_time = tokio::time::Instant::now();
            let mut compact_bgra: Vec<u8> =
                Vec::with_capacity((disp_width * disp_height * 4) as usize);

            loop {
                if ws_tx.is_closed() {
                    tracing::info!("WS channel closed, stopping OpenH264 capture");
                    break;
                }

                let sample = match stream.next().await {
                    Some(s) => s,
                    None => {
                        tracing::warn!("ScreenCaptureKit stream ended");
                        break;
                    }
                };

                let now = tokio::time::Instant::now();
                if now < next_frame_time {
                    continue;
                }
                next_frame_time = now + frame_interval;

                let pixel_buffer = match sample.image_buffer() {
                    Some(pb) => pb,
                    None => continue,
                };

                let width = pixel_buffer.width() as u32;
                let height = pixel_buffer.height() as u32;
                if width == 0 || height == 0 {
                    continue;
                }

                let guard = match pixel_buffer.lock(CVPixelBufferLockFlags::READ_ONLY) {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::warn!("Failed to lock pixel buffer: {}", e);
                        continue;
                    }
                };

                let bgra_slice = guard.as_slice();
                let bytes_per_row = pixel_buffer.bytes_per_row();
                let expected_stride = width as usize * 4;

                // Get contiguous BGRA data (strip row padding if needed)
                let bgra_data: &[u8] = if bytes_per_row == expected_stride {
                    bgra_slice
                } else {
                    compact_bgra.clear();
                    compact_bgra.reserve((width * height * 4) as usize);
                    for row in 0..height as usize {
                        let row_start = row * bytes_per_row;
                        let row_end = row_start + expected_stride;
                        if row_end <= bgra_slice.len() {
                            compact_bgra.extend_from_slice(&bgra_slice[row_start..row_end]);
                        }
                    }
                    &compact_bgra
                };

                // Encode BGRA → H264 NAL in-process
                let encoded = match sw_encoder.encode_bgra(bgra_data) {
                    Ok(Some(frame)) => frame,
                    Ok(None) => {
                        // Frame skipped by rate control
                        drop(guard);
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!("OpenH264 encode error: {}", e);
                        drop(guard);
                        continue;
                    }
                };

                drop(guard);

                let seq = frame_seq.fetch_add(1, Ordering::Relaxed);
                let nal_len = encoded.data.len();

                let envelope = Envelope {
                    id: "f".to_string(),
                    session_id: session_id.to_string(),
                    timestamp: None,
                    payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                        width,
                        height,
                        data: encoded.data,
                        sequence: seq,
                        quality: 0,
                        codec: FrameCodec::H264.into(),
                        is_keyframe: encoded.is_keyframe,
                    })),
                };

                let mut buf = Vec::with_capacity(nal_len + 64);
                if envelope.encode(&mut buf).is_ok() {
                    if ws_tx.send(buf).is_err() {
                        tracing::info!("WS channel closed, stopping capture");
                        break;
                    }
                    if seq % 60 == 0 {
                        tracing::info!(
                            seq,
                            h264_bytes = nal_len,
                            is_keyframe = encoded.is_keyframe,
                            "Sent H264 frame (macOS SCK OpenH264)"
                        );
                    }
                }
            }
        } else {
            tracing::warn!("FFmpeg and OpenH264 not available, falling back to JPEG encoding");

            // JPEG fallback — original path
            let frame_seq = AtomicU32::new(0);
            let frame_interval = std::time::Duration::from_millis(1000 / DEFAULT_FPS as u64);
            let mut next_frame_time = tokio::time::Instant::now();
            let mut rgb_data: Vec<u8> = Vec::with_capacity((disp_width * disp_height * 3) as usize);
            let mut jpeg_buf: Vec<u8> = Vec::with_capacity(256 * 1024);

            loop {
                if ws_tx.is_closed() {
                    tracing::info!("WS channel closed, stopping ScreenCaptureKit capture");
                    break;
                }

                let sample = match stream.next().await {
                    Some(s) => s,
                    None => {
                        tracing::warn!("ScreenCaptureKit stream ended");
                        break;
                    }
                };

                let now = tokio::time::Instant::now();
                if now < next_frame_time {
                    continue;
                }
                next_frame_time = now + frame_interval;

                let pixel_buffer = match sample.image_buffer() {
                    Some(pb) => pb,
                    None => continue,
                };

                let width = pixel_buffer.width() as u32;
                let height = pixel_buffer.height() as u32;
                if width == 0 || height == 0 {
                    continue;
                }

                let guard = match pixel_buffer.lock(CVPixelBufferLockFlags::READ_ONLY) {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::warn!("Failed to lock pixel buffer: {}", e);
                        continue;
                    }
                };

                let bgra_slice = guard.as_slice();
                let bytes_per_row = pixel_buffer.bytes_per_row();
                let expected_stride = width as usize * 4;

                if bytes_per_row == expected_stride {
                    bgra_to_rgb(bgra_slice, &mut rgb_data);
                } else {
                    rgb_data.clear();
                    rgb_data.reserve((width * height * 3) as usize);
                    for row in 0..height as usize {
                        let row_start = row * bytes_per_row;
                        let row_end = row_start + expected_stride;
                        if row_end <= bgra_slice.len() {
                            for chunk in bgra_slice[row_start..row_end].chunks_exact(4) {
                                rgb_data.push(chunk[2]);
                                rgb_data.push(chunk[1]);
                                rgb_data.push(chunk[0]);
                            }
                        }
                    }
                }

                drop(guard);

                jpeg_buf.clear();
                let mut cursor = Cursor::new(&mut jpeg_buf);
                let eff_quality = quality_config.effective_quality(DEFAULT_QUALITY as u32) as u8;
                let encoder = JpegEncoder::new_with_quality(&mut cursor, eff_quality);
                if encoder
                    .write_image(&rgb_data, width, height, image::ExtendedColorType::Rgb8)
                    .is_err()
                {
                    tracing::warn!("JPEG encode failed for {}x{} frame", width, height);
                    continue;
                }
                drop(cursor);

                let seq = frame_seq.fetch_add(1, Ordering::Relaxed);
                let jpeg_len = jpeg_buf.len();

                let envelope = Envelope {
                    id: "f".to_string(),
                    session_id: session_id.to_string(),
                    timestamp: None,
                    payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                        width,
                        height,
                        data: jpeg_buf.clone(),
                        sequence: seq,
                        quality: eff_quality as u32,
                        codec: FrameCodec::Jpeg.into(),
                        is_keyframe: true,
                    })),
                };

                let mut buf = Vec::with_capacity(jpeg_len + 64);
                if envelope.encode(&mut buf).is_ok() {
                    if ws_tx.send(buf).is_err() {
                        tracing::info!("WS channel closed, stopping capture");
                        break;
                    }
                    if seq % 60 == 0 {
                        tracing::info!(
                            seq,
                            jpeg_bytes = jpeg_len,
                            width,
                            height,
                            "Sent desktop frame (macOS SCK JPEG fallback)"
                        );
                    }
                }
            }
        }

        let _ = stream.stop_capture();
        tracing::info!(
            "ScreenCaptureKit capture stopped for session {}",
            session_id
        );
        Ok(())
    }

    /// Fallback capture path: CoreGraphics CGDisplayCreateImage polling.
    /// Used if ScreenCaptureKit is unavailable (macOS < 12.3) or fails to init.
    async fn capture_with_coregraphics(
        session_id: &str,
        ws_tx: &mpsc::UnboundedSender<Vec<u8>>,
        quality_config: &std::sync::Arc<super::QualityConfig>,
    ) {
        use std::ffi::c_void;
        use std::io::Write;
        use std::process::{Command, Stdio};

        type CGImageRef = *const c_void;
        type CGColorSpaceRef = *const c_void;
        type CGContextRef = *mut c_void;

        #[repr(C)]
        #[derive(Copy, Clone)]
        struct CGPoint {
            x: f64,
            y: f64,
        }
        #[repr(C)]
        #[derive(Copy, Clone)]
        struct CGSize {
            width: f64,
            height: f64,
        }
        #[repr(C)]
        #[derive(Copy, Clone)]
        struct CGRect {
            origin: CGPoint,
            size: CGSize,
        }

        const BITMAP_INFO_RGBX: u32 = 5;

        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGDisplayCreateImage(display: CGDirectDisplayID) -> CGImageRef;
            fn CGImageGetWidth(image: CGImageRef) -> usize;
            fn CGImageGetHeight(image: CGImageRef) -> usize;
            fn CGColorSpaceCreateDeviceRGB() -> CGColorSpaceRef;
            fn CGColorSpaceRelease(space: CGColorSpaceRef);
            fn CGImageRelease(image: CGImageRef);
            fn CGBitmapContextCreate(
                data: *mut u8,
                width: usize,
                height: usize,
                bits_per_component: usize,
                bytes_per_row: usize,
                space: CGColorSpaceRef,
                bitmap_info: u32,
            ) -> CGContextRef;
            fn CGContextDrawImage(ctx: CGContextRef, rect: CGRect, image: CGImageRef);
            fn CGContextRelease(ctx: CGContextRef);
        }

        let display_id = unsafe { CGMainDisplayID() };
        let frame_interval = std::time::Duration::from_millis(1000 / DEFAULT_FPS as u64);
        let frame_seq = AtomicU32::new(0);

        tracing::info!(
            "Starting CoreGraphics fallback capture for session {}",
            session_id
        );

        // Probe dimensions from first capture to size FFmpeg correctly
        let (init_width, init_height) = {
            let dim_result = tokio::task::spawn_blocking(move || unsafe {
                let cg_image = CGDisplayCreateImage(display_id);
                if cg_image.is_null() {
                    return None;
                }
                let native_w = CGImageGetWidth(cg_image);
                let native_h = CGImageGetHeight(cg_image);
                CGImageRelease(cg_image);
                if native_w == 0 || native_h == 0 {
                    return None;
                }
                const MAX_WIDTH: usize = 1280;
                if native_w > MAX_WIDTH {
                    let scale = MAX_WIDTH as f64 / native_w as f64;
                    Some((MAX_WIDTH as u32, (native_h as f64 * scale) as u32))
                } else {
                    Some((native_w as u32, native_h as u32))
                }
            })
            .await;
            match dim_result {
                Ok(Some(dims)) => dims,
                _ => {
                    tracing::error!("CG: could not determine display dimensions");
                    return;
                }
            }
        };

        // Resolve FFmpeg path (system PATH → cached → auto-download)
        let ffmpeg_path = crate::ffmpeg::ensure_ffmpeg().await;

        // Try to spawn FFmpeg for H264 encoding
        // CG produces RGBX (RGB + padding byte) — FFmpeg input pixel format is "0rgb"
        // (which is XRGB in memory but the bitmap is actually RGBX with AlphaNoneSkipLast)
        let ffmpeg_result = match ffmpeg_path {
            Some(ref p) => Command::new(p)
                .args([
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "0rgb",
                    "-s",
                    &format!("{}x{}", init_width, init_height),
                    "-r",
                    &format!("{}", DEFAULT_FPS),
                    "-i",
                    "pipe:0",
                    "-c:v",
                    "h264_videotoolbox",
                    "-realtime",
                    "1",
                    "-prio_speed",
                    "1",
                    "-b:v",
                    "3000k",
                    "-maxrate",
                    "5000k",
                    "-g",
                    "60",
                    "-bf",
                    "0",
                    "-flags",
                    "+global_header",
                    "-bsf:v",
                    "dump_extra",
                    "-f",
                    "h264",
                    "-",
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn(),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "FFmpeg not available",
            )),
        };

        if let Ok(mut ffmpeg) = ffmpeg_result {
            tracing::info!("CG: FFmpeg h264_videotoolbox started — using H264");

            let mut ffmpeg_stdin = ffmpeg.stdin.take().unwrap();
            let ffmpeg_stdout = ffmpeg.stdout.take().unwrap();

            // Background thread reads H264 from stdout
            let ws_tx_h264 = ws_tx.clone();
            let sid = session_id.to_string();
            let h264_seq = std::sync::Arc::new(AtomicU32::new(0));
            let h264_seq_reader = h264_seq.clone();

            let reader_handle = std::thread::spawn(move || {
                use std::io::Read;
                let mut stdout = ffmpeg_stdout;
                let mut buffer = Vec::with_capacity(256 * 1024);
                let mut read_buf = [0u8; 65536];

                loop {
                    if ws_tx_h264.is_closed() {
                        break;
                    }
                    match stdout.read(&mut read_buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            buffer.extend_from_slice(&read_buf[..n]);
                            while let Some(nal_unit) = super::extract_h264_access_unit(&mut buffer)
                            {
                                let seq = h264_seq_reader.fetch_add(1, Ordering::Relaxed);
                                let nal_len = nal_unit.len();
                                let is_keyframe = super::is_h264_keyframe(&nal_unit);

                                let envelope = Envelope {
                                    id: "f".to_string(),
                                    session_id: sid.clone(),
                                    timestamp: None,
                                    payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                                        width: 0,
                                        height: 0,
                                        data: nal_unit,
                                        sequence: seq,
                                        quality: 0,
                                        codec: FrameCodec::H264.into(),
                                        is_keyframe,
                                    })),
                                };

                                let mut buf = Vec::with_capacity(nal_len + 64);
                                if envelope.encode(&mut buf).is_ok() {
                                    if ws_tx_h264.send(buf).is_err() {
                                        return;
                                    }
                                    if seq % 60 == 0 {
                                        tracing::info!(
                                            seq,
                                            h264_bytes = nal_len,
                                            is_keyframe,
                                            "Sent H264 frame (macOS CG)"
                                        );
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            // Main capture loop: CG screenshot → RGBX → FFmpeg stdin
            loop {
                if ws_tx.is_closed() {
                    break;
                }

                let next_frame_time = tokio::time::Instant::now() + frame_interval;

                let frame_result = tokio::task::spawn_blocking(move || unsafe {
                    let cg_image = CGDisplayCreateImage(display_id);
                    if cg_image.is_null() {
                        return None;
                    }
                    let native_w = CGImageGetWidth(cg_image);
                    let native_h = CGImageGetHeight(cg_image);
                    if native_w == 0 || native_h == 0 {
                        CGImageRelease(cg_image);
                        return None;
                    }

                    const MAX_WIDTH: usize = 1280;
                    let (width, height) = if native_w > MAX_WIDTH {
                        let scale = MAX_WIDTH as f64 / native_w as f64;
                        (MAX_WIDTH, (native_h as f64 * scale) as usize)
                    } else {
                        (native_w, native_h)
                    };
                    let bpr = width * 4;

                    let mut pixel_data = vec![0u8; bpr * height];
                    let cs = CGColorSpaceCreateDeviceRGB();
                    let ctx = CGBitmapContextCreate(
                        pixel_data.as_mut_ptr(),
                        width,
                        height,
                        8,
                        bpr,
                        cs,
                        BITMAP_INFO_RGBX,
                    );
                    if ctx.is_null() {
                        CGColorSpaceRelease(cs);
                        CGImageRelease(cg_image);
                        return None;
                    }
                    let rect = CGRect {
                        origin: CGPoint { x: 0.0, y: 0.0 },
                        size: CGSize {
                            width: width as f64,
                            height: height as f64,
                        },
                    };
                    CGContextDrawImage(ctx, rect, cg_image);
                    CGContextRelease(ctx);
                    CGColorSpaceRelease(cs);
                    CGImageRelease(cg_image);
                    Some(pixel_data)
                })
                .await;

                let rgbx_data = match frame_result {
                    Ok(Some(f)) => f,
                    Ok(None) => {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("CG capture task panicked: {}", e);
                        break;
                    }
                };

                // Write raw RGBX data directly to FFmpeg stdin
                if ffmpeg_stdin.write_all(&rgbx_data).is_err() {
                    tracing::warn!("CG: FFmpeg stdin write failed");
                    break;
                }

                tokio::time::sleep_until(next_frame_time).await;
            }

            // Clean up FFmpeg
            drop(ffmpeg_stdin);
            let _ = ffmpeg.kill();
            let _ = ffmpeg.wait();
            let _ = reader_handle.join();
        } else {
            tracing::warn!("CG: FFmpeg not available, using JPEG fallback");

            // JPEG fallback — original path
            let mut rgb_data: Vec<u8> = Vec::new();
            let mut jpeg_buf: Vec<u8> = Vec::with_capacity(256 * 1024);

            loop {
                if ws_tx.is_closed() {
                    break;
                }

                let next_frame_time = tokio::time::Instant::now() + frame_interval;

                let frame_result = tokio::task::spawn_blocking(move || unsafe {
                    let cg_image = CGDisplayCreateImage(display_id);
                    if cg_image.is_null() {
                        return None;
                    }
                    let native_w = CGImageGetWidth(cg_image);
                    let native_h = CGImageGetHeight(cg_image);
                    if native_w == 0 || native_h == 0 {
                        CGImageRelease(cg_image);
                        return None;
                    }

                    const MAX_WIDTH: usize = 1280;
                    let (width, height) = if native_w > MAX_WIDTH {
                        let scale = MAX_WIDTH as f64 / native_w as f64;
                        (MAX_WIDTH, (native_h as f64 * scale) as usize)
                    } else {
                        (native_w, native_h)
                    };
                    let bpr = width * 4;

                    let mut pixel_data = vec![0u8; bpr * height];
                    let cs = CGColorSpaceCreateDeviceRGB();
                    let ctx = CGBitmapContextCreate(
                        pixel_data.as_mut_ptr(),
                        width,
                        height,
                        8,
                        bpr,
                        cs,
                        BITMAP_INFO_RGBX,
                    );
                    if ctx.is_null() {
                        CGColorSpaceRelease(cs);
                        CGImageRelease(cg_image);
                        return None;
                    }
                    let rect = CGRect {
                        origin: CGPoint { x: 0.0, y: 0.0 },
                        size: CGSize {
                            width: width as f64,
                            height: height as f64,
                        },
                    };
                    CGContextDrawImage(ctx, rect, cg_image);
                    CGContextRelease(ctx);
                    CGColorSpaceRelease(cs);
                    CGImageRelease(cg_image);
                    Some((pixel_data, width as u32, height as u32))
                })
                .await;

                let (rgbx_data, width, height) = match frame_result {
                    Ok(Some(f)) => f,
                    Ok(None) => {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("CG capture task panicked: {}", e);
                        break;
                    }
                };

                // RGBX → RGB
                rgb_data.clear();
                rgb_data.reserve((width * height * 3) as usize);
                for chunk in rgbx_data.chunks_exact(4) {
                    rgb_data.push(chunk[0]);
                    rgb_data.push(chunk[1]);
                    rgb_data.push(chunk[2]);
                }

                jpeg_buf.clear();
                let mut cursor = Cursor::new(&mut jpeg_buf);
                let eff_quality = quality_config.effective_quality(DEFAULT_QUALITY as u32) as u8;
                let encoder = JpegEncoder::new_with_quality(&mut cursor, eff_quality);
                if encoder
                    .write_image(&rgb_data, width, height, image::ExtendedColorType::Rgb8)
                    .is_err()
                {
                    tokio::time::sleep_until(next_frame_time).await;
                    continue;
                }
                drop(cursor);

                let jpeg_data = jpeg_buf.clone();
                let seq = frame_seq.fetch_add(1, Ordering::Relaxed);

                let envelope = Envelope {
                    id: "f".to_string(),
                    session_id: session_id.to_string(),
                    timestamp: None,
                    payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                        width,
                        height,
                        data: jpeg_data,
                        sequence: seq,
                        quality: eff_quality as u32,
                        codec: FrameCodec::Jpeg.into(),
                        is_keyframe: true,
                    })),
                };

                let mut buf = Vec::new();
                if envelope.encode(&mut buf).is_ok() {
                    if ws_tx.send(buf).is_err() {
                        break;
                    }
                    if seq % 60 == 0 {
                        tracing::info!(
                            seq,
                            width,
                            height,
                            "Sent desktop frame (macOS CG JPEG fallback)"
                        );
                    }
                }

                tokio::time::sleep_until(next_frame_time).await;
            }
        }

        tracing::info!(
            "CoreGraphics fallback capture stopped for session {}",
            session_id
        );
    }

    pub async fn capture_session(
        session_id: String,
        monitor_index: u32,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
        quality_config: std::sync::Arc<super::QualityConfig>,
    ) {
        // Pre-flight TCC check
        let has_access = unsafe { CGPreflightScreenCaptureAccess() };
        if has_access {
            tracing::debug!("Screen Recording permission: granted");
        } else {
            tracing::warn!(
                "Screen Recording permission: NOT granted. \
                 Grant permission in System Settings > Privacy & Security > Screen Recording."
            );
        }

        // Wrap the capture work in a future that can be cancelled by shutdown_rx
        let capture_work = async {
            // Try ScreenCaptureKit first (macOS 12.3+), fall back to CoreGraphics
            #[cfg(feature = "sck")]
            {
                tracing::info!(
                    "Attempting ScreenCaptureKit capture for session {}",
                    session_id
                );
                match capture_with_screencapturekit(
                    &session_id,
                    monitor_index,
                    &ws_tx,
                    &quality_config,
                )
                .await
                {
                    Ok(()) => {
                        tracing::info!("ScreenCaptureKit session ended normally: {}", session_id);
                        return;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "ScreenCaptureKit failed ({}), falling back to CoreGraphics",
                            e
                        );
                    }
                }
            }

            #[cfg(not(feature = "sck"))]
            tracing::info!(
                "SCK feature disabled, using CoreGraphics for session {}",
                session_id
            );

            capture_with_coregraphics(&session_id, &ws_tx, &quality_config).await;
        };

        tokio::select! {
            _ = capture_work => {
                tracing::info!("macOS capture ended naturally for session {}", session_id);
            }
            _ = shutdown_rx => {
                tracing::info!("macOS capture received shutdown signal for session {}", session_id);
                // The ws_tx channel being dropped will cause in-flight operations to stop
            }
        }
    }
}

// ─── Windows: DXGI Desktop Duplication (Phase C) ───────────────────

#[cfg(target_os = "windows")]
mod windows {
    use std::io::Cursor;
    use std::io::Write;
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use image::codecs::jpeg::JpegEncoder;
    use image::{ExtendedColorType, ImageEncoder};
    use prost::Message as ProstMessage;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
    use windows_capture::frame::Frame;
    use windows_capture::graphics_capture_api::InternalCaptureControl;
    use windows_capture::monitor::Monitor;
    use windows_capture::settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    };

    use sc_protocol::{envelope, DesktopFrame, Envelope, FrameCodec};

    const DEFAULT_QUALITY: u8 = 60;

    /// State for FFmpeg-based H264 encoding
    struct FfmpegEncoder {
        stdin: ChildStdin,
        process: Child,
        _reader_handle: std::thread::JoinHandle<()>,
    }

    struct CaptureHandler {
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
        session_id: String,
        frame_seq: AtomicU32,
        /// FFmpeg encoder state — None before first frame or if FFmpeg unavailable
        ffmpeg: Option<FfmpegEncoder>,
        /// OpenH264 in-process encoder — fallback when FFmpeg is unavailable
        sw_encoder: Option<crate::encoder::SoftwareEncoder>,
        /// Track whether we've attempted FFmpeg init
        ffmpeg_attempted: bool,
        /// Path to FFmpeg binary
        ffmpeg_path: Option<std::path::PathBuf>,
        /// Shared quality config for dynamic JPEG quality adjustment
        quality_config: Arc<super::QualityConfig>,
        /// Current H264 bitrate in kbps
        current_bitrate: u32,
        /// Frame counter for periodic bitrate checks
        frames_since_bitrate_check: u32,
    }

    impl CaptureHandler {
        /// Try to spawn FFmpeg for H264 encoding. Returns None if unavailable.
        fn try_init_ffmpeg(
            ffmpeg_path: &std::path::Path,
            width: u32,
            height: u32,
            bitrate: u32,
            ws_tx: &mpsc::UnboundedSender<Vec<u8>>,
            session_id: &str,
        ) -> Option<FfmpegEncoder> {
            let bitrate_str = format!("{}k", bitrate);
            let maxrate_str = format!("{}k", bitrate * 3 / 2);
            // Try nvenc -> qsv -> Media Foundation -> libx264
            for encoder in &["h264_nvenc", "h264_qsv", "h264_mf", "libx264"] {
                let result = Command::new(ffmpeg_path)
                    .args([
                        "-f",
                        "rawvideo",
                        "-pix_fmt",
                        "bgra",
                        "-s",
                        &format!("{}x{}", width, height),
                        "-r",
                        "30",
                        "-i",
                        "pipe:0",
                        "-c:v",
                        encoder,
                        "-preset",
                        "ultrafast",
                        "-tune",
                        "zerolatency",
                        "-b:v",
                        &bitrate_str,
                        "-maxrate",
                        &maxrate_str,
                        "-g",
                        "60",
                        "-bf",
                        "0",
                        "-flags",
                        "+global_header",
                        "-bsf:v",
                        "dump_extra",
                        "-f",
                        "h264",
                        "-",
                    ])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn();

                if let Ok(mut child) = result {
                    let stdin = child.stdin.take().unwrap();
                    let stdout = child.stdout.take().unwrap();

                    let ws_tx_reader = ws_tx.clone();
                    let sid = session_id.to_string();
                    let frame_seq = Arc::new(AtomicU32::new(0));
                    let frame_seq_reader = frame_seq.clone();

                    let reader_handle = std::thread::spawn(move || {
                        use std::io::Read;
                        let mut stdout = stdout;
                        let mut buffer = Vec::with_capacity(512 * 1024);
                        let mut read_buf = [0u8; 65536];

                        loop {
                            if ws_tx_reader.is_closed() {
                                break;
                            }
                            match stdout.read(&mut read_buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    buffer.extend_from_slice(&read_buf[..n]);
                                    while let Some(nal_unit) =
                                        super::extract_h264_access_unit(&mut buffer)
                                    {
                                        let seq = frame_seq_reader.fetch_add(1, Ordering::Relaxed);
                                        let nal_len = nal_unit.len();
                                        let is_keyframe = super::is_h264_keyframe(&nal_unit);

                                        let envelope = Envelope {
                                            id: "f".to_string(),
                                            session_id: sid.clone(),
                                            timestamp: None,
                                            payload: Some(envelope::Payload::DesktopFrame(
                                                DesktopFrame {
                                                    width: 0,
                                                    height: 0,
                                                    data: nal_unit,
                                                    sequence: seq,
                                                    quality: 0,
                                                    codec: FrameCodec::H264.into(),
                                                    is_keyframe,
                                                },
                                            )),
                                        };

                                        let mut buf = Vec::with_capacity(nal_len + 64);
                                        if envelope.encode(&mut buf).is_ok() {
                                            if ws_tx_reader.send(buf).is_err() {
                                                return;
                                            }
                                            if seq % 60 == 0 {
                                                tracing::info!(
                                                    seq,
                                                    h264_bytes = nal_len,
                                                    is_keyframe,
                                                    encoder = *encoder,
                                                    "Sent H264 frame (Windows)"
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    });

                    tracing::info!(encoder = *encoder, "FFmpeg H264 encoder started (Windows)");
                    return Some(FfmpegEncoder {
                        stdin,
                        process: child,
                        _reader_handle: reader_handle,
                    });
                }
            }
            None
        }
    }

    impl GraphicsCaptureApiHandler for CaptureHandler {
        type Flags = (
            mpsc::UnboundedSender<Vec<u8>>,
            String,
            Option<std::path::PathBuf>,
            Arc<super::QualityConfig>,
        );
        type Error = Box<dyn std::error::Error + Send + Sync>;

        fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
            let (ws_tx, session_id, ffmpeg_path, quality_config) = ctx.flags;
            let initial_bitrate = quality_config.effective_bitrate(4000);
            Ok(Self {
                ws_tx,
                session_id,
                frame_seq: AtomicU32::new(0),
                ffmpeg: None,
                sw_encoder: None,
                ffmpeg_attempted: false,
                ffmpeg_path,
                quality_config,
                current_bitrate: initial_bitrate,
                frames_since_bitrate_check: 0,
            })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame<'_>,
            capture_control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            if self.ws_tx.is_closed() {
                capture_control.stop();
                return Ok(());
            }

            let width = frame.width();
            let height = frame.height();
            let mut frame_buffer = frame.buffer()?;
            let bgra_data = frame_buffer.as_raw_buffer();

            // Lazy FFmpeg init on first frame (when we know dimensions)
            if !self.ffmpeg_attempted {
                self.ffmpeg_attempted = true;
                if let Some(ref path) = self.ffmpeg_path {
                    self.ffmpeg = Self::try_init_ffmpeg(
                        path,
                        width,
                        height,
                        self.current_bitrate,
                        &self.ws_tx,
                        &self.session_id,
                    );
                }
                if self.ffmpeg.is_none() {
                    // Try OpenH264 in-process encoder as fallback
                    match crate::encoder::SoftwareEncoder::new(
                        width,
                        height,
                        self.current_bitrate,
                        30,
                    ) {
                        Ok(enc) => {
                            tracing::info!(
                                "FFmpeg not available, using OpenH264 software encoder (bitrate={}kbps)",
                                self.current_bitrate
                            );
                            self.sw_encoder = Some(enc);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "FFmpeg and OpenH264 not available on Windows, using JPEG fallback: {}",
                                e
                            );
                        }
                    }
                }
            }

            // H264 path: write BGRA to FFmpeg stdin
            if let Some(ref mut enc) = self.ffmpeg {
                if enc.stdin.write_all(bgra_data).is_err() {
                    tracing::warn!("FFmpeg stdin write failed, falling back to JPEG");
                    // Drop the broken encoder, switch to JPEG
                    self.ffmpeg = None;
                } else {
                    // Check for bitrate change every 30 frames
                    self.frames_since_bitrate_check += 1;
                    if self.frames_since_bitrate_check >= 30 {
                        self.frames_since_bitrate_check = 0;
                        let new_bitrate = self.quality_config.effective_bitrate(4000);
                        let ratio = if new_bitrate > self.current_bitrate {
                            new_bitrate as f64 / self.current_bitrate as f64
                        } else {
                            self.current_bitrate as f64 / new_bitrate as f64
                        };
                        if ratio >= 1.2 {
                            tracing::info!(
                                old_bitrate = self.current_bitrate,
                                new_bitrate,
                                "Windows: Bitrate changed, restarting FFmpeg encoder"
                            );
                            self.current_bitrate = new_bitrate;
                            // Teardown current encoder
                            if let Some(mut old_enc) = self.ffmpeg.take() {
                                drop(old_enc.stdin);
                                let _ = old_enc.process.kill();
                                let _ = old_enc.process.wait();
                            }
                            // Respawn with new bitrate
                            if let Some(ref path) = self.ffmpeg_path {
                                self.ffmpeg = Self::try_init_ffmpeg(
                                    path,
                                    width,
                                    height,
                                    self.current_bitrate,
                                    &self.ws_tx,
                                    &self.session_id,
                                );
                                if self.ffmpeg.is_some() {
                                    tracing::info!(
                                        "FFmpeg restarted with bitrate={}kbps",
                                        self.current_bitrate
                                    );
                                }
                            }
                        }
                    }
                }
                return Ok(());
            }

            // OpenH264 in-process encoder path
            if let Some(ref mut sw_enc) = self.sw_encoder {
                match sw_enc.encode_bgra(bgra_data) {
                    Ok(Some(encoded)) => {
                        let seq = self.frame_seq.fetch_add(1, Ordering::Relaxed);
                        let nal_len = encoded.data.len();

                        let envelope = Envelope {
                            id: "f".to_string(),
                            session_id: self.session_id.clone(),
                            timestamp: None,
                            payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                                width,
                                height,
                                data: encoded.data,
                                sequence: seq,
                                quality: 0,
                                codec: FrameCodec::H264.into(),
                                is_keyframe: encoded.is_keyframe,
                            })),
                        };

                        let mut buf = Vec::with_capacity(nal_len + 64);
                        if envelope.encode(&mut buf).is_ok() {
                            if self.ws_tx.send(buf).is_err() {
                                capture_control.stop();
                            }
                            if seq % 60 == 0 {
                                tracing::info!(
                                    seq,
                                    h264_bytes = nal_len,
                                    is_keyframe = encoded.is_keyframe,
                                    "Sent H264 frame (Windows OpenH264)"
                                );
                            }
                        }
                    }
                    Ok(None) => {} // Frame skipped by rate control
                    Err(e) => {
                        tracing::warn!("OpenH264 encode error: {}", e);
                    }
                }
                return Ok(());
            }

            // JPEG fallback path
            let pixel_count = (width * height) as usize;
            let mut rgb_data = Vec::with_capacity(pixel_count * 3);
            for i in 0..pixel_count {
                let base = i * 4;
                if base + 2 < bgra_data.len() {
                    rgb_data.push(bgra_data[base + 2]); // R
                    rgb_data.push(bgra_data[base + 1]); // G
                    rgb_data.push(bgra_data[base]); // B
                }
            }

            let mut jpeg_buf = Cursor::new(Vec::new());
            let eff_quality = self
                .quality_config
                .effective_quality(DEFAULT_QUALITY as u32) as u8;
            let encoder = JpegEncoder::new_with_quality(&mut jpeg_buf, eff_quality);
            if encoder
                .write_image(&rgb_data, width, height, ExtendedColorType::Rgb8)
                .is_err()
            {
                return Ok(());
            }

            let jpeg_data = jpeg_buf.into_inner();
            let seq = self.frame_seq.fetch_add(1, Ordering::Relaxed);

            let envelope = Envelope {
                id: Uuid::new_v4().to_string(),
                session_id: self.session_id.clone(),
                timestamp: None,
                payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                    width,
                    height,
                    data: jpeg_data,
                    sequence: seq,
                    quality: eff_quality as u32,
                    codec: FrameCodec::Jpeg.into(),
                    is_keyframe: true,
                })),
            };

            let mut buf = Vec::new();
            if envelope.encode(&mut buf).is_ok() {
                if self.ws_tx.send(buf).is_err() {
                    capture_control.stop();
                }
                if seq % 30 == 0 {
                    tracing::info!(
                        seq,
                        width,
                        height,
                        "Sent desktop frame (Windows JPEG fallback)"
                    );
                }
            }

            Ok(())
        }

        fn on_closed(&mut self) -> Result<(), Self::Error> {
            tracing::info!("Windows capture closed for session {}", self.session_id);
            // Clean up FFmpeg if running
            if let Some(mut enc) = self.ffmpeg.take() {
                drop(enc.stdin);
                let _ = enc.process.kill();
                let _ = enc.process.wait();
            }
            Ok(())
        }
    }

    pub async fn capture_session(
        session_id: String,
        monitor_index: u32,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
        quality_config: std::sync::Arc<super::QualityConfig>,
    ) {
        tracing::info!(
            "Starting DXGI capture for session {} (monitor {})",
            session_id,
            monitor_index
        );

        // Resolve FFmpeg path before entering the blocking capture thread
        let ffmpeg_path = crate::ffmpeg::ensure_ffmpeg().await;

        // TODO: Use Monitor::enumerate() when available to select by index
        let monitor = match Monitor::primary() {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("Failed to get primary monitor: {}", e);
                return;
            }
        };

        let settings = Settings::new(
            monitor,
            CursorCaptureSettings::WithCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            (ws_tx, session_id.clone(), ffmpeg_path, quality_config),
        );

        // Run capture in a blocking thread since it uses COM
        let capture_handle = tokio::task::spawn_blocking(move || CaptureHandler::start(settings));

        tokio::select! {
            result = capture_handle => {
                match result {
                    Ok(Ok(_)) => tracing::info!("Windows capture ended for session {}", session_id),
                    Ok(Err(e)) => tracing::error!("Windows capture error: {}", e),
                    Err(e) => tracing::error!("Windows capture thread panicked: {}", e),
                }
            }
            _ = shutdown_rx => {
                tracing::info!("Windows capture received shutdown signal for session {}", session_id);
                // The ws_tx channel being dropped will cause CaptureHandler to stop
                // on the next frame arrival when it tries to send
            }
        }
    }
}
