//! Audio playback for remote desktop viewer.
//!
//! Receives Opus-encoded audio frames from the agent and plays them back
//! via SDL2's audio subsystem using `AudioQueue<f32>`.

use tracing::{debug, info, warn};

/// Opus decode constants — must match agent encoding settings.
const OPUS_SAMPLE_RATE: u32 = 48_000;
const OPUS_CHANNELS: u32 = 2;
/// Samples per 20ms Opus frame (960 per channel × 2 channels = 1920 f32s)
const OPUS_FRAME_SAMPLES: usize = 960;

/// Audio player wrapping SDL2 AudioQueue + Opus decoder.
pub struct AudioPlayer {
    decoder: opus::Decoder,
    queue: sdl2::audio::AudioQueue<f32>,
    muted: bool,
    /// PCM decode buffer (reused across frames)
    decode_buf: Vec<f32>,
}

impl AudioPlayer {
    /// Create a new audio player from an SDL2 audio subsystem.
    ///
    /// Opens a new audio device with the desired spec (48kHz stereo f32).
    pub fn new(audio_subsystem: &sdl2::AudioSubsystem) -> Result<Self, String> {
        let decoder = opus::Decoder::new(OPUS_SAMPLE_RATE, opus::Channels::Stereo)
            .map_err(|e| format!("Failed to create Opus decoder: {}", e))?;

        let desired_spec = sdl2::audio::AudioSpecDesired {
            freq: Some(OPUS_SAMPLE_RATE as i32),
            channels: Some(OPUS_CHANNELS as u8),
            samples: Some(OPUS_FRAME_SAMPLES as u16), // 20ms buffer
        };

        let queue = audio_subsystem
            .open_queue::<f32, _>(None, &desired_spec)
            .map_err(|e| format!("Failed to open audio queue: {}", e))?;

        // Start playback immediately — samples will be queued as they arrive
        queue.resume();

        info!(
            "Audio player initialized: {}Hz {}ch",
            OPUS_SAMPLE_RATE, OPUS_CHANNELS
        );

        Ok(Self {
            decoder,
            queue,
            muted: false,
            decode_buf: vec![0.0f32; OPUS_FRAME_SAMPLES * OPUS_CHANNELS as usize],
        })
    }

    /// Decode an Opus frame and queue for playback.
    pub fn play_frame(&mut self, opus_data: &[u8]) {
        if self.muted {
            return;
        }

        match self
            .decoder
            .decode_float(opus_data, &mut self.decode_buf, false)
        {
            Ok(decoded_samples) => {
                // decoded_samples is per-channel count
                let total_floats = decoded_samples * OPUS_CHANNELS as usize;
                let pcm = &self.decode_buf[..total_floats];

                // Queue the samples — SDL2 will play them in order
                if let Err(e) = self.queue.queue_audio(pcm) {
                    warn!("Failed to queue audio: {}", e);
                }

                // If too much audio is buffered (>200ms), clear to reduce latency
                let buffered_bytes = self.queue.size();
                let max_bytes = (OPUS_SAMPLE_RATE as u32 * OPUS_CHANNELS * 4 * 200 / 1000) as u32; // 200ms
                if buffered_bytes > max_bytes {
                    debug!(
                        buffered_bytes,
                        max_bytes, "Audio buffer too large, clearing for latency"
                    );
                    self.queue.clear();
                }
            }
            Err(e) => {
                warn!("Opus decode error: {}", e);
            }
        }
    }

    /// Toggle mute state. Returns the new muted status.
    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        if self.muted {
            self.queue.pause();
            self.queue.clear();
            info!("Audio muted");
        } else {
            self.queue.resume();
            info!("Audio unmuted");
        }
        self.muted
    }

    /// Check if audio is currently muted.
    #[allow(dead_code)]
    pub fn is_muted(&self) -> bool {
        self.muted
    }
}
