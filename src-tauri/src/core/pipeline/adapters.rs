//! Pure conversions for the pipeline. No runtime or retained transcript state.

use crate::audio_capture::types::AudioSource as CaptureSource;
use crate::audio_processing::capture_adapter::samples_to_ms;
use crate::core::error::{AppError, AppResult};
use crate::core::types::AudioSource;
use crate::speaker_analysis::SpeakerInterval;
use crate::speech_recognition::RecognitionResult;
use crate::transcript_processing::types::{
    RecognizedChunk, SpeakerInterval as TranscriptSpeakerInterval,
};

/// Select a single input; this does not start capture or claim microphone-only
/// capture is supported by the current capture manager.
pub fn single_capture_source(source: &AudioSource) -> AppResult<CaptureSource> {
    match source {
        AudioSource::Loopback => Ok(CaptureSource::Loopback),
        AudioSource::Microphone => Ok(CaptureSource::Microphone),
        AudioSource::Both => Err(AppError::InvalidConfig(
            "Both audio sources are unsupported until a mixing policy is implemented".into(),
        )),
    }
}

/// Bound one inference window on the same active-audio sample clock as VAD.
/// Use the global end sample, rather than summing rounded chunk durations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecognitionWindow {
    start_ms: u64,
    end_ms: u64,
}

impl RecognitionWindow {
    pub fn from_samples(start_sample: u64, sample_count: u64, sample_rate: u32) -> AppResult<Self> {
        let end_sample = start_sample
            .checked_add(sample_count)
            .ok_or_else(|| AppError::InvalidConfig("Recognition sample offset overflow".into()))?;
        Ok(Self {
            start_ms: samples_to_ms(start_sample, sample_rate)?,
            end_ms: samples_to_ms(end_sample, sample_rate)?,
        })
    }
}

pub fn recognition_to_chunk(
    result: RecognitionResult,
    window: RecognitionWindow,
) -> AppResult<RecognizedChunk> {
    if result.end_ms < result.start_ms {
        return Err(AppError::InvalidConfig(
            "Recognition interval is reversed".into(),
        ));
    }
    let start_ms = window.start_ms.checked_add(result.start_ms);
    let end_ms = window.start_ms.checked_add(result.end_ms);
    let (Some(start_ms), Some(end_ms)) = (start_ms, end_ms) else {
        return Err(AppError::InvalidConfig(
            "Recognition timestamp overflow".into(),
        ));
    };
    if end_ms > window.end_ms {
        return Err(AppError::InvalidConfig(
            "Recognition interval exceeds its audio window".into(),
        ));
    }
    Ok(RecognizedChunk {
        start_ms,
        end_ms,
        text: result.text,
        language: if result.language.trim().is_empty() {
            None
        } else {
            Some(result.language)
        },
        is_final: !result.is_partial,
    })
}

pub fn speaker_to_interval(
    interval: SpeakerInterval,
    active_end_ms: u64,
) -> AppResult<TranscriptSpeakerInterval> {
    if interval.end_ms < interval.start_ms {
        return Err(AppError::InvalidConfig(
            "Speaker interval is reversed".into(),
        ));
    }
    if interval.end_ms > active_end_ms {
        return Err(AppError::InvalidConfig(
            "Speaker interval exceeds active audio".into(),
        ));
    }
    Ok(TranscriptSpeakerInterval {
        start_ms: interval.start_ms,
        end_ms: interval.end_ms,
        speaker: interval.speaker,
    })
}

#[cfg(test)]
mod tests {
    include!("adapters/tests.rs");
}
