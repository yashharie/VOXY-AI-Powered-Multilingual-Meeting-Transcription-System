//! Review/export use Member 7's model directly; there is no second segment type.
pub use crate::transcript_processing::types::{SegmentStatus, TranscriptSegment};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranscriptError {
    #[error("Transcript segment ID must be a positive integer")]
    InvalidId,
    #[error("Transcript segment not found: {0}")]
    NotFound(u64),
    #[error("Transcript segment {0} ends before it starts")]
    InvalidTimestamp(u64),
}
pub fn validate_segment(segment: &TranscriptSegment) -> Result<(), TranscriptError> {
    if segment.id == 0 {
        return Err(TranscriptError::InvalidId);
    }
    if segment.end_ms < segment.start_ms {
        return Err(TranscriptError::InvalidTimestamp(segment.id));
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptDeletion {
    pub session_id: String,
    pub segment_id: u64,
}
