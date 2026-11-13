//! Core audio capture implementation.
//!
//! [`AudioCaptureManager`] is the main public handle for starting and stopping
//! capture sessions. It spawns a dedicated OS thread (not a Tokio task) for
//! each active capture stream because WASAPI requires a synchronous,
//! low-latency event loop that must not be blocked by the async runtime.
//!
//! Captured audio is forwarded to the preprocessing module via a bounded
//! Tokio [`mpsc`] channel whose sender is owned by the capture threads and
//! whose receiver is returned to the caller.
//!
//! ## WASAPI loopback setup
//! To capture loopback audio (system output) with wasapi 0.14, the
//! `AudioClient` is opened on the *Render* endpoint but `initialize_client`
//! is called with `direction = Direction::Capture`.  The crate internally
//! sets `AUDCLNT_STREAMFLAGS_LOOPBACK` when it detects this combination.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use wasapi::{Direction, ShareMode};

use crate::audio_capture::{
    config::{CaptureConfig, DeviceSelection},
    device::DeviceEnumerator,
    error::CaptureError,
    types::{unix_ms_now, AudioChunk, AudioSource},
};

// ── Capture state ─────────────────────────────────────────────────────────────

/// Tracks whether a capture thread should keep running.
type StopFlag = Arc<AtomicBool>;

/// Handle returned by [`AudioCaptureManager::start`].
///
/// Dropping this handle — or calling [`CaptureHandle::stop`] — signals both
/// capture threads to exit cleanly and joins them.
pub struct CaptureHandle {
    stop_flag: StopFlag,
    loopback_thread: Option<thread::JoinHandle<()>>,
    mic_thread: Option<thread::JoinHandle<()>>,
}

impl CaptureHandle {
    /// Signal the capture threads to stop and wait for them to exit.
    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        for handle in [self.loopback_thread.take(), self.mic_thread.take()]
            .into_iter()
            .flatten()
        {
            if let Err(e) = handle.join() {
                error!("Capture thread panicked: {e:?}");
            }
        }
        info!("Audio capture stopped.");
    }
}

impl Drop for CaptureHandle {
    /// Automatically stop capture when the handle goes out of scope.
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        for handle in [self.loopback_thread.take(), self.mic_thread.take()]
            .into_iter()
            .flatten()
        {
            let _ = handle.join();
        }
    }
}

// ── Manager ───────────────────────────────────────────────────────────────────

/// Primary handle for the audio capture module.
///
/// # Example
/// ```rust,ignore
/// let config = CaptureConfig::default();
/// let (handle, mut rx) = AudioCaptureManager::start(config)?;
///
/// while let Some(chunk) = rx.recv().await {
///     // forward chunk to preprocessing
/// }
///
/// handle.stop(); // or just drop it
/// ```
pub struct AudioCaptureManager;

