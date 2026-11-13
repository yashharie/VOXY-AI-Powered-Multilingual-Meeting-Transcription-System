use crate::audio_processing::audio_format::{AudioFrame, PcmFormat};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioValidationError {
    EmptyInput,
    InvalidSampleRate,
    InvalidChannelCount,
    InvalidSample,
    IncompleteFrame,
    InvalidFrameShape,
}

pub fn validate_pcm_format(format: &PcmFormat) -> Result<(), AudioValidationError> {
    if format.sample_rate == 0 || format.sample_rate > 384_000 {
        return Err(AudioValidationError::InvalidSampleRate);
    }
    if format.channels == 0 || format.channels > 8 {
        return Err(AudioValidationError::InvalidChannelCount);
    }
    if format.bits_per_sample == 0 || format.bits_per_sample > 64 {
        return Err(AudioValidationError::InvalidSampleRate);
    }
    Ok(())
}

pub fn validate_audio_frame(frame: &AudioFrame) -> Result<(), AudioValidationError> {
    if frame.sample_rate == 0 || frame.sample_rate > 384_000 {
        return Err(AudioValidationError::InvalidSampleRate);
    }
    if frame.channels == 0 || frame.channels > 8 {
        return Err(AudioValidationError::InvalidChannelCount);
    }
    if frame.samples.is_empty() {
        return Ok(());
    }
    if frame.samples.len() % frame.channels as usize != 0 {
        return Err(AudioValidationError::IncompleteFrame);
    }

    if frame.samples.iter().any(|sample| !sample.is_finite()) {
        return Err(AudioValidationError::InvalidSample);
    }

    Ok(())
}

pub fn validate_pcm_samples(samples: &[f32], channels: u16) -> Result<(), AudioValidationError> {
    if samples.is_empty() {
        return Ok(());
    }
    if channels == 0 || channels > 8 {
        return Err(AudioValidationError::InvalidChannelCount);
    }
    if samples.len() % channels as usize != 0 {
        return Err(AudioValidationError::IncompleteFrame);
    }
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err(AudioValidationError::InvalidSample);
    }
    Ok(())
}
