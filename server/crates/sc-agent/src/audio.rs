//! Audio capture and streaming for remote desktop sessions.
//!
//! Uses `cpal` for cross-platform audio loopback capture and `opus` for encoding.
//! Captured audio is encoded as Opus frames and sent as `AudioFrame` protobuf
//! envelopes via the same `UnboundedSender<Vec<u8>>` used by video frames.
//!
//! Platform behavior:
//! - **Linux**: Captures from PulseAudio monitor device (system audio output)
//! - **macOS**: Captures from default input device (virtual audio may be needed)
//! - **Windows**: Captures from WASAPI loopback (default output device)

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use prost::Message as ProstMessage;
use tokio::sync::mpsc;
use uuid::Uuid;

use sc_protocol::{envelope, AudioFrame, Envelope};

/// Opus frame duration in milliseconds (20ms is standard).
const OPUS_FRAME_MS: usize = 20;
/// Opus sample rate — always 48kHz for best quality.
const OPUS_SAMPLE_RATE: u32 = 48_000;
/// Number of channels (stereo).
const OPUS_CHANNELS: u32 = 2;
/// Samples per Opus frame (48000 * 20ms / 1000 = 960).
const OPUS_FRAME_SAMPLES: usize = (OPUS_SAMPLE_RATE as usize * OPUS_FRAME_MS) / 1000;

/// Manages audio capture sessions, one per desktop session.
pub struct AudioCapturer {
    sessions: HashMap<String, AudioSession>,
}

