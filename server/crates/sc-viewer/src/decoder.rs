//! H.264 decoder using ffmpeg (libavcodec).
//!
//! Decodes H.264 Annex-B NAL units into YUV420p frames for SDL2 rendering.

use anyhow::{bail, Context, Result};
use tracing::{debug, info, warn};

/// Decoded video frame in YUV420p format
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    /// Y plane data
    pub y: Vec<u8>,
    /// U plane data (quarter resolution)
    pub u: Vec<u8>,
    /// V plane data (quarter resolution)
    pub v: Vec<u8>,
    /// Y plane stride (bytes per row, may be > width due to padding)
    pub y_stride: usize,
    /// U/V plane stride
    pub uv_stride: usize,
}

/// H.264 decoder wrapping ffmpeg's libavcodec
pub struct H264Decoder {
    decoder: ffmpeg_next::decoder::Video,
    context_opened: bool,
}

impl H264Decoder {
    /// Create a new H.264 decoder.
    pub fn new() -> Result<Self> {
        ffmpeg_next::init().context("Failed to initialize ffmpeg")?;

        let codec = ffmpeg_next::decoder::find(ffmpeg_next::codec::Id::H264)
            .context("H.264 codec not found — ensure ffmpeg is installed with libx264")?;

        info!("Using H.264 decoder: {}", codec.name());

        let mut context = ffmpeg_next::codec::Context::new_with_codec(codec);

        // Configure for low-latency decoding
        unsafe {
            let ctx = &mut *context.as_mut_ptr();
            ctx.flags |= ffmpeg_next::sys::AV_CODEC_FLAG_LOW_DELAY as i32;
            ctx.flags2 |= ffmpeg_next::sys::AV_CODEC_FLAG2_FAST as i32;
        }

        let decoder = context
            .decoder()
            .video()
            .context("Failed to open H.264 decoder")?;

        info!("H.264 decoder initialized successfully");

        Ok(Self {
            decoder,
            context_opened: true,
        })
    }

    /// Decode an H.264 Annex-B access unit (may contain multiple NAL units).
    /// Returns decoded frames (usually 0 or 1 per call).
    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<DecodedFrame>> {
        if !self.context_opened {
            bail!("Decoder not initialized");
        }

        let packet = ffmpeg_next::Packet::copy(data);

        self.decoder
            .send_packet(&packet)
            .context("Failed to send packet to decoder")?;

        let mut frames = Vec::new();
        let mut decoded_frame = ffmpeg_next::frame::Video::empty();

        while self.decoder.receive_frame(&mut decoded_frame).is_ok() {
            let width = decoded_frame.width();
            let height = decoded_frame.height();

            // Ensure output is YUV420p
            if decoded_frame.format() != ffmpeg_next::format::Pixel::YUV420P {
                warn!(
                    "Unexpected pixel format: {:?}, expected YUV420P",
                    decoded_frame.format()
                );
                // TODO: Add swscale conversion if needed
                continue;
            }

            let y_stride = decoded_frame.stride(0);
            let u_stride = decoded_frame.stride(1);

            let y_data = decoded_frame.data(0);
            let u_data = decoded_frame.data(1);
            let v_data = decoded_frame.data(2);

            // Copy plane data (ffmpeg planes may have padding)
            let y_size = y_stride * height as usize;
            let uv_size = u_stride * (height as usize / 2);

            frames.push(DecodedFrame {
                width,
                height,
                y: y_data[..y_size].to_vec(),
                u: u_data[..uv_size].to_vec(),
                v: v_data[..uv_size].to_vec(),
                y_stride,
                uv_stride: u_stride,
            });

            debug!("Decoded frame: {}x{}", width, height);
        }

        Ok(frames)
    }

    /// Flush the decoder to get any remaining frames.
    #[allow(dead_code)]
    pub fn flush(&mut self) -> Result<Vec<DecodedFrame>> {
        self.decoder
            .send_eof()
            .context("Failed to send EOF to decoder")?;

        let mut frames = Vec::new();
        let mut decoded_frame = ffmpeg_next::frame::Video::empty();

        while self.decoder.receive_frame(&mut decoded_frame).is_ok() {
            let width = decoded_frame.width();
            let height = decoded_frame.height();

            if decoded_frame.format() != ffmpeg_next::format::Pixel::YUV420P {
                continue;
            }

            let y_stride = decoded_frame.stride(0);
            let u_stride = decoded_frame.stride(1);

            frames.push(DecodedFrame {
                width,
                height,
                y: decoded_frame.data(0)[..y_stride * height as usize].to_vec(),
                u: decoded_frame.data(1)[..u_stride * (height as usize / 2)].to_vec(),
                v: decoded_frame.data(2)[..u_stride * (height as usize / 2)].to_vec(),
                y_stride,
                uv_stride: u_stride,
            });
        }

        Ok(frames)
    }
}

impl Drop for H264Decoder {
    fn drop(&mut self) {
        self.context_opened = false;
        info!("H.264 decoder closed");
    }
}
