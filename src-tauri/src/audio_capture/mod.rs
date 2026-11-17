//! Audio capture module — owned by Member 3.
//!
//! Exposes the real WASAPI capture implementation from capture.rs.

pub mod capture;
pub mod config;
pub mod device;
pub mod error;
pub mod mock;
pub mod types;

#[cfg(test)]
mod tests;

pub use capture::{AudioCaptureManager, CaptureHandle};
pub use config::{CaptureConfig, DeviceSelection};
pub use error::CaptureError;
pub use types::{AudioChunk, AudioSource};

use crate::core::error::AppResult;
use crate::core::types::AudioSource as CoreAudioSource;

/// A single captured audio frame passed to the pipeline.
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

/// Pipeline-facing handle that wraps the real CaptureHandle.
#[derive(Debug)]
pub struct AudioCaptureHandle {
    inner: Option<CaptureHandle>,
}

impl AudioCaptureHandle {
    /// Start real WASAPI loopback capture and return the handle + audio receiver.
    pub fn start_real(
        config: CaptureConfig,
    ) -> Result<(Self, tokio::sync::mpsc::Receiver<AudioChunk>), CaptureError> {
        let (handle, rx) = AudioCaptureManager::start(config)?;
        Ok((Self { inner: Some(handle) }, rx))
    }

    /// Stub start — used as fallback if capture fails.
    pub async fn start(_source: CoreAudioSource) -> AppResult<Self> {
        tracing::info!("audio_capture::start — stub fallback");
        Ok(Self { inner: None })
    }

    pub fn pause(&self) -> AppResult<()> {
        if let Some(ref handle) = self.inner {
            handle.pause();
        }
        Ok(())
    }

    pub fn resume(&self) -> AppResult<()> {
        if let Some(ref handle) = self.inner {
            handle.resume();
        }
        Ok(())
    }

    pub async fn stop(self) -> AppResult<()> {
        if let Some(handle) = self.inner {
            handle.stop();
        }
        Ok(())
    }
}
