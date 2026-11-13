//! VAD and speaker diarization module — owned by Member 6.
//!
//! Branch: feature/vad-diarization
//! Directory: src-tauri/src/speaker_analysis/
//!
//! Stub maintained by Member 2 for compilation. Member 6 owns all
//! implementation inside this directory.
//!
//! Integration interface expected by Member 2:
//!   - `VadEngine::is_speech(audio: &ProcessedAudio) -> AppResult<bool>`
//!   - `DiarizationEngine::assign_speaker(segment_start_ms, segment_end_ms) -> Option<String>`

use crate::audio_processing::ProcessedAudio;
use crate::core::error::AppResult;

pub struct VadEngine;

impl VadEngine {
    pub fn new() -> Self {
        VadEngine
    }

    /// Returns true when speech activity is detected in the audio chunk.
    pub fn is_speech(&self, _audio: &ProcessedAudio) -> AppResult<bool> {
        tracing::debug!("speaker_analysis::VadEngine::is_speech — stub");
        Ok(true) // stub: always speech
    }
}

impl Default for VadEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DiarizationEngine;

impl DiarizationEngine {
    pub fn new() -> Self {
        DiarizationEngine
    }

    /// Returns a generic speaker label for the given time range, if available.
    pub fn assign_speaker(
        &self,
        _start_ms: u64,
        _end_ms: u64,
    ) -> Option<String> {
        None // stub: diarization not yet implemented
    }
}

impl Default for DiarizationEngine {
    fn default() -> Self {
        Self::new()
    }
}
