use crate::audio_processing::audio_format::AudioFrame;
use crate::audio_processing::validator::{validate_audio_frame, AudioValidationError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioNormalizer {
    pub peak_limit: f32,
}

impl Default for AudioNormalizer {
    fn default() -> Self {
        Self { peak_limit: 0.98 }
    }
}

impl AudioNormalizer {
    pub fn new(peak_limit: f32) -> Self {
        Self {
            peak_limit: peak_limit.clamp(0.05, 0.99),
        }
    }

    pub fn normalize(&self, samples: &[f32]) -> Vec<f32> {
        if samples.is_empty() {
            return Vec::new();
        }

        let peak = samples
            .iter()
            .fold(0.0f32, |acc, sample| acc.max(sample.abs()));
        if peak <= 0.0001 {
            return samples.to_vec();
        }

        let scale = (self.peak_limit / peak).min(1.0);
        samples
            .iter()
            .map(|sample| (sample * scale).clamp(-self.peak_limit, self.peak_limit))
            .collect()
    }
}

pub fn normalize_pcm(samples: &[f32], peak_limit: f32) -> Result<Vec<f32>, AudioValidationError> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err(AudioValidationError::InvalidSample);
    }
    Ok(AudioNormalizer::new(peak_limit).normalize(samples))
}

pub fn normalize_frame(frame: &AudioFrame, peak_limit: f32) -> AudioFrame {
    if let Err(_) = validate_audio_frame(frame) {
        return frame.clone();
    }
    let normalized =
        normalize_pcm(&frame.samples, peak_limit).unwrap_or_else(|_| frame.samples.clone());
    AudioFrame::new(
        frame.sample_rate,
        frame.channels,
        normalized,
        frame.timestamp_ms,
        frame.frame_index,
        frame.is_last,
    )
}

pub fn normalize_audio(samples: &[f32], peak_limit: f32) -> Result<Vec<f32>, AudioValidationError> {
    normalize_pcm(samples, peak_limit)
}
