//! Transcript processing module — owned by Member 7.
//!
//! Branch: feature/transcript-processing
//! Directory: src-tauri/src/transcript_processing/
//!
//! Stub maintained by Member 2 for compilation. Member 7 owns all
//! implementation inside this directory.
//!
//! Integration interface expected by Member 2:
//!   - `TranscriptProcessor::build_segment(...) -> TranscriptSegment`
//!   - Responsible for timestamp association, speaker label merging, and
//!     deduplication before the segment is pushed to the session and emitted.

use chrono::Utc;

use crate::core::types::TranscriptSegment;
use crate::speech_recognition::RecognitionResult;

pub struct TranscriptProcessor;

impl TranscriptProcessor {
    pub fn new() -> Self {
        TranscriptProcessor
    }

    /// Build a `TranscriptSegment` from a speech recognition result.
    pub fn build_segment(
        &self,
        result: RecognitionResult,
        speaker: Option<String>,
    ) -> TranscriptSegment {
        TranscriptSegment {
            index: 0, // assigned by Session::push_segment
            text: result.text,
            start_time: result.start_ms as f64 / 1000.0,
            end_time: result.end_ms as f64 / 1000.0,
            language: result.language,
            speaker,
            is_partial: result.is_partial,
            timestamp: Utc::now(),
        }
    }
}

impl Default for TranscriptProcessor {
    fn default() -> Self {
        Self::new()
    }
}
