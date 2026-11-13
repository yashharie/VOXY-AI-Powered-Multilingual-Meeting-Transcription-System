//! Audio preprocessing module — owned by Member 4.
//!
//! Branch: feature/audio-preprocessing
//! Directory: src-tauri/src/audio_processing/
//!
//! Stub maintained by Member 2 for compilation. Member 4 owns all
//! implementation inside this directory.
//!
//! Integration interface expected by Member 2:
//!   - `AudioProcessor::process(frame: AudioFrame) -> AppResult<ProcessedAudio>`
//!   - Output: 16 kHz mono f32 PCM, normalised, chunked for VAD/Whisper.

use crate::audio_capture::AudioFrame;
use crate::core::error::AppResult;

/// Normalised, resampled audio ready for VAD and speech recognition.
#[derive(Debug, Clone)]
pub struct ProcessedAudio {
    /// Mono f32 PCM samples at 16 000 Hz.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Stateless audio preprocessor handle.
pub struct AudioProcessor;

impl AudioProcessor {
    pub fn new() -> Self {
        AudioProcessor
    }

    /// Process a raw captured frame into normalised 16 kHz mono PCM.
    pub fn process(&self, _frame: AudioFrame) -> AppResult<ProcessedAudio> {
        tracing::debug!("audio_processing::process — stub");
        Ok(ProcessedAudio {
            samples: Vec::new(),
            sample_rate: 16_000,
        })
    }
}

impl Default for AudioProcessor {
    fn default() -> Self {
        Self::new()
    }
}