impl AudioCaptureManager {
    /// Start audio capture according to `config`.
    ///
    /// Returns:
    /// - A [`CaptureHandle`] that keeps the capture threads alive. Drop or
    ///   call `.stop()` to end capture.
    /// - A [`mpsc::Receiver<AudioChunk>`] that delivers PCM chunks to the
    ///   preprocessing module.
    pub fn start(
        config: CaptureConfig,
    ) -> Result<(CaptureHandle, mpsc::Receiver<AudioChunk>), CaptureError> {
        info!("Starting audio capture (mic={})", config.enable_microphone);

        let (tx, rx) = mpsc::channel::<AudioChunk>(config.channel_capacity);

        let stop_flag: StopFlag = Arc::new(AtomicBool::new(false));

        // ── Loopback thread ──────────────────────────────────────────────────
        let loopback_device = match &config.loopback_device {
            DeviceSelection::Default => DeviceEnumerator::default_output_device()?,
            DeviceSelection::ById(id) => {
                DeviceEnumerator::find_output_device_by_id(id)?
            }
        };

        info!(device = %loopback_device.name, "Opening loopback capture on device");

        let lb_tx = tx.clone();
        let lb_stop = stop_flag.clone();
        let lb_chunk_frames = config.target_chunk_frames;
        let lb_device_id = loopback_device.id.clone();
        let lb_device_name = loopback_device.name.clone();

        let loopback_thread = thread::Builder::new()
            .name("voxy-loopback-capture".into())
            .spawn(move || {
                if let Err(e) = loopback_capture_loop(
                    &lb_device_id,
                    &lb_device_name,
                    lb_chunk_frames,
                    lb_tx,
                    lb_stop,
                ) {
                    error!("Loopback capture error: {e}");
                }
            })
            .map_err(|e| CaptureError::TaskError(e.to_string()))?;

        // ── Optional microphone thread ───────────────────────────────────────
        let mic_thread = if config.enable_microphone {
            let mic_device = match &config.microphone_device {
                DeviceSelection::Default => {
                    DeviceEnumerator::default_input_device()
                        .map_err(|e| {
                            warn!("Microphone not available: {e}");
                            e
                        })
                        .ok()
                }
                DeviceSelection::ById(id) => {
                    DeviceEnumerator::find_input_device_by_id(id)
                        .map_err(|e| {
                            warn!("Requested microphone not found: {e}");
                            e
                        })
                        .ok()
                }
            };

            if let Some(device) = mic_device {
                info!(device = %device.name, "Opening microphone capture on device");

                let mic_tx = tx.clone();
                let mic_stop = stop_flag.clone();
                let mic_chunk_frames = config.target_chunk_frames;
                let mic_device_id = device.id.clone();
                let mic_device_name = device.name.clone();

                let handle = thread::Builder::new()
                    .name("voxy-microphone-capture".into())
                    .spawn(move || {
                        if let Err(e) = microphone_capture_loop(
                            &mic_device_id,
                            &mic_device_name,
                            mic_chunk_frames,
                            mic_tx,
                            mic_stop,
                        ) {
                            error!("Microphone capture error: {e}");
                        }
                    })
                    .map_err(|e| CaptureError::TaskError(e.to_string()))?;

                Some(handle)
            } else {
                warn!("Microphone capture requested but no device was available; continuing with loopback only.");
                None
            }
        } else {
            None
        };

        // Drop our clone of `tx`; the threads now own the only senders.
        drop(tx);

        Ok((
            CaptureHandle {
                stop_flag,
                loopback_thread: Some(loopback_thread),
                mic_thread,
            },
            rx,
        ))
    }
}

// ── Loopback capture loop ─────────────────────────────────────────────────────

