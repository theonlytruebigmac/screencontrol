//! Screen capture for remote desktop sessions.
//!
//! Platform-specific implementations:
//! - **Linux (GNOME)**: Uses the Mutter ScreenCast D-Bus API + GStreamer pipeline
//!   for silent, unattended capture — no portal dialog needed.
//! - **macOS**: Placeholder — will use ScreenCaptureKit or CGDisplayStream.
//! - **Windows**: Placeholder — will use DXGI Desktop Duplication.

use std::collections::HashMap;

use prost::Message as ProstMessage;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use uuid::Uuid;

use sc_protocol::{envelope, Envelope, MonitorInfo, ScreenInfo};

/// Manages screen capture sessions.
pub struct DesktopCapturer {
    sessions: HashMap<String, CaptureHandle>,
}

struct CaptureHandle {
    handle: JoinHandle<()>,
    /// Send on this channel to trigger Mutter ScreenCast D-Bus session cleanup.
    shutdown_tx: Option<oneshot::Sender<()>>,
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
    pub fn start_capture(
        &mut self,
        session_id: &str,
        monitor_index: u32,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
    ) {
        let sid = session_id.to_string();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        #[cfg(target_os = "linux")]
        let handle = tokio::spawn(linux::capture_session(
            sid.clone(),
            monitor_index,
            ws_tx,
            shutdown_rx,
        ));

        #[cfg(target_os = "macos")]
        let handle = tokio::spawn(macos::capture_session(sid.clone(), monitor_index, ws_tx));

        #[cfg(target_os = "windows")]
        let handle = tokio::spawn(windows::capture_session(sid.clone(), monitor_index, ws_tx));

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
            },
        );
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
fn enumerate_monitors() -> Vec<MonitorInfo> {
    #[cfg(target_os = "linux")]
    {
        if let Some(monitors) = enumerate_monitors_xrandr() {
            if !monitors.is_empty() {
                return monitors;
            }
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

// ─── Linux: GNOME Mutter D-Bus + GStreamer ──────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU32, Ordering};

    use prost::Message as ProstMessage;
    use tokio::sync::mpsc;
    use uuid::Uuid;
    use zbus::Connection;

    use sc_protocol::{envelope, DesktopFrame, Envelope};

    const DEFAULT_FPS: u32 = 15;
    const DEFAULT_QUALITY: u32 = 60;

    struct MutterScreenCast {
        node_id: u32,
        connection: Connection,
        session_path: zbus::zvariant::OwnedObjectPath,
    }

    pub async fn capture_session(
        session_id: String,
        monitor_index: u32,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
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
            "Mutter ScreenCast session started silently (no user dialog)"
        );

        let dbus_conn = screencast_info.connection;
        let session_path = screencast_info.session_path;

        // Spawn a keeper that waits for the shutdown signal, then
        // properly stops the Mutter ScreenCast D-Bus session.
        let keeper_conn = dbus_conn.clone();
        let keeper_path = session_path.clone();
        let _session_keeper = tokio::spawn(async move {
            // Wait for shutdown signal
            let _ = shutdown_rx.await;
            tracing::info!("Stopping Mutter ScreenCast D-Bus session...");
            match keeper_conn
                .call_method(
                    Some("org.gnome.Mutter.ScreenCast"),
                    keeper_path.as_ref(),
                    Some("org.gnome.Mutter.ScreenCast.Session"),
                    "Stop",
                    &(),
                )
                .await
            {
                Ok(_) => tracing::info!("Mutter ScreenCast session stopped via D-Bus"),
                Err(e) => tracing::warn!("Failed to stop Mutter ScreenCast session: {}", e),
            }
        });

        let sid = session_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            run_gst_capture(node_id, DEFAULT_FPS, DEFAULT_QUALITY, &sid, ws_tx);
        })
        .await;

        if let Err(e) = result {
            tracing::error!("GStreamer capture thread panicked: {}", e);
        }
    }

    async fn request_mutter_screencast() -> anyhow::Result<MutterScreenCast> {
        let connection = Connection::session().await?;

        // Session properties — disable animations to avoid visual artifacts
        let mut session_props = std::collections::HashMap::<String, zbus::zvariant::Value>::new();
        session_props.insert(
            "disable-animations".to_string(),
            zbus::zvariant::Value::Bool(true),
        );

        let reply: zbus::zvariant::OwnedObjectPath = connection
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

        tracing::info!(session_path = %reply, "Mutter ScreenCast session created");

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

        let stream_reply: zbus::zvariant::OwnedObjectPath = connection
            .call_method(
                Some("org.gnome.Mutter.ScreenCast"),
                reply.as_ref(),
                Some("org.gnome.Mutter.ScreenCast.Session"),
                "RecordMonitor",
                &("", stream_props),
            )
            .await?
            .body()
            .deserialize()?;

        tracing::info!(stream_path = %stream_reply, "Mutter ScreenCast stream created");

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

        connection
            .call_method(
                Some("org.gnome.Mutter.ScreenCast"),
                reply.as_ref(),
                Some("org.gnome.Mutter.ScreenCast.Session"),
                "Start",
                &(),
            )
            .await?;

        tracing::info!("Mutter ScreenCast session started, waiting for PipeWire node_id...");

        let node_id = tokio::time::timeout(std::time::Duration::from_secs(5), node_rx)
            .await
            .map_err(|_| anyhow::anyhow!("Timeout waiting for PipeWireStreamAdded signal"))?
            .map_err(|_| anyhow::anyhow!("Signal task dropped"))?;

        signal_task.abort();

        tracing::info!(node_id, "Mutter ScreenCast PipeWire node ready");

        Ok(MutterScreenCast {
            node_id,
            connection,
            session_path: reply,
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

        let pipeline = format!(
            "pipewiresrc path={node_id} do-timestamp=true keepalive-time=1000 \
             ! videoconvert \
             ! videorate \
             ! video/x-raw,framerate={fps}/1 \
             ! jpegenc quality={quality} \
             ! fdsink fd=1",
        );

        tracing::info!("Launching GStreamer pipeline: {}", pipeline);

        let mut child = match Command::new("sh")
            .args(["-c", &format!("gst-launch-1.0 -q -e {}", pipeline)])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("Failed to launch GStreamer: {}", e);
                return;
            }
        };

        let mut stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                tracing::error!("Failed to capture GStreamer stdout");
                let _ = child.kill();
                return;
            }
        };

        tracing::info!("GStreamer pipeline started, reading JPEG frames");

        let mut buffer = Vec::with_capacity(1024 * 1024);
        let mut read_buf = [0u8; 65536];
        let mut frame_width = 0u32;
        let mut frame_height = 0u32;

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

                    while let Some(frame) = extract_jpeg_frame(&mut buffer) {
                        let seq = frame_seq.fetch_add(1, Ordering::Relaxed);
                        let jpeg_len = frame.len();

                        if seq == 0 {
                            if let Some((w, h)) = parse_jpeg_dimensions(&frame) {
                                frame_width = w;
                                frame_height = h;
                                tracing::info!(width = w, height = h, "Detected frame dimensions");
                            }
                        }

                        let envelope = Envelope {
                            id: Uuid::new_v4().to_string(),
                            session_id: sid.clone(),
                            timestamp: None,
                            payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                                width: frame_width,
                                height: frame_height,
                                data: frame,
                                sequence: seq,
                                quality,
                            })),
                        };

                        let mut buf = Vec::new();
                        if envelope.encode(&mut buf).is_ok() {
                            if ws_tx.send(buf).is_err() {
                                tracing::info!("WS channel closed, stopping capture");
                                let _ = child.kill();
                                return;
                            }
                            if seq % 30 == 0 {
                                tracing::info!(seq, jpeg_bytes = jpeg_len, "Sent desktop frame");
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

    fn extract_jpeg_frame(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
        let soi_pos = buffer
            .windows(2)
            .position(|w| w[0] == 0xFF && w[1] == 0xD8)?;

        let search_start = soi_pos + 2;
        if search_start >= buffer.len() {
            return None;
        }

        let eoi_pos = buffer[search_start..]
            .windows(2)
            .position(|w| w[0] == 0xFF && w[1] == 0xD9)
            .map(|p| search_start + p)?;

        let frame_end = eoi_pos + 2;
        let frame = buffer[soi_pos..frame_end].to_vec();
        buffer.drain(..frame_end);

        Some(frame)
    }

    fn parse_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
        for i in 0..data.len().saturating_sub(9) {
            if data[i] == 0xFF && (data[i + 1] == 0xC0 || data[i + 1] == 0xC2) {
                let height = ((data[i + 5] as u32) << 8) | (data[i + 6] as u32);
                let width = ((data[i + 7] as u32) << 8) | (data[i + 8] as u32);
                if width > 0 && height > 0 {
                    return Some((width, height));
                }
            }
        }
        None
    }
}