struct AudioSession {
    /// Signal to stop the capture thread.
    stop_flag: Arc<AtomicBool>,
    /// Thread handle for the capture loop.
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl AudioCapturer {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Start capturing audio for a desktop session.
    ///
    /// Audio frames are sent as protobuf envelopes via `tx`.
    pub fn start_capture(&mut self, session_id: &str, tx: mpsc::UnboundedSender<Vec<u8>>) {
        // Stop existing capture for this session if any
        self.stop_capture(session_id);

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_flag.clone();
        let sid = session_id.to_string();

        let thread = std::thread::Builder::new()
            .name(format!("audio-capture-{}", &sid[..8.min(sid.len())]))
            .spawn(move || {
                if let Err(e) = run_capture_loop(&sid, stop_clone, tx) {
                    tracing::warn!(session_id=%sid, "Audio capture ended with error: {}", e);
                } else {
                    tracing::info!(session_id=%sid, "Audio capture ended cleanly");
                }
            })
            .expect("Failed to spawn audio capture thread");

        self.sessions.insert(
            session_id.to_string(),
            AudioSession {
                stop_flag,
                thread_handle: Some(thread),
            },
        );

        tracing::info!(session_id=%session_id, "Audio capture started");
    }

    /// Stop capturing audio for a desktop session.
    pub fn stop_capture(&mut self, session_id: &str) {
        if let Some(mut session) = self.sessions.remove(session_id) {
            session.stop_flag.store(true, Ordering::Relaxed);
            if let Some(handle) = session.thread_handle.take() {
                // Don't block forever — the thread should exit within a few hundred ms
                let _ = handle.join();
            }
            tracing::info!(session_id=%session_id, "Audio capture stopped");
        }
    }

    /// Stop all active audio captures.
    pub fn stop_all(&mut self) {
        let ids: Vec<String> = self.sessions.keys().cloned().collect();
        for id in ids {
            self.stop_capture(&id);
        }
    }
}

impl Drop for AudioCapturer {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// Core capture loop running on a dedicated thread.
///
/// Uses cpal to capture audio from the default output device (loopback mode
/// on Windows/Linux) or default input device (macOS).
fn run_capture_loop(
    session_id: &str,
    stop_flag: Arc<AtomicBool>,
    tx: mpsc::UnboundedSender<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use cpal::traits::{DeviceTrait, StreamTrait};

    let host = cpal::default_host();

    // Try to get a loopback/monitor device for system audio
    let device = get_capture_device(&host)?;
    let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
    tracing::info!(
        session_id=%session_id,
        device=%device_name,
        "Using audio capture device"
    );

    let supported_config = device.default_input_config()?;
    let sample_rate = supported_config.sample_rate().0;
    let channels = supported_config.channels() as u32;
    let sample_format = supported_config.sample_format();

    tracing::info!(
        session_id=%session_id,
        sample_rate=%sample_rate,
        channels=%channels,
        sample_format=?sample_format,
        "Audio device config"
    );

    // Create Opus encoder — always encode at 48kHz stereo
    let mut encoder = opus::Encoder::new(
        OPUS_SAMPLE_RATE,
        opus::Channels::Stereo,
        opus::Application::Audio,
    )?;
    // Set a reasonable bitrate (64kbps for stereo)
    encoder.set_bitrate(opus::Bitrate::Bits(64_000))?;

    let sequence = Arc::new(AtomicU32::new(0));

    // Accumulator for PCM samples (resampled to 48kHz stereo)
    // We need OPUS_FRAME_SAMPLES * 2 (stereo) f32 samples per Opus frame
    let samples_needed = OPUS_FRAME_SAMPLES * OPUS_CHANNELS as usize;
    let pcm_buf: Arc<std::sync::Mutex<Vec<f32>>> = Arc::new(std::sync::Mutex::new(
        Vec::with_capacity(samples_needed * 4),
    ));

    let pcm_buf_writer = pcm_buf.clone();
    let stop_for_stream = stop_flag.clone();
    let src_rate = sample_rate;
    let src_channels = channels;

    // Build the input stream
    let config = cpal::StreamConfig {
        channels: channels as u16,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if stop_for_stream.load(Ordering::Relaxed) {
                    return;
                }
                let resampled = resample_to_stereo_48k(data, src_rate, src_channels);
                if let Ok(mut buf) = pcm_buf_writer.lock() {
                    buf.extend_from_slice(&resampled);
                }
            },
            |err| tracing::warn!("Audio stream error: {}", err),
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                if stop_for_stream.load(Ordering::Relaxed) {
                    return;
                }
                let floats: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                let resampled = resample_to_stereo_48k(&floats, src_rate, src_channels);
                if let Ok(mut buf) = pcm_buf_writer.lock() {
                    buf.extend_from_slice(&resampled);
                }
            },
            |err| tracing::warn!("Audio stream error: {}", err),
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                if stop_for_stream.load(Ordering::Relaxed) {
                    return;
                }
                let floats: Vec<f32> = data.iter().map(|&s| (s as f32 / 32768.0) - 1.0).collect();
                let resampled = resample_to_stereo_48k(&floats, src_rate, src_channels);
                if let Ok(mut buf) = pcm_buf_writer.lock() {
                    buf.extend_from_slice(&resampled);
                }
            },
            |err| tracing::warn!("Audio stream error: {}", err),
            None,
        )?,
        fmt => {
            return Err(format!("Unsupported sample format: {:?}", fmt).into());
        }
    };

    stream.play()?;
    tracing::info!(session_id=%session_id, "Audio stream playing");

    // Encoding loop: pull from accumulator, encode Opus frames, send
    let mut opus_output = vec![0u8; 4000]; // max Opus frame ~4KB
    let sid = session_id.to_string();

    while !stop_flag.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(5));

        // Drain accumulated samples and encode Opus frames
        let mut samples_to_encode = Vec::new();
        if let Ok(mut buf) = pcm_buf.lock() {
            if buf.len() >= samples_needed {
                let drain_len = buf.len() - (buf.len() % samples_needed);
                samples_to_encode = buf.drain(..drain_len).collect();
            }
        }

        // Encode in chunks of OPUS_FRAME_SAMPLES * CHANNELS
        for chunk in samples_to_encode.chunks_exact(samples_needed) {
            match encoder.encode_float(chunk, &mut opus_output) {
                Ok(encoded_len) => {
                    let seq = sequence.fetch_add(1, Ordering::Relaxed);
                    let frame = AudioFrame {
                        data: opus_output[..encoded_len].to_vec(),
                        sample_rate: OPUS_SAMPLE_RATE,
                        channels: OPUS_CHANNELS,
                        sequence: seq,
                    };
                    let envelope = Envelope {
                        id: Uuid::new_v4().to_string(),
                        session_id: sid.clone(),
                        timestamp: None,
                        payload: Some(envelope::Payload::AudioFrame(frame)),
                    };
                    let mut buf = Vec::with_capacity(envelope.encoded_len());
                    if envelope.encode(&mut buf).is_ok() {
                        if tx.send(buf).is_err() {
                            tracing::debug!("Audio tx channel closed");
                            return Ok(());
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Opus encode error: {}", e);
                }
            }
        }
    }

    // Stream automatically stops when dropped
    drop(stream);
    tracing::info!(session_id=%session_id, "Audio capture loop exiting");
    Ok(())
}