/// Runs in a dedicated OS thread. Opens the named device for WASAPI loopback
/// capture and delivers PCM chunks over `tx` until `stop_flag` is set.
///
/// WASAPI loopback: open a Render endpoint, initialize the AudioClient with
/// `direction = Capture` in Shared mode.  The wasapi crate automatically sets
/// `AUDCLNT_STREAMFLAGS_LOOPBACK` for this combination.
fn loopback_capture_loop(
    device_id: &str,
    device_name: &str,
    target_chunk_frames: usize,
    tx: mpsc::Sender<AudioChunk>,
    stop_flag: StopFlag,
) -> Result<(), CaptureError> {
    wasapi::initialize_mta()
        .map_err(|e| CaptureError::Wasapi(format!("COM init: {e}")))?;

    // Find the render endpoint by ID in the collection.
    let device = find_device_by_id_in_collection(device_id, &Direction::Render)?;

    let mut audio_client = device
        .get_iaudioclient()
        .map_err(|e| CaptureError::Wasapi(format!("get_iaudioclient: {e}")))?;

    // Use the device's own mix format; resampling is handled by the
    // preprocessing module (Member 4).
    let mix_format = audio_client
        .get_mixformat()
        .map_err(|e| CaptureError::Wasapi(format!("get_mixformat: {e}")))?;

    let sample_rate_hz = mix_format.get_samplespersec();
    let channels = mix_format.get_nchannels() as usize;
    // bytes_per_frame = channels × (bits_per_sample / 8)
    let bytes_per_frame = mix_format.get_blockalign() as usize;

    debug!(
        device = device_name,
        rate = sample_rate_hz,
        channels = channels,
        bytes_per_frame = bytes_per_frame,
        "Loopback mix format"
    );

    // Buffer duration: 10 ms (100-ns units: 10 ms × 10 000 = 100 000).
    let buffer_duration_hns: i64 = 100_000;

    // Direction::Capture on a Render device → wasapi sets LOOPBACK flag.
    audio_client
        .initialize_client(
            &mix_format,
            buffer_duration_hns,
            &Direction::Capture,
            &ShareMode::Shared,
            false,
        )
        .map_err(|e| {
            CaptureError::Wasapi(format!("initialize_client (loopback): {e}"))
        })?;

    let event_handle = audio_client
        .set_get_eventhandle()
        .map_err(|e| CaptureError::Wasapi(format!("set_get_eventhandle: {e}")))?;

    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|e| CaptureError::Wasapi(format!("get_audiocaptureclient: {e}")))?;

    audio_client
        .start_stream()
        .map_err(|e| CaptureError::Wasapi(format!("start_stream: {e}")))?;

    info!(device = device_name, "Loopback capture started");

    // Max frames per WASAPI buffer call. We read the actual buffer size after init.
    let max_buffer_frames = audio_client
        .get_bufferframecount()
        .map_err(|e| CaptureError::Wasapi(format!("get_bufferframecount: {e}")))?
        as usize;

    // Raw byte buffer sized for the max number of frames we can receive in one call.
    let max_bytes = max_buffer_frames * bytes_per_frame;
    let mut raw_buf: Vec<u8> = vec![0u8; max_bytes];

    let mut sample_buffer: Vec<f32> = Vec::with_capacity(target_chunk_frames * 2);

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // Wait up to 200 ms for new audio data to arrive.
        match event_handle.wait_for_event(200) {
            Ok(()) => {}
            Err(_) => {
                // Timeout — check stop flag and retry.
                continue;
            }
        }

        // Drain all available packets from this wake-up.
        loop {
            let available_frames = match capture_client.get_next_nbr_frames() {
                Ok(Some(n)) => n as usize,
                Ok(None) => 0,
                Err(e) => {
                    let msg = format!("{e}");
                    // AUDCLNT_E_DEVICE_INVALIDATED (0x88890004) means disconnected.
                    if msg.contains("INVALIDATED") || msg.contains("88890004") {
                        return Err(CaptureError::DeviceDisconnected(
                            device_name.to_string(),
                        ));
                    }
                    return Err(CaptureError::Wasapi(msg));
                }
            };

            if available_frames == 0 {
                break;
            }

            // Ensure our buffer is large enough.
            let needed_bytes = available_frames * bytes_per_frame;
            if raw_buf.len() < needed_bytes {
                raw_buf.resize(needed_bytes, 0u8);
            }

            match capture_client.read_from_device(bytes_per_frame, &mut raw_buf) {
                Ok((frames_read, _flags)) if frames_read > 0 => {
                    let byte_count = frames_read as usize * bytes_per_frame;
                    let samples =
                        bytes_to_f32_mono(&raw_buf[..byte_count], channels, bytes_per_frame);
                    sample_buffer.extend_from_slice(&samples);
                }
                Ok(_) => break, // 0 frames — nothing to read right now
                Err(e) => {
                    warn!("loopback read_from_device: {e}");
                    break;
                }
            }
        }

        // Deliver complete chunks to the downstream channel.
        while sample_buffer.len() >= target_chunk_frames {
            let chunk_samples: Vec<f32> =
                sample_buffer.drain(..target_chunk_frames).collect();

            let chunk = AudioChunk::new(
                chunk_samples,
                sample_rate_hz,
                unix_ms_now(),
                AudioSource::Loopback,
            );

            if tx.blocking_send(chunk).is_err() {
                // Receiver dropped — session ended.
                info!("Loopback channel closed; stopping capture thread.");
                let _ = audio_client.stop_stream();
                return Ok(());
            }
        }
    }

    audio_client
        .stop_stream()
        .map_err(|e| CaptureError::Wasapi(format!("stop_stream: {e}")))?;

    info!(device = device_name, "Loopback capture stopped cleanly");
    Ok(())
}

// ── Microphone capture loop ───────────────────────────────────────────────────

