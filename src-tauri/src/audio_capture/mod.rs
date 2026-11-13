//! Audio capture module — owned by Member 3.
//!
//! Branch: feature/wasapi-audio-capture
//! Directory: src-tauri/src/audio_capture/
//!
//! This file is a coordination stub maintained by Member 2.
//! Member 3 owns all implementation inside this directory.
//!
//! Integration interface expected by Member 2:
//!   - `AudioCaptureHandle::start(config) -> AppResult<Self>`
//!   - `AudioCaptureHandle::pause() -> AppResult<()>`
//!   - `AudioCaptureHandle::resume() -> AppResult<()>`
//!   - `AudioCaptureHandle::stop() -> AppResult<()>`
//!   - Audio frames delivered via a tokio channel (see AudioFrame below).

use crate::core::error::AppResult;
use crate::core::types::AudioSource;

/// A single captured audio frame — raw PCM f32 samples.
#[derive(Debug, Clone)]
pub struct AudioFrame {
    /// Interleaved PCM f32 samples.
    pub samples: Vec<f32>,
    /// Number of audio channels.
    pub channels: u16,
    /// Sample rate in Hz (e.g. 44100, 48000).
    pub sample_rate: u32,
}

/// Opaque handle to the audio capture subsystem.
/// Replace this stub with the real WASAPI implementation.
pub struct AudioCaptureHandle;

impl AudioCaptureHandle {
    /// Start audio capture for the given source.
    pub async fn start(_source: AudioSource) -> AppResult<Self> {
        tracing::info!("audio_capture::start — stub, not yet implemented");
        Ok(AudioCaptureHandle)
    }

    /// Pause capture without releasing the device.
    pub async fn pause(&self) -> AppResult<()> {
        tracing::info!("audio_capture::pause — stub");
        Ok(())
    }

    /// Resume a paused capture.
    pub async fn resume(&self) -> AppResult<()> {
        tracing::info!("audio_capture::resume — stub");
        Ok(())
    }

    /// Stop capture and release all device resources.
    pub async fn stop(self) -> AppResult<()> {
        tracing::info!("audio_capture::stop — stub");
        Ok(())
    }
}
