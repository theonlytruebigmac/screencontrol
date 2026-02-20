#![allow(dead_code)]
//! Cross-platform cursor shape and position capture.
//!
//! Provides cursor data for streaming to remote viewers. Each platform
//! implements cursor capture differently:
//!
//! - **Linux**: Cursor is embedded in the GStreamer/ScreenCast stream
//!   (handled by compositor) — this module provides position only.
//! - **macOS**: CoreGraphics cursor seed + NSCursor API for shape data.
//! - **Windows**: `GetCursorInfo` + `GetIconInfo` + `GetDIBits` for shape.
//!
//! ## Caching Strategy
//!
//! Cursor shapes are identified by a platform-specific `cursor_id` (u64).
//! When the cursor shape changes, the full RGBA pixel data is sent once.
//! Subsequent position updates reference the cached `cursor_id`.

use std::collections::HashSet;

/// Captured cursor shape data.
#[derive(Clone, Debug)]
pub struct CursorShape {
    /// Platform-specific cursor handle (stable across frames)
    pub cursor_id: u64,
    /// Cursor image width in pixels
    pub width: u32,
    /// Cursor image height in pixels
    pub height: u32,
    /// Click-point X offset from top-left
    pub hotspot_x: u32,
    /// Click-point Y offset from top-left
    pub hotspot_y: u32,
    /// RGBA pixel data (width * height * 4 bytes)
    pub data: Vec<u8>,
}

/// Current cursor position (normalized coordinates).
#[derive(Clone, Debug)]
pub struct CursorPos {
    /// Normalized X position (0.0–1.0 relative to captured monitor)
    pub x: f64,
    /// Normalized Y position (0.0–1.0 relative to captured monitor)
    pub y: f64,
    /// Reference to the current cursor shape
    pub cursor_id: u64,
    /// Whether cursor is visible
    pub visible: bool,
}

/// Tracks cursor state and detects changes.
pub struct CursorTracker {
    /// Set of cursor_ids we've already sent to the viewer
    sent_shapes: HashSet<u64>,
    /// Last known cursor_id (for change detection)
    last_cursor_id: u64,
    /// Last known cursor position (for dedup)
    last_x: f64,
    last_y: f64,
}

impl CursorTracker {
    pub fn new() -> Self {
        Self {
            sent_shapes: HashSet::new(),
            last_cursor_id: 0,
            last_x: -1.0,
            last_y: -1.0,
        }
    }

    /// Check if a cursor shape needs to be sent (first time seeing this cursor_id).
    pub fn needs_shape_update(&self, cursor_id: u64) -> bool {
        !self.sent_shapes.contains(&cursor_id)
    }

    /// Mark a cursor shape as sent.
    pub fn mark_shape_sent(&mut self, cursor_id: u64) {
        self.sent_shapes.insert(cursor_id);
    }

    /// Check if cursor position has meaningfully changed.
    pub fn position_changed(&self, x: f64, y: f64) -> bool {
        let dx = (x - self.last_x).abs();
        let dy = (y - self.last_y).abs();
        // Threshold: ~0.5px on a 1920-wide screen
        dx > 0.0003 || dy > 0.0003
    }

    /// Update the last known position.
    pub fn update_position(&mut self, x: f64, y: f64, cursor_id: u64) {
        self.last_x = x;
        self.last_y = y;
        self.last_cursor_id = cursor_id;
    }

    /// Reset state (e.g. on session start).
    pub fn reset(&mut self) {
        self.sent_shapes.clear();
        self.last_cursor_id = 0;
        self.last_x = -1.0;
        self.last_y = -1.0;
    }
}

// ─── Linux ──────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
pub mod platform {
    use super::*;

