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
}

impl QualityConfig {
    pub fn new() -> Self {
        Self {
            quality: AtomicU32::new(0),
            max_fps: AtomicU32::new(0),
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
    pub fn set_quality(&self, session_id: &str, quality: u32, max_fps: u32) {
        if let Some(handle) = self.sessions.get(session_id) {
            handle.quality.quality.store(quality, Ordering::Relaxed);
            handle.quality.max_fps.store(max_fps, Ordering::Relaxed);
            tracing::info!(
                session_id,
                quality,
                max_fps,
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
            qc.clone(),
        ));

        #[cfg(target_os = "windows")]
        let handle = tokio::spawn(windows::capture_session(
            sid.clone(),
            monitor_index,
            ws_tx,
            qc.clone(),
        ));

        // For non-Linux platforms, drop the shutdown_rx since they don't use it yet
        #[cfg(not(target_os = "linux"))]
        drop(shutdown_rx);

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let handle = tokio::spawn(async move {
            tracing::error!("Screen capture not supported on this platform");
            let _ = (ws_tx, monitor_index);
        });

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
            // Signal the session keeper to stop the Mutter ScreenCast D-Bus session
            if let Some(tx) = handle.shutdown_tx.take() {
                let _ = tx.send(());
                tracing::info!(
                    "Sent shutdown signal for ScreenCast session: {}",
                    session_id
                );
            }
            handle.handle.abort();
            tracing::info!("Screen capture session stopped: {}", session_id);
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

/// Extract a complete H264 access unit from Annex B byte stream.
/// Access units are delimited by `00 00 00 01` start codes.
/// Returns the bytes from one start code to the next (inclusive of first start code).
fn extract_h264_access_unit(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let first = find_h264_start_code(buffer, 0)?;
    let search_from = first + 4;
    if search_from >= buffer.len() {
        return None;
    }
    let second = match find_h264_start_code(buffer, search_from) {
        Some(pos) => pos,
        None => return None,
    };
    let frame = buffer[first..second].to_vec();
    buffer.drain(..second);
    Some(frame)
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

// ─── Linux: GNOME Mutter D-Bus + GStreamer ──────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU32, Ordering};

    use prost::Message as ProstMessage;
    use tokio::sync::mpsc;
    use zbus::Connection;

    use sc_protocol::{envelope, DesktopFrame, Envelope, FrameCodec};

    const DEFAULT_FPS: u32 = 30;
    const DEFAULT_QUALITY: u32 = 50;

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

        // Spawn a keeper that waits for the shutdown signal, then
        // properly stops the Mutter RemoteDesktop D-Bus session
        // (which also stops the paired ScreenCast session).
        let keeper_conn = dbus_conn.clone();
        let keeper_rd_path = rd_session_path.clone();
        let _session_keeper = tokio::spawn(async move {
            // Wait for shutdown signal
            let _ = shutdown_rx.await;
            tracing::info!("Stopping Mutter RemoteDesktop D-Bus session...");
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
        });

        let sid = session_id.clone();
        let eff_fps = quality_config.effective_fps(DEFAULT_FPS);
        let eff_quality = quality_config.effective_quality(DEFAULT_QUALITY);
        let result = tokio::task::spawn_blocking(move || {
            run_gst_capture(node_id, eff_fps, eff_quality, &sid, ws_tx);
        })
        .await;

        if let Err(e) = result {
            tracing::error!("GStreamer capture thread panicked: {}", e);
        }
    }

    async fn request_mutter_screencast() -> anyhow::Result<MutterScreenCast> {
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
        let rd_session_id = rd_session_path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();

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
        let signal_task = tokio::spawn(async move {
            use futures_util::StreamExt;

            let mut stream = zbus::MessageStream::from(&signal_conn);

            while let Some(msg) = stream.next().await {
                if let Ok(msg) = msg {
                    let header = msg.header();
                    if header.member().map(|m| m.as_str()) == Some("PipeWireStreamAdded") {
                        if let Ok(node_id) = msg.body().deserialize::<u32>() {
                            let _ = node_tx.send(node_id);
                            return;
                        }
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
    ) {
        let sid = session_id.to_string();
        let frame_seq = AtomicU32::new(0);

        // Try VAAPI hardware encoder first, fall back to x264enc software
        let vaapi_pipeline = format!(
            "pipewiresrc path={node_id} do-timestamp=true keepalive-time=1000 \
             ! videoconvert \
             ! videorate \
             ! video/x-raw,framerate={fps}/1 \
             ! vaapih264enc rate-control=cbr bitrate=4000 keyframe-period=60 \
             ! video/x-h264,stream-format=byte-stream,alignment=au \
             ! fdsink fd=1",
        );

        let x264_pipeline = format!(
            "pipewiresrc path={node_id} do-timestamp=true keepalive-time=1000 \
             ! videoconvert \
             ! videorate \
             ! video/x-raw,framerate={fps}/1 \
             ! x264enc tune=zerolatency speed-preset=ultrafast \
               bitrate=4000 key-int-max=60 bframes=0 \
               option-string=\"repeat-headers=1:annexb=1\" \
             ! video/x-h264,stream-format=byte-stream,alignment=au \
             ! fdsink fd=1",
        );

        // Try VAAPI first
        let (pipeline_name, mut child) = {
            match Command::new("sh")
                .args(["-c", &format!("gst-launch-1.0 -q -e {}", vaapi_pipeline)])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => {
                    tracing::info!("Using VAAPI hardware H264 encoder");
                    ("vaapih264enc", c)
                }
                Err(_) => {
                    tracing::info!("VAAPI not available, trying x264enc software encoder");
                    match Command::new("sh")
                        .args(["-c", &format!("gst-launch-1.0 -q -e {}", x264_pipeline)])
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                    {
                        Ok(c) => ("x264enc", c),
                        Err(e) => {
                            tracing::error!("Failed to launch GStreamer: {}", e);
                            return;
                        }
                    }
                }
            }
        };

        tracing::info!(
            "Launched GStreamer H264 pipeline (encoder: {})",
            pipeline_name
        );
        let mut stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                tracing::error!("Failed to capture GStreamer stdout");
                let _ = child.kill();
                return;
            }
        };

        tracing::info!("GStreamer pipeline started, reading H264 stream");

        let mut buffer = Vec::with_capacity(1024 * 1024);
        let mut read_buf = [0u8; 65536];

        loop {
            if ws_tx.is_closed() {
                tracing::info!("WS channel closed, stopping GStreamer capture");
                break;
            }

            match stdout.read(&mut read_buf) {
                Ok(0) => {
                    tracing::info!("GStreamer stdout EOF");
                    break;
                }
                Ok(n) => {
                    buffer.extend_from_slice(&read_buf[..n]);

                    while let Some(nal_unit) = super::extract_h264_access_unit(&mut buffer) {
                        let seq = frame_seq.fetch_add(1, Ordering::Relaxed);
                        let nal_len = nal_unit.len();
                        let is_keyframe = super::is_h264_keyframe(&nal_unit);

                        if ws_tx.is_closed() {
                            tracing::info!("WS channel closed, stopping capture");
                            let _ = child.kill();
                            return;
                        }

                        let envelope = Envelope {
                            id: "f".to_string(),
                            session_id: sid.clone(),
                            timestamp: None,
                            payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                                width: 0, // H264 carries resolution in SPS
                                height: 0,
                                data: nal_unit,
                                sequence: seq,
                                quality,
                                codec: FrameCodec::H264.into(),
                                is_keyframe,
                            })),
                        };

                        let mut buf = Vec::with_capacity(nal_len + 64);
                        if envelope.encode(&mut buf).is_ok() {
                            if ws_tx.send(buf).is_err() {
                                tracing::info!("WS channel closed, stopping capture");
                                let _ = child.kill();
                                return;
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
                    tracing::error!("Error reading GStreamer stdout: {}", e);
                    break;
                }
            }
        }

        let _ = child.kill();
        let _ = child.wait();

        if let Some(mut stderr) = child.stderr.take() {
            let mut err_output = String::new();
            let _ = stderr.read_to_string(&mut err_output);
            if !err_output.is_empty() {
                tracing::warn!("GStreamer stderr: {}", err_output);
            }
        }

        tracing::info!("GStreamer capture stopped for session {}", sid);
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
        let ffmpeg_result = match ffmpeg_path {
            Some(ref p) => Command::new(p)
                .args([
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "bgra",
                    "-s",
                    &format!("{}x{}", disp_width, disp_height),
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
                    "4000k",
                    "-maxrate",
                    "6000k",
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
                .spawn(),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "FFmpeg not available",
            )),
        };

        let use_h264 = ffmpeg_result.is_ok();

        if use_h264 {
            tracing::info!("FFmpeg h264_videotoolbox encoder started — using H264");

            let mut ffmpeg = ffmpeg_result.unwrap();
            let mut ffmpeg_stdin = ffmpeg.stdin.take().unwrap();
            let ffmpeg_stdout = ffmpeg.stdout.take().unwrap();

            // Background thread: read H264 NAL units from FFmpeg stdout and send via WS
            let ws_tx_h264 = ws_tx.clone();
            let sid = session_id.to_string();
            let frame_seq = std::sync::Arc::new(AtomicU32::new(0));
            let frame_seq_reader = frame_seq.clone();

            let reader_handle = std::thread::spawn(move || {
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
            }

            // Clean up FFmpeg
            drop(ffmpeg_stdin); // close stdin to signal EOF
            let _ = ffmpeg.kill();
            let _ = ffmpeg.wait();
            let _ = reader_handle.join();
        } else {
            tracing::warn!("FFmpeg not available, falling back to JPEG encoding");

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

        // Try ScreenCaptureKit first (macOS 12.3+), fall back to CoreGraphics
        tracing::info!(
            "Attempting ScreenCaptureKit capture for session {}",
            session_id
        );
        match capture_with_screencapturekit(&session_id, monitor_index, &ws_tx, &quality_config)
            .await
        {
            Ok(()) => {
                tracing::info!("ScreenCaptureKit session ended normally: {}", session_id);
            }
            Err(e) => {
                tracing::warn!(
                    "ScreenCaptureKit failed ({}), falling back to CoreGraphics",
                    e
                );
                capture_with_coregraphics(&session_id, &ws_tx, &quality_config).await;
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
        /// Track whether we've attempted FFmpeg init
        ffmpeg_attempted: bool,
        /// Path to FFmpeg binary
        ffmpeg_path: Option<std::path::PathBuf>,
        /// Shared quality config for dynamic JPEG quality adjustment
        quality_config: Arc<super::QualityConfig>,
    }

    impl CaptureHandler {
        /// Try to spawn FFmpeg for H264 encoding. Returns None if unavailable.
        fn try_init_ffmpeg(
            ffmpeg_path: &std::path::Path,
            width: u32,
            height: u32,
            ws_tx: &mpsc::UnboundedSender<Vec<u8>>,
            session_id: &str,
        ) -> Option<FfmpegEncoder> {
            // Try nvenc first, fall back to h264_mf (Media Foundation)
            for encoder in &["h264_nvenc", "h264_mf", "libx264"] {
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
                        "4000k",
                        "-maxrate",
                        "6000k",
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
            Ok(Self {
                ws_tx,
                session_id,
                frame_seq: AtomicU32::new(0),
                ffmpeg: None,
                ffmpeg_attempted: false,
                ffmpeg_path,
                quality_config,
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
                    self.ffmpeg =
                        Self::try_init_ffmpeg(path, width, height, &self.ws_tx, &self.session_id);
                }
                if self.ffmpeg.is_none() {
                    tracing::warn!("FFmpeg not available on Windows, using JPEG fallback");
                }
            }

            // H264 path: write BGRA to FFmpeg stdin
            if let Some(ref mut enc) = self.ffmpeg {
                if enc.stdin.write_all(bgra_data).is_err() {
                    tracing::warn!("FFmpeg stdin write failed, falling back to JPEG");
                    // Drop the broken encoder, switch to JPEG
                    self.ffmpeg = None;
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
        let result = tokio::task::spawn_blocking(move || CaptureHandler::start(settings)).await;

        match result {
            Ok(Ok(_)) => tracing::info!("Windows capture ended for session {}", session_id),
            Ok(Err(e)) => tracing::error!("Windows capture error: {}", e),
            Err(e) => tracing::error!("Windows capture thread panicked: {}", e),
        }
    }
}
