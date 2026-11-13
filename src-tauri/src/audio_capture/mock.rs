//! Mock audio source for integration testing without real hardware.
//!
//! [`MockAudioSource`] generates synthetic sine-wave PCM chunks at a
//! configurable frequency and sample rate. It is structurally identical to the
//! real WASAPI source from the caller's perspective: it returns the same
//! [`mpsc::Receiver<AudioChunk>`] interface.
//!
//! Use this in unit/integration tests or when running on CI where no audio
//! device is available.

use std::{
    f32::consts::TAU,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use tokio::sync::mpsc;
use tracing::info;

use crate::audio_capture::{
    error::CaptureError,
    types::{unix_ms_now, AudioChunk, AudioSource},
};

/// Configuration for the mock audio source.
#[derive(Debug, Clone)]
pub struct MockConfig {
    /// Sample rate of the generated audio (Hz).
    pub sample_rate_hz: u32,
    /// Tone frequency to generate (Hz).
    pub tone_hz: f32,
    /// Amplitude of the generated tone (0.0–1.0).
    pub amplitude: f32,
    /// Number of samples per delivered chunk.
    pub chunk_frames: usize,
    /// Channel buffer capacity.
    pub channel_capacity: usize,
    /// Source tag attached to each chunk.
    pub source: AudioSource,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 48_000,
            tone_hz: 440.0,
            amplitude: 0.2,
            chunk_frames: 4800,
            channel_capacity: 64,
            source: AudioSource::Loopback,
        }
    }
}

/// A stop handle for the mock generator thread.
pub struct MockHandle {
    stop_flag: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockHandle {
    /// Stop the mock generator.
    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
        info!("Mock audio source stopped.");
    }
}

impl Drop for MockHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

/// Synthetic audio source that produces sine-wave PCM chunks.
pub struct MockAudioSource;

impl MockAudioSource {
    /// Start the mock generator. Returns a handle and a chunk receiver,
    /// mirroring the real [`AudioCaptureManager::start`] signature.
    pub fn start(
        config: MockConfig,
    ) -> Result<(MockHandle, mpsc::Receiver<AudioChunk>), CaptureError> {
        let (tx, rx) = mpsc::channel::<AudioChunk>(config.channel_capacity);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let sf = stop_flag.clone();

        let handle = thread::Builder::new()
            .name("voxy-mock-audio".into())
            .spawn(move || {
                let mut phase: f32 = 0.0;
                let phase_increment =
                    TAU * config.tone_hz / config.sample_rate_hz as f32;

                // Sleep duration per chunk to simulate real-time delivery.
                let chunk_duration_ms =
                    (config.chunk_frames as f64 / config.sample_rate_hz as f64)
                        * 1000.0;
                let sleep_duration =
                    Duration::from_millis(chunk_duration_ms as u64);

                info!(
                    "Mock audio source started: {}Hz tone at {}Hz sample rate",
                    config.tone_hz, config.sample_rate_hz
                );

                loop {
                    if sf.load(Ordering::Relaxed) {
                        break;
                    }

                    let samples: Vec<f32> = (0..config.chunk_frames)
                        .map(|_| {
                            let s = config.amplitude * phase.sin();
                            phase = (phase + phase_increment) % TAU;
                            s
                        })
                        .collect();

                    let chunk = AudioChunk::new(
                        samples,
                        config.sample_rate_hz,
                        unix_ms_now(),
                        config.source,
                    );

                    if tx.blocking_send(chunk).is_err() {
                        break;
                    }

                    thread::sleep(sleep_duration);
                }
            })
            .map_err(|e| CaptureError::TaskError(e.to_string()))?;

        Ok((MockHandle { stop_flag, thread: Some(handle) }, rx))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod mock_tests {
    use super::*;

    #[tokio::test]
    async fn mock_delivers_chunks_with_correct_shape() {
        let config = MockConfig {
            sample_rate_hz: 16_000,
            chunk_frames: 1600,
            channel_capacity: 8,
            ..MockConfig::default()
        };

        let (handle, mut rx) = MockAudioSource::start(config.clone()).unwrap();

        let chunk = rx.recv().await.expect("Should receive a chunk");

        assert_eq!(chunk.sample_rate_hz, 16_000);
        assert_eq!(chunk.frame_count, 1600);
        assert_eq!(chunk.samples.len(), 1600);
        assert_eq!(chunk.source, AudioSource::Loopback);

        handle.stop();
    }

    #[tokio::test]
    async fn mock_stops_cleanly() {
        let (handle, mut rx) = MockAudioSource::start(MockConfig::default()).unwrap();
        // Receive at least one chunk to confirm it's running.
        rx.recv().await.expect("Should receive a chunk");
        // Drop handle → generator exits → channel closes.
        drop(handle);
        // The receiver must eventually report the channel is closed.
        // Drain remaining buffered chunks.
        while rx.try_recv().is_ok() {}
        // Now the channel should be empty and the sender gone.
        // We cannot assert recv() returns None without waiting, so just confirm
        // no panic occurred on drop.
    }

    #[tokio::test]
    async fn mock_chunk_duration_matches_expected() {
        let config = MockConfig {
            sample_rate_hz: 48_000,
            chunk_frames: 4800,
            ..MockConfig::default()
        };
        let (handle, mut rx) = MockAudioSource::start(config).unwrap();
        let chunk = rx.recv().await.unwrap();
        // 4800 / 48000 * 1000 = 100 ms
        let dur = chunk.duration_ms();
        assert!((dur - 100.0).abs() < 0.1, "Expected ~100 ms, got {dur}");
        handle.stop();
    }
}
