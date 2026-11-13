//! Plain data types shared inside `speaker_analysis` and handed to other modules.

use serde::{Deserialize, Serialize};

/// Audio format expected from `audio_processing`: 16 kHz, mono, f32 PCM.
pub const SAMPLE_RATE: u32 = 16_000;

pub fn samples_to_ms(samples: u64) -> u64 {
    samples * 1000 / SAMPLE_RATE as u64
}

pub fn ms_to_samples(ms: u64) -> u64 {
    ms * SAMPLE_RATE as u64 / 1000
}

/// Generic label for the n-th distinct speaker (0-based id -> "Speaker 1").
pub fn speaker_label(id: u32) -> String {
    format!("Speaker {}", id + 1)
}

/// A speaker-attributed time range, session-relative milliseconds.
///
/// Same shape as `SpeakerInterval` in `transcript_processing` (Member 7), so the
/// pipeline can forward it as `TranscriptInput::Speakers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerInterval {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: String,
}

/// A detected speech region (VAD output), session-relative milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSegment {
    pub start_ms: u64,
    pub end_ms: u64,
}

/// A finished speech region together with its audio (internal, never serialised).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechChunk {
    /// Offset of the first sample from the start of the session stream.
    pub start_sample: u64,
    pub samples: Vec<f32>,
}

impl SpeechChunk {
    pub fn start_ms(&self) -> u64 {
        samples_to_ms(self.start_sample)
    }
    pub fn end_ms(&self) -> u64 {
        samples_to_ms(self.start_sample + self.samples.len() as u64)
    }
    pub fn as_segment(&self) -> SpeechSegment {
        SpeechSegment {
            start_ms: self.start_ms(),
            end_ms: self.end_ms(),
        }
    }
}