// ─── macOS: CoreGraphics capture ────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos {
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU32, Ordering};

    use image::codecs::jpeg::JpegEncoder;
    use image::ImageEncoder;
    use prost::Message as ProstMessage;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use sc_protocol::{envelope, DesktopFrame, Envelope};

    const DEFAULT_FPS: u32 = 15;
    const DEFAULT_QUALITY: u8 = 60;

    // CoreGraphics FFI bindings
    type CGDirectDisplayID = u32;
    type CGImageRef = *const std::ffi::c_void;
    type CGColorSpaceRef = *const std::ffi::c_void;
    type CGContextRef = *mut std::ffi::c_void;

    type CGEventRef = *const std::ffi::c_void;
    type CGEventSourceRef = *const std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> CGDirectDisplayID;
        fn CGDisplayPixelsWide(display: CGDirectDisplayID) -> usize;
        fn CGDisplayPixelsHigh(display: CGDirectDisplayID) -> usize;
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
        fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
        fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
        fn CFRelease(cf: *const std::ffi::c_void);
        fn CGPreflightScreenCaptureAccess() -> bool;
        #[allow(dead_code)]
        fn CGRequestScreenCaptureAccess() -> bool;
    }

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

    // kCGImageAlphaNoneSkipLast = RGBX (RGB with padding byte, no alpha)
    const BITMAP_INFO_RGBX: u32 = 5; // kCGImageAlphaNoneSkipLast

    /// Get current mouse cursor position in logical (point) coordinates
    fn get_mouse_position() -> (f64, f64) {
        unsafe {
            let event = CGEventCreate(std::ptr::null());
            if event.is_null() {
                return (0.0, 0.0);
            }
            let point = CGEventGetLocation(event);
            CFRelease(event);
            (point.x, point.y)
        }
    }

    /// Draw a simple arrow cursor onto RGBX pixel data at the given pixel coordinates.
    /// `scale` controls pixel size (e.g. 2 for Retina) so the cursor is visible.
    fn draw_cursor(
        pixel_data: &mut [u8],
        img_width: usize,
        img_height: usize,
        cx: usize,
        cy: usize,
        scale: usize,
    ) {
        // Standard arrow cursor shape (11x19 logical pixels)
        // 1 = white fill, 2 = black outline, 0 = transparent
        const CURSOR: &[&[u8]] = &[
            &[2],
            &[2, 2],
            &[2, 1, 2],
            &[2, 1, 1, 2],
            &[2, 1, 1, 1, 2],
            &[2, 1, 1, 1, 1, 2],
            &[2, 1, 1, 1, 1, 1, 2],
            &[2, 1, 1, 1, 1, 1, 1, 2],
            &[2, 1, 1, 1, 1, 1, 1, 1, 2],
            &[2, 1, 1, 1, 1, 1, 1, 1, 1, 2],
            &[2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2],
            &[2, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2],
            &[2, 1, 1, 1, 2, 1, 1, 2],
            &[2, 1, 1, 2, 0, 2, 1, 1, 2],
            &[2, 1, 2, 0, 0, 2, 1, 1, 2],
            &[2, 2, 0, 0, 0, 0, 2, 1, 1, 2],
            &[2, 0, 0, 0, 0, 0, 2, 1, 1, 2],
            &[0, 0, 0, 0, 0, 0, 0, 2, 1, 2],
            &[0, 0, 0, 0, 0, 0, 0, 2, 2],
        ];

        let s = scale.max(1);
        let bytes_per_row = img_width * 4;
        for (dy, row) in CURSOR.iter().enumerate() {
            for (dx, &pixel) in row.iter().enumerate() {
                if pixel == 0 {
                    continue;
                }
                let (r, g, b) = if pixel == 1 {
                    (255u8, 255u8, 255u8)
                } else {
                    (0u8, 0u8, 0u8)
                };
                // Fill a scale×scale block for each cursor pixel
                for sy in 0..s {
                    let py = cy + dy * s + sy;
                    if py >= img_height {
                        break;
                    }
                    for sx in 0..s {
                        let px = cx + dx * s + sx;
                        if px >= img_width {
                            break;
                        }
                        let base = py * bytes_per_row + px * 4;
                        if base + 2 < pixel_data.len() {
                            pixel_data[base] = r;
                            pixel_data[base + 1] = g;
                            pixel_data[base + 2] = b;
                        }
                    }
                }
            }
        }
    }

    /// Capture a single frame from the display and return raw RGBX pixel data with cursor
    fn capture_display_frame(display_id: CGDirectDisplayID) -> Option<(Vec<u8>, u32, u32)> {
        unsafe {
            let cg_image = CGDisplayCreateImage(display_id);
            if cg_image.is_null() {
                return None;
            }

            let width = CGImageGetWidth(cg_image);
            let height = CGImageGetHeight(cg_image);
            if width == 0 || height == 0 {
                CGImageRelease(cg_image);
                return None;
            }

            let bytes_per_row = width * 4;
            let mut pixel_data: Vec<u8> = vec![0u8; bytes_per_row * height];

            let color_space = CGColorSpaceCreateDeviceRGB();
            let context = CGBitmapContextCreate(
                pixel_data.as_mut_ptr(),
                width,
                height,
                8, // bits per component
                bytes_per_row,
                color_space,
                BITMAP_INFO_RGBX,
            );

            if context.is_null() {
                CGColorSpaceRelease(color_space);
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

            CGContextDrawImage(context, rect, cg_image);
            CGContextRelease(context);
            CGColorSpaceRelease(color_space);
            CGImageRelease(cg_image);

            // Draw cursor overlay
            let (mx, my) = get_mouse_position();
            // Scale from logical points to pixel coordinates (Retina)
            let logical_w = CGDisplayPixelsWide(display_id) as f64;
            let scale_f = if logical_w > 0.0 {
                width as f64 / logical_w
            } else {
                2.0
            };
            let scale = scale_f.round() as usize;
            let cursor_x = (mx * scale_f) as usize;
            let cursor_y = (my * scale_f) as usize;
            draw_cursor(&mut pixel_data, width, height, cursor_x, cursor_y, scale);

            Some((pixel_data, width as u32, height as u32))
        }
    }

    pub async fn capture_session(
        session_id: String,
        _monitor_index: u32,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
    ) {
        // Pre-flight TCC check — informational only, do NOT call
        // CGRequestScreenCaptureAccess as it triggers a system dialog every time.
        unsafe {
            let has_access = CGPreflightScreenCaptureAccess();
            if has_access {
                tracing::debug!("Screen Recording permission pre-flight: granted");
            } else {
                tracing::warn!(
                    "Screen Recording permission pre-flight: NOT granted. \
                     Grant permission in System Settings > Privacy & Security > Screen Recording."
                );
            }
        }

        let display_id = unsafe { CGMainDisplayID() };
        let display_width = unsafe { CGDisplayPixelsWide(display_id) } as u32;
        let display_height = unsafe { CGDisplayPixelsHigh(display_id) } as u32;

        tracing::info!(
            "Starting CoreGraphics capture for session {} (display {}, {}x{})",
            session_id,
            display_id,
            display_width,
            display_height
        );

        let frame_interval = std::time::Duration::from_millis(1000 / DEFAULT_FPS as u64);
        let frame_seq = AtomicU32::new(0);

        loop {
            if ws_tx.is_closed() {
                tracing::info!("WS channel closed, stopping macOS capture");
                break;
            }

            // Capture a frame using CoreGraphics (blocking FFI, run on blocking thread)
            let frame_result =
                tokio::task::spawn_blocking(move || capture_display_frame(display_id)).await;

            let (bgra_data, width, height) = match frame_result {
                Ok(Some(frame)) => frame,
                Ok(None) => {
                    tracing::warn!("CGDisplayCreateImage returned null — no screen access?");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
                Err(e) => {
                    tracing::error!("Capture task panicked: {}", e);
                    break;
                }
            };

            // Convert RGBX → RGB for JPEG encoding (drop padding byte)
            let bytes_per_row = width as usize * 4;
            let pixel_count = (width * height) as usize;
            let mut rgb_data = Vec::with_capacity(pixel_count * 3);
            for row in 0..height as usize {
                let row_start = row * bytes_per_row;
                for col in 0..width as usize {
                    let base = row_start + col * 4;
                    if base + 2 < bgra_data.len() {
                        rgb_data.push(bgra_data[base]); // R
                        rgb_data.push(bgra_data[base + 1]); // G
                        rgb_data.push(bgra_data[base + 2]); // B
                    }
                }
            }

            // Encode to JPEG
            let mut jpeg_buf = Cursor::new(Vec::new());
            let encoder = JpegEncoder::new_with_quality(&mut jpeg_buf, DEFAULT_QUALITY);
            if encoder
                .write_image(&rgb_data, width, height, image::ExtendedColorType::Rgb8)
                .is_err()
            {
                tracing::warn!("JPEG encode failed for {}x{} frame", width, height);
                tokio::time::sleep(frame_interval).await;
                continue;
            }

            let jpeg_data = jpeg_buf.into_inner();
            let seq = frame_seq.fetch_add(1, Ordering::Relaxed);

            let envelope = Envelope {
                id: Uuid::new_v4().to_string(),
                session_id: session_id.clone(),
                timestamp: None,
                payload: Some(envelope::Payload::DesktopFrame(DesktopFrame {
                    width,
                    height,
                    data: jpeg_data,
                    sequence: seq,
                    quality: DEFAULT_QUALITY as u32,
                })),
            };

            let mut buf = Vec::new();
            if envelope.encode(&mut buf).is_ok() {
                if ws_tx.send(buf).is_err() {
                    tracing::info!("WS channel closed, stopping capture");
                    break;
                }
                if seq % 30 == 0 {
                    tracing::info!(seq, width, height, "Sent desktop frame (macOS CG)");
                }
            }

            tokio::time::sleep(frame_interval).await;
        }

        tracing::info!("CoreGraphics capture stopped for session {}", session_id);
    }
}

// ─── Windows: DXGI Desktop Duplication (Phase C) ───────────────────

#[cfg(target_os = "windows")]
mod windows {
    use std::io::Cursor;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use image::codecs::jpeg::JpegEncoder;
    use image::ColorType;
    use prost::Message as ProstMessage;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use windows_capture::capture::{CaptureControl, GraphicsCaptureApiHandler};
    use windows_capture::frame::Frame;
    use windows_capture::graphics_capture_api::InternalCaptureControl;
    use windows_capture::monitor::Monitor;
    use windows_capture::settings::{
        ColorFormat, CursorCaptureSettings, DrawBorderSettings, Settings,
    };

    use sc_protocol::{envelope, DesktopFrame, Envelope};

    const DEFAULT_QUALITY: u8 = 60;

    struct CaptureHandler {
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
        session_id: String,
        frame_seq: AtomicU32,
    }

    impl GraphicsCaptureApiHandler for CaptureHandler {
        type Flags = (mpsc::UnboundedSender<Vec<u8>>, String);
        type Error = Box<dyn std::error::Error + Send + Sync>;

        fn new((ws_tx, session_id): Self::Flags) -> Result<Self, Self::Error> {
            Ok(Self {
                ws_tx,
                session_id,
                frame_seq: AtomicU32::new(0),
            })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            capture_control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            if self.ws_tx.is_closed() {
                capture_control.stop();
                return Ok(());
            }

            let width = frame.width();
            let height = frame.height();

            // Get BGRA buffer from frame
            let bgra_data = frame.buffer()?;

            // Convert BGRA → RGB for JPEG encoding
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

            // Encode to JPEG
            let mut jpeg_buf = Cursor::new(Vec::new());
            let encoder = JpegEncoder::new_with_quality(&mut jpeg_buf, DEFAULT_QUALITY);
            if encoder
                .write_image(&rgb_data, width, height, ColorType::Rgb8.into())
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
                    quality: DEFAULT_QUALITY as u32,
                })),
            };

            let mut buf = Vec::new();
            if envelope.encode(&mut buf).is_ok() {
                if self.ws_tx.send(buf).is_err() {
                    capture_control.stop();
                }
                if seq % 30 == 0 {
                    tracing::info!(seq, width, height, "Sent desktop frame (Windows)");
                }
            }

            Ok(())
        }

        fn on_closed(&mut self) -> Result<(), Self::Error> {
            tracing::info!("Windows capture closed for session {}", self.session_id);
            Ok(())
        }
    }

    pub async fn capture_session(
        session_id: String,
        monitor_index: u32,
        ws_tx: mpsc::UnboundedSender<Vec<u8>>,
    ) {
        tracing::info!(
            "Starting DXGI capture for session {} (monitor {})",
            session_id,
            monitor_index
        );

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
            ColorFormat::Bgra8,
            (ws_tx, session_id.clone()),
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