/// Get the best audio capture device for system audio loopback.
#[allow(unreachable_code)]
fn get_capture_device(host: &cpal::Host) -> Result<cpal::Device, Box<dyn std::error::Error>> {
    use cpal::traits::{DeviceTrait, HostTrait};

    // On Linux with PulseAudio/PipeWire, look for a monitor device
    #[cfg(target_os = "linux")]
    {
        if let Ok(devices) = host.input_devices() {
            for device in devices {
                if let Ok(name) = device.name() {
                    // PulseAudio/PipeWire monitor devices have ".monitor" suffix
                    // or contain "Monitor of" in the description
                    if name.contains(".monitor") || name.contains("Monitor") {
                        tracing::info!("Found audio monitor device: {}", name);
                        return Ok(device);
                    }
                }
            }
        }
        // CRITICAL: Do NOT fall back to default input device on Linux.
        // The default input is typically a microphone or webcam, which would
        // activate the camera LED and capture the user's audio/video — a
        // serious privacy violation. Instead, return an error so audio capture
        // is silently skipped for this session.
        tracing::warn!(
            "No audio monitor/loopback device found — audio capture disabled. \
             Install PulseAudio or PipeWire with pipewire-pulse for system audio."
        );
        return Err("No audio monitor device found (refusing to use microphone/webcam)".into());
    }

    // On Windows, try to find a loopback device
    // cpal on WASAPI doesn't directly expose loopback; we use default output
    // as input for monitoring (requires WASAPI loopback support)
    #[cfg(target_os = "windows")]
    {
        // Try output device first for WASAPI loopback
        if let Some(device) = host.default_output_device() {
            tracing::info!("Using default output device for WASAPI loopback");
            return Ok(device);
        }
    }

    // Fallback: default input device (macOS and others)
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        return host
            .default_input_device()
            .ok_or_else(|| "No audio input device available".into());
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    Err("No suitable audio capture device found".into())
}

/// Resample interleaved PCM from arbitrary rate/channels to 48kHz stereo f32.
///
/// Uses simple linear interpolation for rate conversion and channel remapping.
fn resample_to_stereo_48k(samples: &[f32], src_rate: u32, src_channels: u32) -> Vec<f32> {
    if src_rate == OPUS_SAMPLE_RATE && src_channels == OPUS_CHANNELS {
        return samples.to_vec();
    }

    let src_frames = samples.len() / src_channels as usize;
    if src_frames == 0 {
        return Vec::new();
    }

    // Calculate output frame count
    let dst_frames = (src_frames as u64 * OPUS_SAMPLE_RATE as u64 / src_rate as u64) as usize;
    let mut output = Vec::with_capacity(dst_frames * 2); // stereo output

    for i in 0..dst_frames {
        // Map output frame index to input frame index
        let src_pos = i as f64 * src_rate as f64 / OPUS_SAMPLE_RATE as f64;
        let src_idx = src_pos as usize;
        let frac = src_pos - src_idx as f64;

        // Get left and right channels from source
        let (left, right) = if src_channels == 1 {
            // Mono → duplicate to both channels
            let s0 = samples.get(src_idx).copied().unwrap_or(0.0);
            let s1 = samples.get(src_idx + 1).copied().unwrap_or(s0);
            let val = s0 + (s1 - s0) * frac as f32;
            (val, val)
        } else {
            // Multi-channel → take first two channels
            let base0 = src_idx * src_channels as usize;
            let base1 = (src_idx + 1).min(src_frames - 1) * src_channels as usize;

            let l0 = samples.get(base0).copied().unwrap_or(0.0);
            let l1 = samples.get(base1).copied().unwrap_or(l0);
            let r0 = samples.get(base0 + 1).copied().unwrap_or(l0);
            let r1 = samples.get(base1 + 1).copied().unwrap_or(r0);

            let left = l0 + (l1 - l0) * frac as f32;
            let right = r0 + (r1 - r0) * frac as f32;
            (left, right)
        };

        output.push(left);
        output.push(right);
    }

    output
}
