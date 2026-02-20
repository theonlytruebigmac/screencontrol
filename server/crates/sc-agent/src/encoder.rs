#![allow(dead_code)]
//! In-process H264 software encoder using the `openh264` crate.
//!
//! Provides a zero-dependency H264 encoding path for macOS and Windows,
//! used as a fallback when FFmpeg is not available. Converts BGRA pixel
//! buffers to YUV I420 and encodes them via Cisco's OpenH264 library.
//!
//! ## Encoder priority (macOS/Windows)
//!
//! 1. FFmpeg with HW acceleration (VideoToolbox / NVENC / QSV)
//! 2. **This module** — in-process OpenH264 software encoder
//! 3. JPEG fallback (no H264)

use openh264::encoder::{
    BitRate, Complexity, EncoderConfig, FrameRate, FrameType, RateControlMode, SpsPpsStrategy,
    UsageType,
};
use openh264::formats::YUVSource;
use openh264::OpenH264API;

/// In-process H264 software encoder wrapping OpenH264.
///
/// Accepts BGRA pixel buffers, converts to YUV I420, and produces
/// Annex B H264 NAL units ready for WebSocket transmission.
pub struct SoftwareEncoder {
    encoder: openh264::encoder::Encoder,
    width: u32,
    height: u32,
    /// Pre-allocated YUV I420 buffer to avoid per-frame allocation
    yuv_buf: YuvI420Buffer,
    /// Number of frames encoded (for logging)
    frame_count: u64,
}

/// Pre-allocated YUV I420 planar buffer.
struct YuvI420Buffer {
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
    width: usize,
    height: usize,
}

impl YUVSource for YuvI420Buffer {
    fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn strides(&self) -> (usize, usize, usize) {
        (self.width, self.width / 2, self.width / 2)
    }

    fn y(&self) -> &[u8] {
        &self.y
    }

    fn u(&self) -> &[u8] {
        &self.u
    }

    fn v(&self) -> &[u8] {
        &self.v
    }
}

impl YuvI420Buffer {
    fn new(width: usize, height: usize) -> Self {
        let y_size = width * height;
        let uv_size = (width / 2) * (height / 2);
        Self {
            y: vec![0u8; y_size],
            u: vec![0u8; uv_size],
            v: vec![0u8; uv_size],
            width,
            height,
        }
    }

    /// Convert BGRA pixel data to YUV I420 in-place.
    ///
    /// Uses BT.601 coefficients (standard for screen content):
    ///   Y  =  0.299*R + 0.587*G + 0.114*B
    ///   Cb = -0.169*R - 0.331*G + 0.500*B + 128
    ///   Cr =  0.500*R - 0.419*G - 0.081*B + 128
    fn fill_from_bgra(&mut self, bgra: &[u8]) {
        let w = self.width;
        let h = self.height;

        // Y plane: full resolution
        for row in 0..h {
            let bgra_row = row * w * 4;
            let y_row = row * w;
            for col in 0..w {
                let px = bgra_row + col * 4;
                let b = bgra[px] as i32;
                let g = bgra[px + 1] as i32;
                let r = bgra[px + 2] as i32;
                // BT.601: Y = 0.299R + 0.587G + 0.114B
                // Use fixed-point: (66*R + 129*G + 25*B + 128) >> 8 + 16
                self.y[y_row + col] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16) as u8;
            }
        }

        // U (Cb) and V (Cr) planes: half resolution (2x2 averaging)
        let half_w = w / 2;
        for row in 0..(h / 2) {
            let src_row0 = (row * 2) * w * 4;
            let src_row1 = (row * 2 + 1) * w * 4;
            let uv_row = row * half_w;

            for col in 0..half_w {
                let px00 = src_row0 + (col * 2) * 4;
                let px01 = src_row0 + (col * 2 + 1) * 4;
                let px10 = src_row1 + (col * 2) * 4;
                let px11 = src_row1 + (col * 2 + 1) * 4;

                // Average 2x2 block
                let b =
                    (bgra[px00] as i32 + bgra[px01] as i32 + bgra[px10] as i32 + bgra[px11] as i32)
                        / 4;
                let g = (bgra[px00 + 1] as i32
                    + bgra[px01 + 1] as i32
                    + bgra[px10 + 1] as i32
                    + bgra[px11 + 1] as i32)
                    / 4;
                let r = (bgra[px00 + 2] as i32
                    + bgra[px01 + 2] as i32
                    + bgra[px10 + 2] as i32
                    + bgra[px11 + 2] as i32)
                    / 4;

                // BT.601 Cb = (-38*R - 74*G + 112*B + 128) >> 8 + 128
                self.u[uv_row + col] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128) as u8;
                // BT.601 Cr = (112*R - 94*G - 18*B + 128) >> 8 + 128
                self.v[uv_row + col] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128) as u8;
            }
        }
    }
}

