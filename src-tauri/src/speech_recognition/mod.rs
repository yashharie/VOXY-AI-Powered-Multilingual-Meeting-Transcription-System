//! Speech recognition module — owned by Member 5.
//!
//! Integrates [whisper.cpp](https://github.com/ggerganov/whisper.cpp) via the
//! [`whisper-rs`](https://crates.io/crates/whisper-rs) crate for local,
//! on-device multilingual speech recognition.
//!
//! Supported languages: Sinhala (`si`), Tamil (`ta`), English (`en`),
//! and auto-detection.
//!
//! Integration interface expected by the pipeline (`core/pipeline.rs`):
//!   - `WhisperEngine::new(model_path) -> AppResult<Self>`
//!   - `WhisperEngine::transcribe(audio: &ProcessedAudio, lang: &str) -> AppResult<Vec<RecognitionResult>>`

pub mod config;

use crate::audio_processing::ProcessedAudio;
use crate::core::error::AppResult;

// Re-export key configuration types.
pub use config::{WhisperLanguage, WhisperModelConfig};

/// A recognized text span with timing metadata.
#[derive(Debug, Clone)]
pub struct RecognitionResult {
    pub text: String,
    pub language: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub is_partial: bool,
}

/// Handle to the Whisper inference engine.
pub struct WhisperEngine;

impl WhisperEngine {
    /// Load the whisper.cpp model at the given path.
    pub fn new(_model_path: &std::path::Path) -> AppResult<Self> {
        tracing::info!("speech_recognition::WhisperEngine::new — stub");
        Ok(WhisperEngine)
    }

    /// Transcribe processed audio. Returns one or more recognition results.
    pub fn transcribe(
        &self,
        _audio: &ProcessedAudio,
        _language_hint: &str,
    ) -> AppResult<Vec<RecognitionResult>> {
        tracing::debug!("speech_recognition::transcribe — stub");
        Ok(Vec::new())
    }
}
