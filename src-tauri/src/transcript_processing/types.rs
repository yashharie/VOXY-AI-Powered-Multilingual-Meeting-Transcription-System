//! Agreed transcript data structures (Member 7).
//! All JSON sent to the frontend uses camelCase.

use serde::{Deserialize, Serialize};

/// Whether a segment can still change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentStatus {
    /// Provisional text, may still be replaced by a newer update.
    Partial,
    /// Finished text, will not change any more.
    Final,
}

/// One piece of transcript shown in the live transcript panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    /// Unique id inside the session. The frontend uses it to update a segment in place.
    pub id: u64,
    /// Start time from session start, in milliseconds.
    pub start_ms: u64,
    /// End time from session start, in milliseconds.
    pub end_ms: u64,
    /// Generic label such as "Speaker 1". None when diarization has no answer yet.
    pub speaker: Option<String>,
    /// Original-language text (Sinhala, Tamil, English or mixed). Never translated.
    pub text: String,
    /// Language code from Whisper when available, e.g. "si", "ta", "en".
    pub language: Option<String>,
    pub status: SegmentStatus,
}

/// INPUT from the speech recognition module (Member 5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognizedChunk {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub language: Option<String>,
    /// false = provisional result, true = finished result.
    pub is_final: bool,
}

/// INPUT from the diarization module (Member 6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerInterval {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: String,
}

/// Anything the processing task can receive.
#[derive(Debug, Clone)]
pub enum TranscriptInput {
    Recognized(RecognizedChunk),
    Speakers(Vec<SpeakerInterval>),
}

/// OUTPUT: payload of the Tauri event. Contains only the segments that changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptUpdate {
    pub session_id: String,
    pub segments: Vec<TranscriptSegment>,
}

/// OUTPUT for the review module (Member 8): the full ordered transcript of a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTranscript {
    pub session_id: String,
    pub segments: Vec<TranscriptSegment>,
}
