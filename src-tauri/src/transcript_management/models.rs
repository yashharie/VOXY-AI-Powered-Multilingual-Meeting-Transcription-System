//! Transcript management models — re-exports the shared segment type.
pub use crate::core::types::TranscriptSegment;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranscriptError {
    #[error("Transcript segment not found: {0}")]
    NotFound(u64),
    #[error("Transcript segment {0} ends before it starts")]
    InvalidTimestamp(u64),
}

pub fn validate_segment(segment: &TranscriptSegment) -> Result<(), TranscriptError> {
    if segment.end_time < segment.start_time {
        return Err(TranscriptError::InvalidTimestamp(segment.index.into()));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptDeletion {
    pub session_id: String,
    pub segment_index: u64,
}
