//! Boundary between mono capture chunks and the existing preprocessor.

use std::sync::Mutex;

use crate::audio_capture::{types::AudioSource, AudioChunk};
use crate::core::error::{AppError, AppResult};

use super::{AudioFrame, AudioPreprocessor, ProcessedAudio};

pub const OUTPUT_SAMPLE_RATE: u32 = 16_000;

/// Convert sample counts without per-chunk rounding drift or integer overflow.
pub fn samples_to_ms(samples: u64, sample_rate: u32) -> AppResult<u64> {
    if sample_rate == 0 || sample_rate > 384_000 {
        return Err(AppError::InvalidConfig("Invalid audio sample rate".into()));
    }
    u64::try_from(u128::from(samples) * 1000 / u128::from(sample_rate))
        .map_err(|_| AppError::InvalidConfig("Audio timestamp overflow".into()))
}

/// Capture timestamps are epoch-based. The caller supplies an active-audio
/// offset instead; paused chunks must not be submitted to this adapter.
pub fn capture_to_frame(chunk: AudioChunk, active_start_ms: u64) -> AppResult<AudioFrame> {
    samples_to_ms(0, chunk.sample_rate_hz)?;
    if chunk.frame_count != chunk.samples.len() {
        return Err(AppError::InvalidConfig(
            "Capture frame count does not match mono samples".into(),
        ));
    }
    if chunk.samples.iter().any(|sample| !sample.is_finite()) {
        return Err(AppError::InvalidConfig(
            "Capture contains non-finite PCM samples".into(),
        ));
    }
    Ok(AudioFrame::new(
        chunk.sample_rate_hz,
        1,
        chunk.samples,
        Some(active_start_ms),
        None,
        false,
    ))
}

struct CaptureState {
    preprocessor: AudioPreprocessor,
    source_and_rate: Option<(AudioSource, u32)>,
    output_samples: u64,
}

/// One processor per active audio stream. A source/rate change is rejected
/// before preprocessing, rather than reusing an incompatible resampler.
/// Only counters and the existing preprocessor are retained here.
pub struct AudioProcessor {
    state: Mutex<CaptureState>,
}

impl AudioProcessor {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(CaptureState {
                preprocessor: AudioPreprocessor::new(OUTPUT_SAMPLE_RATE, 1, 64_000),
                source_and_rate: None,
                output_samples: 0,
            }),
        }
    }

    pub fn process(&self, chunk: AudioChunk) -> AppResult<ProcessedAudio> {
        let source = chunk.source;
        let rate = chunk.sample_rate_hz;
        // Validate before taking or mutating processing state.
        let mut frame = capture_to_frame(chunk, 0)?;
        let mut state = self.state.lock().map_err(|error| {
            AppError::Internal(format!("audio processor lock poisoned: {error}"))
        })?;
        if let Some((expected_source, expected_rate)) = state.source_and_rate {
            if source != expected_source || rate != expected_rate {
                return Err(AppError::InvalidConfig(
                    "Capture source/sample rate changed; a new stream processor is required".into(),
                ));
            }
        }
        let start_ms = samples_to_ms(state.output_samples, OUTPUT_SAMPLE_RATE)?;
        frame.timestamp_ms = Some(start_ms);
        let mut processed = state
            .preprocessor
            .process_frame(&frame)
            .map_err(|error| AppError::InvalidConfig(format!("invalid captured PCM: {error:?}")))?;
        let output_len = u64::try_from(processed.samples.len())
            .map_err(|_| AppError::InvalidConfig("Audio sample count overflow".into()))?;
        let next_samples = state
            .output_samples
            .checked_add(output_len)
            .ok_or_else(|| AppError::InvalidConfig("Audio sample count overflow".into()))?;
        let end_ms = samples_to_ms(next_samples, OUTPUT_SAMPLE_RATE)?;
        processed.timestamp_ms = Some(start_ms);
        processed.timing.start_ms = Some(start_ms);
        processed.timing.end_ms = Some(end_ms);
        state.output_samples = next_samples;
        if !frame.samples.is_empty() {
            state.source_and_rate = Some((source, rate));
        }
        Ok(processed)
    }
}

impl Default for AudioProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
