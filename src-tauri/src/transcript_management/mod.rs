//! Transcript management module — owned by Member 8.
//!
//! Branch: feature/transcript-management
//! Directory: src-tauri/src/transcript_management/
//!
//! Stub maintained by Member 2 for compilation. Member 8 owns all
//! implementation inside this directory.
//!
//! Integration interface expected by Member 2:
//!   - `save_transcript(segments, format, path) -> AppResult<()>`
//!   - Coordinates with AppState cleanup on session stop.

use std::path::Path;

use crate::core::error::AppResult;
use crate::core::types::TranscriptSegment;

/// Export format options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportFormat {
    Txt,
    Json,
}

/// Save transcript segments to a file at the given path.
pub fn save_transcript(
    _segments: &[TranscriptSegment],
    _format: ExportFormat,
    _dest: &Path,
) -> AppResult<()> {
    tracing::info!("transcript_management::save_transcript — stub");
    Ok(())
}