/// Runs in a dedicated OS thread. Opens the named Capture endpoint for
/// standard WASAPI shared-mode capture (microphone).
fn microphone_capture_loop(
    device_id: &str,
    device_name: &str,
    target_chunk_frames: usize,
    tx: mpsc::Sender<AudioChunk>,
    stop_flag: StopFlag,
) -> Result<(), CaptureError> {
    wasapi::initialize_mta()
        .map_err(|e| CaptureError::Wasapi(format!("COM init (mic): {e}")))?;

    let device = find_device_by_id_in_collection(device_id, &Direction::Capture)?;

    let mut audio_client = device
        .get_iaudioclient()
        .map_err(|e| CaptureError::Wasapi(format!("get_iaudioclient (mic): {e}")))?;

    let mix_format = audio_client
        .get_mixformat()
        .map_err(|e| CaptureError::Wasapi(format!("get_mixformat (mic): {e}")))?;

    let sample_rate_hz = mix_format.get_samplespersec();
    let channels = mix_format.get_nchannels() as usize;
    let bytes_per_frame = mix_format.get_blockalign() as usize;

    debug!(
        device = device_name,
        rate = sample_rate_hz,
        channels = channels,
        "Microphone mix format"
    );

    let buffer_duration_hns: i64 = 100_000;

    audio_client
        .initialize_client(
            &mix_format,
            buffer_duration_hns,
            &Direction::Capture,
            &ShareMode::Shared,
            false,
        )
        .map_err(|e| CaptureError::Wasapi(format!("initialize_client (mic): {e}")))?;

    let event_handle = audio_client
        .set_get_eventhandle()
        .map_err(|e| CaptureError::Wasapi(format!("set_get_eventhandle (mic): {e}")))?;

    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|e| CaptureError::Wasapi(format!("get_audiocaptureclient (mic): {e}")))?;

    audio_client
        .start_stream()
        .map_err(|e| CaptureError::Wasapi(format!("start_stream (mic): {e}")))?;

    info!(device = device_name, "Microphone capture started");

    let max_buffer_frames = audio_client
        .get_bufferframecount()
        .map_err(|e| CaptureError::Wasapi(format!("get_bufferframecount (mic): {e}")))?
        as usize;

    let max_bytes = max_buffer_frames * bytes_per_frame;
    let mut raw_buf: Vec<u8> = vec![0u8; max_bytes];

    let mut sample_buffer: Vec<f32> = Vec::with_capacity(target_chunk_frames * 2);

    loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        match event_handle.wait_for_event(200) {
            Ok(()) => {}
            Err(_) => continue,
        }

        loop {
            let available_frames = match capture_client.get_next_nbr_frames() {
                Ok(Some(n)) => n as usize,
                Ok(None) => 0,
                Err(e) => {
                    let msg = format!("{e}");
                    if msg.contains("INVALIDATED") || msg.contains("88890004") {
                        return Err(CaptureError::DeviceDisconnected(
                            device_name.to_string(),
                        ));
                    }
                    return Err(CaptureError::Wasapi(msg));
                }
            };

            if available_frames == 0 {
                break;
            }

            let needed_bytes = available_frames * bytes_per_frame;
            if raw_buf.len() < needed_bytes {
                raw_buf.resize(needed_bytes, 0u8);
            }

            match capture_client.read_from_device(bytes_per_frame, &mut raw_buf) {
                Ok((frames_read, _flags)) if frames_read > 0 => {
                    let byte_count = frames_read as usize * bytes_per_frame;
                    let samples =
                        bytes_to_f32_mono(&raw_buf[..byte_count], channels, bytes_per_frame);
                    sample_buffer.extend_from_slice(&samples);
                }
                Ok(_) => break,
                Err(e) => {
                    warn!("mic read_from_device: {e}");
                    break;
                }
            }
        }

        while sample_buffer.len() >= target_chunk_frames {
            let chunk_samples: Vec<f32> =
                sample_buffer.drain(..target_chunk_frames).collect();

            let chunk = AudioChunk::new(
                chunk_samples,
                sample_rate_hz,
                unix_ms_now(),
                AudioSource::Microphone,
            );

            if tx.blocking_send(chunk).is_err() {
                info!("Microphone channel closed; stopping mic thread.");
                let _ = audio_client.stop_stream();
                return Ok(());
            }
        }
    }

    audio_client
        .stop_stream()
        .map_err(|e| CaptureError::Wasapi(format!("stop_stream (mic): {e}")))?;

    info!(device = device_name, "Microphone capture stopped cleanly");
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Find a WASAPI device by endpoint ID string by enumerating the collection
/// for the given direction. wasapi 0.14 does not expose a direct
/// `get_device_by_id` function, so we enumerate and match.
fn find_device_by_id_in_collection(
    target_id: &str,
    direction: &Direction,
) -> Result<wasapi::Device, CaptureError> {
    let collection = wasapi::DeviceCollection::new(direction)
        .map_err(|e| CaptureError::Wasapi(format!("DeviceCollection::new: {e}")))?;

    let count = collection
        .get_nbr_devices()
        .map_err(|e| CaptureError::Wasapi(format!("get_nbr_devices: {e}")))?;

    for i in 0..count {
        let device = collection
            .get_device_at_index(i)
            .map_err(|e| CaptureError::Wasapi(format!("get_device_at_index({i}): {e}")))?;

        let id = device.get_id().unwrap_or_default();
        if id == target_id {
            return Ok(device);
        }
    }

    Err(CaptureError::DeviceNotFound(target_id.to_string()))
}

