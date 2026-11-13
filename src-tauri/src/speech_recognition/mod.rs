//! Speech recognition module — owned by Member 5.
//!
//! Branch: feature/whisper-speech-recognition
//! Directory: src-tauri/src/speech_recognition/
//!
//! Stub maintained by Member 2 for compilation. Member 5 owns all
//! implementation inside this directory.
//!
//! Integration interface expected by Member 2:
//!   - `WhisperEngine::new(model_path) -> AppResult<Self>`
//!   - `WhisperEngine::transcribe(audio: &ProcessedAudio, lang: &str) -> AppResult<RecognitionResult>`

use crate::audio_processing::ProcessedAudio;
use crate::core::error::AppResult;

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