    /// Get cursor position relative to a monitor.
    ///
    /// On Linux/Wayland (GNOME), the cursor is rendered by the compositor
    /// and embedded in the ScreenCast stream. We can still query position
    /// via D-Bus for the CursorPosition message, but shape data comes
    /// from the stream itself.
    pub fn get_cursor_position(
        monitor_x: i32,
        monitor_y: i32,
        monitor_w: u32,
        monitor_h: u32,
    ) -> Option<CursorPos> {
        // On GNOME Wayland, we can't directly query cursor position
        // without a RemoteDesktop session. The cursor is rendered by
        // the compositor into the ScreenCast stream.
        // Return None to indicate cursor position is not separately available.
        let _ = (monitor_x, monitor_y, monitor_w, monitor_h);
        None
    }

    /// Get cursor shape data.
    ///
    /// On Wayland, cursor shape is compositor-managed and rendered into
    /// the capture stream. No separate shape data is needed.
    pub fn get_cursor_shape() -> Option<CursorShape> {
        None
    }
}

// ─── macOS ──────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub mod platform {
    use super::*;

    // CoreGraphics FFI for cursor position
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    extern "C" {
        fn CGSMainConnectionID() -> u32;
        fn CGSGetCurrentCursorLocation(conn: u32, point: *mut CGPoint) -> i32;
        fn CGSCurrentCursorSeed() -> u32;
        fn CGMainDisplayID() -> u32;
        fn CGDisplayPixelsWide(display: u32) -> usize;
        fn CGDisplayPixelsHigh(display: u32) -> usize;
    }

    /// Get normalized cursor position relative to the main display.
    pub fn get_cursor_position(
        monitor_x: i32,
        monitor_y: i32,
        monitor_w: u32,
        monitor_h: u32,
    ) -> Option<CursorPos> {
        unsafe {
            let conn = CGSMainConnectionID();
            let mut point = CGPoint { x: 0.0, y: 0.0 };
            let result = CGSGetCurrentCursorLocation(conn, &mut point);
            if result != 0 {
                return None;
            }

            // Convert absolute screen coords to normalized monitor coords
            let rel_x = point.x - monitor_x as f64;
            let rel_y = point.y - monitor_y as f64;

            let norm_x = rel_x / monitor_w as f64;
            let norm_y = rel_y / monitor_h as f64;

            let visible = norm_x >= 0.0 && norm_x <= 1.0 && norm_y >= 0.0 && norm_y <= 1.0;

            Some(CursorPos {
                x: norm_x.clamp(0.0, 1.0),
                y: norm_y.clamp(0.0, 1.0),
                cursor_id: CGSCurrentCursorSeed() as u64,
                visible,
            })
        }
    }

    /// Get cursor shape via NSCursor.
    ///
    /// Uses the cursor seed to detect changes. Full shape capture requires
    /// NSCursor API via Objective-C runtime, which is complex. For now,
    /// we provide change detection via seed and a basic arrow cursor.
    pub fn get_cursor_shape() -> Option<CursorShape> {
        // The cursor seed changes whenever the cursor shape changes.
        // This is efficient for change detection, but extracting the
        // actual pixel data requires NSCursor or CGSCopyCurrentCursorData.
        //
        // For now, return None which means the viewer uses its own cursor.
        // Full shape capture can be added later with Objective-C FFI.
        None
    }

    /// Get the current cursor seed (for change detection).
    pub fn cursor_seed() -> u32 {
        unsafe { CGSCurrentCursorSeed() }
    }
}