/// Convert a raw byte buffer of interleaved f32 samples into a mono f32 Vec
/// by down-mixing all channels to mono (arithmetic mean).
///
/// `bytes_per_frame` = channels × sizeof(f32).  The mix format used by WASAPI
/// shared mode is typically 32-bit IEEE float; if the device uses a different
/// format the bytes may not decode as valid f32, but that is then surfaced as
/// bad audio data rather than a panic.
pub(crate) fn bytes_to_f32_mono(
    raw: &[u8],
    channels: usize,
    bytes_per_frame: usize,
) -> Vec<f32> {
    if channels == 0 || bytes_per_frame == 0 {
        return Vec::new();
    }

    // Guard: total_frames is bounded by both slice length and frame size.
    let total_frames = raw.len() / bytes_per_frame;
    let bytes_per_channel = bytes_per_frame / channels;

    // We only handle f32 (4-byte) samples.  If bytes_per_channel != 4,
    // return silence so we don't produce garbage — the error will be visible
    // in the audio output but won't crash.
    if bytes_per_channel != std::mem::size_of::<f32>() {
        warn!(
            bytes_per_channel,
            "Unexpected bytes per channel; expected 4 (f32). Returning silence."
        );
        return vec![0.0f32; total_frames];
    }

    let mut mono = Vec::with_capacity(total_frames);

    for frame_idx in 0..total_frames {
        let frame_start = frame_idx * bytes_per_frame;
        let mut sum = 0.0f32;

        for ch in 0..channels {
            let byte_offset = frame_start + ch * bytes_per_channel;
            // Safety: bounds checked above.
            let bytes: [u8; 4] = raw[byte_offset..byte_offset + 4]
                .try_into()
                .unwrap_or([0u8; 4]);
            sum += f32::from_le_bytes(bytes);
        }

        mono.push(sum / channels as f32);
    }

    mono
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod capture_tests {
    use super::bytes_to_f32_mono;

    #[test]
    fn downmix_stereo_to_mono() {
        // Stereo: left = 1.0, right = -1.0  →  mono = 0.0
        let left: f32 = 1.0;
        let right: f32 = -1.0;
        let mut raw = Vec::new();
        raw.extend_from_slice(&left.to_le_bytes());
        raw.extend_from_slice(&right.to_le_bytes());

        // bytes_per_frame = 2 channels × 4 bytes = 8
        let mono = bytes_to_f32_mono(&raw, 2, 8);
        assert_eq!(mono.len(), 1);
        assert!(
            (mono[0] - 0.0).abs() < 1e-6,
            "stereo down-mix should average to 0.0"
        );
    }

    #[test]
    fn downmix_mono_passthrough() {
        let sample: f32 = 0.5;
        let raw = sample.to_le_bytes().to_vec();
        // bytes_per_frame = 1 channel × 4 bytes = 4
        let mono = bytes_to_f32_mono(&raw, 1, 4);
        assert_eq!(mono.len(), 1);
        assert!((mono[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn empty_buffer_produces_no_samples() {
        let mono = bytes_to_f32_mono(&[], 2, 8);
        assert!(mono.is_empty());
    }

    #[test]
    fn downmix_five_one_surround() {
        // Six channels, all 0.6 → mean is 0.6
        let sample: f32 = 0.6;
        let bytes = sample.to_le_bytes();
        let mut raw = Vec::new();
        for _ in 0..6 {
            raw.extend_from_slice(&bytes);
        }
        // bytes_per_frame = 6 × 4 = 24
        let mono = bytes_to_f32_mono(&raw, 6, 24);
        assert_eq!(mono.len(), 1);
        assert!((mono[0] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn multiple_frames_decoded_correctly() {
        // Two stereo frames: [0.4, 0.8] and [1.0, 0.0]
        // Expected mono: [0.6, 0.5]
        let f1_l: f32 = 0.4;
        let f1_r: f32 = 0.8;
        let f2_l: f32 = 1.0;
        let f2_r: f32 = 0.0;

        let mut raw = Vec::new();
        raw.extend_from_slice(&f1_l.to_le_bytes());
        raw.extend_from_slice(&f1_r.to_le_bytes());
        raw.extend_from_slice(&f2_l.to_le_bytes());
        raw.extend_from_slice(&f2_r.to_le_bytes());

        let mono = bytes_to_f32_mono(&raw, 2, 8);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.6).abs() < 1e-6, "frame 0: got {}", mono[0]);
        assert!((mono[1] - 0.5).abs() < 1e-6, "frame 1: got {}", mono[1]);
    }
}
