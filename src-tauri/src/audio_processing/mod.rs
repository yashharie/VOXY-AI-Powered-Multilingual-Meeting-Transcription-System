//! Audio preprocessing for captured PCM, resampling, normalization, and buffering.

pub mod audio_format;
pub mod buffer;
pub mod chunker;
pub mod normalizer;
pub mod resampler;
pub mod validator;

pub use audio_format::{AudioChunk, AudioFrame, AudioTiming, PcmFormat, ProcessedAudio};
pub use buffer::{AudioBuffer, AudioBufferError};
pub use chunker::{chunk_audio, chunk_samples, Chunker};
pub use normalizer::{normalize_audio, normalize_frame, normalize_pcm, AudioNormalizer};
pub use resampler::{
    resample_audio, resample_frame, resample_pcm, ResampleConfig, Resampler, ResamplingError,
};
pub use validator::{validate_audio_frame, validate_pcm_format, AudioValidationError};

use std::cmp;
use std::sync::Mutex;

use crate::audio_capture::AudioFrame as CapturedAudioFrame;
use crate::core::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq)]
pub struct AudioInput {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
    pub timestamp_ms: Option<u64>,
    pub frame_index: Option<u64>,
    pub is_last: bool,
}

impl AudioInput {
    pub fn from_frame(frame: &AudioFrame) -> Self {
        Self {
            sample_rate: frame.sample_rate,
            channels: frame.channels,
            samples: frame.samples.clone(),
            timestamp_ms: frame.timestamp_ms,
            frame_index: frame.frame_index,
            is_last: frame.is_last,
        }
    }

    pub fn saturating_duration_ms(&self) -> u64 {
        if self.sample_rate == 0 || self.samples.is_empty() {
            return 0;
        }
        let total_samples = self.samples.len() as u64;
        let frame_samples = total_samples / self.channels as u64;
        frame_samples.saturating_mul(1000) / self.sample_rate as u64
    }
}

pub struct AudioPreprocessor {
    pub target_sample_rate: u32,
    pub target_channels: u16,
    pub max_buffer_samples: usize,
    pub input_sample_rate: Option<u32>,
    pub input_channels: Option<u16>,
    pub normalization_peak: f32,
    pub resampler: Option<Resampler>,
    pub buffer: AudioBuffer,
}

impl AudioPreprocessor {
    pub fn new(target_sample_rate: u32, target_channels: u16, max_buffer_samples: usize) -> Self {
        let target = cmp::max(target_channels, 1);
        Self {
            target_sample_rate: target_sample_rate.max(1),
            target_channels: target,
            max_buffer_samples,
            input_sample_rate: None,
            input_channels: None,
            normalization_peak: 0.98,
            resampler: None,
            buffer: AudioBuffer::new(max_buffer_samples, target, target_sample_rate),
        }
    }

    pub fn with_normalization_peak(mut self, peak: f32) -> Self {
        self.normalization_peak = peak.clamp(0.05, 0.99);
        self
    }

    pub fn process_frame(
        &mut self,
        frame: &AudioFrame,
    ) -> Result<ProcessedAudio, AudioValidationError> {
        if let Err(err) = validate_audio_frame(frame) {
            return Err(err);
        }

        if frame.samples.is_empty() {
            return Ok(ProcessedAudio::empty_for(
                self.target_sample_rate,
                self.target_channels,
            ));
        }

        self.input_sample_rate = Some(frame.sample_rate);
        self.input_channels = Some(frame.channels);

        let normalized = normalize_frame(frame, self.normalization_peak);
        let converted = convert_channels(&normalized, self.target_channels);
        let final_samples = if frame.sample_rate == self.target_sample_rate {
            converted
        } else {
            let resampler = match &mut self.resampler {
                Some(r) => r,
                None => {
                    let created = Resampler::new(
                        frame.sample_rate,
                        self.target_sample_rate,
                        self.target_channels as usize,
                    )
                    .map_err(|_| AudioValidationError::InvalidSampleRate)?;
                    self.resampler = Some(created);
                    self.resampler.as_mut().unwrap()
                }
            };
            resampler
                .process(&converted)
                .map_err(|_| AudioValidationError::InvalidSampleRate)?
        };

        let sample_count = final_samples.len();
        self.buffer
            .push_samples(
                final_samples.clone(),
                self.target_sample_rate,
                self.target_channels,
            )
            .map_err(|_| AudioValidationError::InvalidSampleRate)?;

        Ok(ProcessedAudio {
            format: PcmFormat::new(self.target_sample_rate, self.target_channels, 32),
            samples: final_samples,
            sample_rate: self.target_sample_rate,
            samples_per_channel: sample_count / self.target_channels as usize,
            timestamp_ms: frame.timestamp_ms,
            timing: AudioTiming {
                start_ms: frame.timestamp_ms,
                end_ms: frame.timestamp_ms,
                frame_index: frame.frame_index,
            },
        })
    }

    pub fn flush(&mut self) -> Vec<f32> {
        self.buffer.drain_all()
    }
}

/// Stateful adapter matching the application pipeline's capture-frame interface.
pub struct AudioProcessor {
    preprocessor: Mutex<AudioPreprocessor>,
}

impl AudioProcessor {
    pub fn new() -> Self {
        Self {
            preprocessor: Mutex::new(AudioPreprocessor::new(16_000, 1, 64_000)),
        }
    }

    pub fn process(&self, frame: CapturedAudioFrame) -> AppResult<ProcessedAudio> {
        let frame = AudioFrame::new(
            frame.sample_rate,
            frame.channels,
            frame.samples,
            None,
            None,
            false,
        );
        let mut preprocessor = self.preprocessor.lock().map_err(|error| {
            AppError::Internal(format!("audio preprocessor lock poisoned: {error}"))
        })?;

        preprocessor.process_frame(&frame).map_err(|error| {
            AppError::InvalidConfig(format!("invalid captured PCM frame: {error:?}"))
        })
    }
}

impl Default for AudioProcessor {
    fn default() -> Self {
        Self::new()
    }
}

fn convert_channels(frame: &AudioFrame, target_channels: u16) -> Vec<f32> {
    match frame.channels {
        1 => frame.samples.clone(),
        _ if target_channels == 1 && frame.channels > 1 => {
            let mut mono = Vec::with_capacity(frame.samples.len() / frame.channels as usize);
            for index in 0..frame.samples.len() / frame.channels as usize {
                let mut sum = 0.0f32;
                for channel in 0..frame.channels as usize {
                    let pos = index * frame.channels as usize + channel;
                    if let Some(sample) = frame.samples.get(pos) {
                        sum += *sample;
                    }
                }
                mono.push(sum / frame.channels as f32);
            }
            mono
        }
        _ if target_channels == frame.channels => frame.samples.clone(),
        _ => frame.samples.clone(),
    }
}