/// Result of encoding a single frame.
pub struct EncodedFrame {
    /// Annex B H264 NAL unit data
    pub data: Vec<u8>,
    /// Whether this frame is a keyframe (IDR)
    pub is_keyframe: bool,
}

impl SoftwareEncoder {
    /// Create a new in-process H264 encoder.
    ///
    /// # Arguments
    /// * `width` — Frame width in pixels (must be even)
    /// * `height` — Frame height in pixels (must be even)
    /// * `bitrate_kbps` — Target bitrate in kilobits per second
    /// * `fps` — Target frame rate
    pub fn new(width: u32, height: u32, bitrate_kbps: u32, fps: u32) -> Result<Self, String> {
        // OpenH264 requires even dimensions
        let width = width & !1;
        let height = height & !1;

        if width == 0 || height == 0 {
            return Err("Encoder dimensions must be non-zero".to_string());
        }

        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(bitrate_kbps * 1000))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .usage_type(UsageType::ScreenContentRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .complexity(Complexity::Low)
            .skip_frames(false)
            .sps_pps_strategy(SpsPpsStrategy::ConstantId);

        let api = OpenH264API::from_source();
        let encoder = openh264::encoder::Encoder::with_api_config(api, config)
            .map_err(|e| format!("Failed to create OpenH264 encoder: {e}"))?;

        let yuv_buf = YuvI420Buffer::new(width as usize, height as usize);

        tracing::info!(
            width,
            height,
            bitrate_kbps,
            fps,
            "OpenH264 software encoder initialized"
        );

        Ok(Self {
            encoder,
            width,
            height,
            yuv_buf,
            frame_count: 0,
        })
    }

    /// Encode a BGRA pixel buffer into an H264 NAL unit.
    ///
    /// Returns `None` if the encoder skips the frame (rate control).
    /// Returns `Some(EncodedFrame)` with Annex B NAL data on success.
    pub fn encode_bgra(&mut self, bgra: &[u8]) -> Result<Option<EncodedFrame>, String> {
        let expected_len = (self.width * self.height * 4) as usize;
        if bgra.len() < expected_len {
            return Err(format!(
                "BGRA buffer too small: got {}, expected {} ({}x{})",
                bgra.len(),
                expected_len,
                self.width,
                self.height
            ));
        }

        // Convert BGRA → YUV I420
        self.yuv_buf.fill_from_bgra(bgra);

        // Encode
        let bitstream = self
            .encoder
            .encode(&self.yuv_buf)
            .map_err(|e| format!("OpenH264 encode error: {e}"))?;

        let frame_type = bitstream.frame_type();
        if matches!(frame_type, FrameType::Skip) {
            return Ok(None);
        }

        let mut nal_data = Vec::with_capacity(expected_len / 10);
        bitstream.write_vec(&mut nal_data);

        if nal_data.is_empty() {
            return Ok(None);
        }

        self.frame_count += 1;
        let is_keyframe = matches!(frame_type, FrameType::IDR | FrameType::I);

        if self.frame_count <= 3 || self.frame_count % 60 == 0 {
            tracing::info!(
                frame = self.frame_count,
                nal_bytes = nal_data.len(),
                is_keyframe,
                ?frame_type,
                "OpenH264 encoded frame"
            );
        }

        Ok(Some(EncodedFrame {
            data: nal_data,
            is_keyframe,
        }))
    }

    /// Request the next frame be a keyframe (IDR).
    pub fn force_keyframe(&mut self) {
        self.encoder.force_intra_frame();
        tracing::debug!("OpenH264: forced keyframe request");
    }

    /// Get the current frame dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get the total number of frames encoded.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}