// ─── Windows ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub mod platform {
    use super::*;
    use std::mem;

    // Win32 FFI types
    #[repr(C)]
    struct POINT {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct CURSORINFO {
        cb_size: u32,
        flags: u32,
        h_cursor: isize,
        pt_screen_pos: POINT,
    }

    #[repr(C)]
    struct ICONINFO {
        f_icon: i32,
        x_hotspot: u32,
        y_hotspot: u32,
        hbm_mask: isize,
        hbm_color: isize,
    }

    #[repr(C)]
    struct BITMAP {
        bm_type: i32,
        bm_width: i32,
        bm_height: i32,
        bm_width_bytes: i32,
        bm_planes: u16,
        bm_bits_pixel: u16,
        bm_bits: *mut u8,
    }

    #[repr(C)]
    struct BITMAPINFOHEADER {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    const CURSOR_SHOWING: u32 = 0x00000001;
    const BI_RGB: u32 = 0;
    const DIB_RGB_COLORS: u32 = 0;

    extern "system" {
        fn GetCursorInfo(pci: *mut CURSORINFO) -> i32;
        fn GetIconInfo(h_icon: isize, piconinfo: *mut ICONINFO) -> i32;
        fn GetObjectW(h: isize, c: i32, pv: *mut BITMAP) -> i32;
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
        fn GetDIBits(
            hdc: isize,
            hbm: isize,
            start: u32,
            clines: u32,
            lpv_bits: *mut u8,
            lpbmi: *mut BITMAPINFOHEADER,
            usage: u32,
        ) -> i32;
        fn DeleteObject(ho: isize) -> i32;
        fn CopyIcon(h_icon: isize) -> isize;
        fn DestroyIcon(h_icon: isize) -> i32;
    }

    /// Get normalized cursor position relative to a monitor.
    pub fn get_cursor_position(
        monitor_x: i32,
        monitor_y: i32,
        monitor_w: u32,
        monitor_h: u32,
    ) -> Option<CursorPos> {
        unsafe {
            let mut ci: CURSORINFO = mem::zeroed();
            ci.cb_size = mem::size_of::<CURSORINFO>() as u32;

            if GetCursorInfo(&mut ci) == 0 {
                return None;
            }

            let visible = (ci.flags & CURSOR_SHOWING) != 0;
            let rel_x = ci.pt_screen_pos.x - monitor_x;
            let rel_y = ci.pt_screen_pos.y - monitor_y;

            let norm_x = rel_x as f64 / monitor_w as f64;
            let norm_y = rel_y as f64 / monitor_h as f64;

            Some(CursorPos {
                x: norm_x.clamp(0.0, 1.0),
                y: norm_y.clamp(0.0, 1.0),
                cursor_id: ci.h_cursor as u64,
                visible,
            })
        }
    }

    /// Get cursor shape data (RGBA pixels, hotspot, dimensions).
    pub fn get_cursor_shape() -> Option<CursorShape> {
        unsafe {
            let mut ci: CURSORINFO = mem::zeroed();
            ci.cb_size = mem::size_of::<CURSORINFO>() as u32;

            if GetCursorInfo(&mut ci) == 0 {
                return None;
            }

            if ci.h_cursor == 0 {
                return None;
            }

            // Copy the cursor handle so we can safely inspect it
            let h_copy = CopyIcon(ci.h_cursor);
            if h_copy == 0 {
                return None;
            }

            let mut ii: ICONINFO = mem::zeroed();
            if GetIconInfo(h_copy, &mut ii) == 0 {
                DestroyIcon(h_copy);
                return None;
            }

            // Get bitmap dimensions from the color bitmap (or mask if no color)
            let bmp_handle = if ii.hbm_color != 0 {
                ii.hbm_color
            } else {
                ii.hbm_mask
            };

            let mut bmp: BITMAP = mem::zeroed();
            if GetObjectW(bmp_handle, mem::size_of::<BITMAP>() as i32, &mut bmp) == 0 {
                if ii.hbm_color != 0 {
                    DeleteObject(ii.hbm_color);
                }
                if ii.hbm_mask != 0 {
                    DeleteObject(ii.hbm_mask);
                }
                DestroyIcon(h_copy);
                return None;
            }

            let width = bmp.bm_width as u32;
            let height = if ii.hbm_color != 0 {
                bmp.bm_height as u32
            } else {
                // Monochrome: mask is double height (AND + XOR)
                (bmp.bm_height / 2) as u32
            };

            if width == 0 || height == 0 || width > 256 || height > 256 {
                if ii.hbm_color != 0 {
                    DeleteObject(ii.hbm_color);
                }
                if ii.hbm_mask != 0 {
                    DeleteObject(ii.hbm_mask);
                }
                DestroyIcon(h_copy);
                return None;
            }

            let mut rgba = vec![0u8; (width * height * 4) as usize];

            if ii.hbm_color != 0 {
                // Color cursor — extract BGRA via GetDIBits
                let hdc = GetDC(0);
                let mut bmi: BITMAPINFOHEADER = mem::zeroed();
                bmi.bi_size = mem::size_of::<BITMAPINFOHEADER>() as u32;
                bmi.bi_width = width as i32;
                bmi.bi_height = -(height as i32); // top-down
                bmi.bi_planes = 1;
                bmi.bi_bit_count = 32;
                bmi.bi_compression = BI_RGB;

                GetDIBits(
                    hdc,
                    ii.hbm_color,
                    0,
                    height,
                    rgba.as_mut_ptr(),
                    &mut bmi,
                    DIB_RGB_COLORS,
                );
                ReleaseDC(0, hdc);

                // Convert BGRA → RGBA in-place
                for pixel in rgba.chunks_exact_mut(4) {
                    pixel.swap(0, 2); // B ↔ R
                }
            } else {
                // Monochrome cursor — convert AND/XOR mask to RGBA
                // For simplicity, create a black cursor with alpha from mask
                let hdc = GetDC(0);
                let mask_height = (bmp.bm_height / 2) as u32;
                let mut mask_data = vec![0u8; (width * mask_height * 4) as usize];

                let mut bmi: BITMAPINFOHEADER = mem::zeroed();
                bmi.bi_size = mem::size_of::<BITMAPINFOHEADER>() as u32;
                bmi.bi_width = width as i32;
                bmi.bi_height = -(mask_height as i32); // top-down, AND mask only
                bmi.bi_planes = 1;
                bmi.bi_bit_count = 32;
                bmi.bi_compression = BI_RGB;

                GetDIBits(
                    hdc,
                    ii.hbm_mask,
                    0,
                    mask_height,
                    mask_data.as_mut_ptr(),
                    &mut bmi,
                    DIB_RGB_COLORS,
                );
                ReleaseDC(0, hdc);

                // AND mask: white (0xFF) = transparent, black (0x00) = opaque
                for i in 0..(width * height) as usize {
                    let mask_px = i * 4;
                    let is_transparent = mask_data[mask_px] == 0xFF;
                    let out_px = i * 4;
                    if is_transparent {
                        rgba[out_px] = 0;
                        rgba[out_px + 1] = 0;
                        rgba[out_px + 2] = 0;
                        rgba[out_px + 3] = 0;
                    } else {
                        rgba[out_px] = 0;
                        rgba[out_px + 1] = 0;
                        rgba[out_px + 2] = 0;
                        rgba[out_px + 3] = 255;
                    }
                }
            }

            // Cleanup
            if ii.hbm_color != 0 {
                DeleteObject(ii.hbm_color);
            }
            if ii.hbm_mask != 0 {
                DeleteObject(ii.hbm_mask);
            }
            DestroyIcon(h_copy);

            Some(CursorShape {
                cursor_id: ci.h_cursor as u64,
                width,
                height,
                hotspot_x: ii.x_hotspot,
                hotspot_y: ii.y_hotspot,
                data: rgba,
            })
        }
    }
}

// ─── Fallback (unsupported platforms) ───────────────────────────

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub mod platform {
    use super::*;

    pub fn get_cursor_position(
        _monitor_x: i32,
        _monitor_y: i32,
        _monitor_w: u32,
        _monitor_h: u32,
    ) -> Option<CursorPos> {
        None
    }

    pub fn get_cursor_shape() -> Option<CursorShape> {
        None
    }
}
