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
pub mod engine;

// Re-export key types.
pub use config::{
    lang_full_name, lang_id_to_enum, validate_language_for_model,
    LanguageDetectionResult, MultilingualConfig, WhisperLanguage, WhisperModelConfig,
};
pub use engine::WhisperEngine;

use crate::audio_processing::ProcessedAudio;

/// A recognized text span with timing metadata.
#[derive(Debug, Clone)]
pub struct RecognitionResult {
    pub text: String,
    pub language: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub is_partial: bool,
}
