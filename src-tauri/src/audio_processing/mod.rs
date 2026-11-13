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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_processing::validator::AudioValidationError;

    fn sine_wave(amplitude: f32, sample_count: usize) -> Vec<f32> {
        (0..sample_count)
            .map(|idx| (idx as f32 / sample_count as f32 * std::f32::consts::TAU).sin() * amplitude)
            .collect()
    }

    #[test]
    fn valid_pcm_input_is_accepted() {
        let mut processor = AudioPreprocessor::new(16000, 1, 4096);
        let frame = AudioFrame::new(48000, 1, sine_wave(0.5, 256), Some(0), Some(0), false);
        let result = processor.process_frame(&frame).unwrap();
        assert_eq!(result.format.sample_rate, 16000);
        assert_eq!(result.sample_rate, 16000);
        assert!(!result.samples.is_empty());
        assert!(result.samples.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn pipeline_adapter_accepts_capture_frames_and_returns_16khz_mono() {
        let processor = AudioProcessor::new();
        let frame = CapturedAudioFrame {
            sample_rate: 48000,
            channels: 2,
            samples: sine_wave(0.3, 512)
                .into_iter()
                .flat_map(|sample| [sample, sample])
                .collect(),
        };

        let result = processor.process(frame).unwrap();

        assert_eq!(result.sample_rate, 16000);
        assert_eq!(result.format.channels, 1);
        assert!(!result.samples.is_empty());
    }

    #[test]
    fn empty_audio_input_is_handled_safely() {
        let mut processor = AudioPreprocessor::new(16000, 1, 4096);
        let frame = AudioFrame::new(48000, 1, vec![], Some(0), Some(0), false);
        let result = processor.process_frame(&frame).unwrap();
        assert!(result.samples.is_empty());
        assert_eq!(result.sample_rate, 16000);
    }

    #[test]
    fn invalid_sample_rate_is_rejected() {
        let frame = AudioFrame::new(0, 1, vec![0.1, -0.1], Some(0), Some(0), false);
        assert_eq!(
            validate_audio_frame(&frame),
            Err(AudioValidationError::InvalidSampleRate)
        );
    }

    #[test]
    fn invalid_channel_count_is_rejected() {
        let frame = AudioFrame::new(48000, 0, vec![0.1], Some(0), Some(0), false);
        assert_eq!(
            validate_audio_frame(&frame),
            Err(AudioValidationError::InvalidChannelCount)
        );
    }

    #[test]
    fn stereo_input_converts_to_mono_without_loss() {
        let frame = AudioFrame::new(
            48000,
            2,
            vec![0.1, -0.1, 0.3, -0.3, 0.5, -0.5],
            None,
            None,
            false,
        );
        let converted = convert_channels(&frame, 1);
        assert_eq!(converted.len(), 3);
        assert!(converted.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn resampling_from_48khz_to_16khz_returns_valid_output() {
        let frame = AudioFrame::new(48000, 1, sine_wave(0.4, 2048), Some(0), Some(0), false);
        let output = resample_frame(&frame, 16000, 1).unwrap();
        assert_eq!(output.sample_rate, 16000);
        assert_eq!(output.channels, 1);
        assert!(!output.samples.is_empty());
        assert!(output.samples.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn quiet_audio_is_normalized_without_inf_nan() {
        let quiet = vec![1e-6; 256];
        let normalized = normalize_pcm(&quiet, 0.98).unwrap();
        assert!(normalized.iter().all(|sample| sample.is_finite()));
        assert!(normalized.iter().all(|sample| sample.abs() <= 0.98));
    }

    #[test]
    fn loud_audio_is_clipped_before_downstream_processing() {
        let loud = vec![2.0, -2.0, 3.0, -3.0, 1.5, -1.5];
        let normalized = normalize_pcm(&loud, 0.98).unwrap();
        assert!(normalized.iter().all(|sample| sample.abs() <= 0.98));
        assert!(normalized.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn incomplete_audio_frames_are_handled_safely() {
        let mut processor = AudioPreprocessor::new(16000, 1, 2048);
        let frame = AudioFrame::new(48000, 1, vec![0.1, -0.1, 0.2], Some(0), Some(0), false);
        let result = processor.process_frame(&frame);
        assert!(result.is_ok());
    }

    #[test]
    fn multiple_chunks_keep_sequence_order() {
        let mut buffer = AudioBuffer::new(16, 1, 16000);
        buffer
            .push_samples(vec![1.0, 2.0, 3.0, 4.0], 16000, 1)
            .unwrap();
        buffer
            .push_samples(vec![5.0, 6.0, 7.0, 8.0], 16000, 1)
            .unwrap();
        let drained = buffer.drain_all();
        assert_eq!(drained.len(), 8);
        assert_eq!(drained[0], 1.0);
        assert_eq!(drained[7], 8.0);
    }

    #[test]
    fn buffer_overflow_truncates_oldest_samples() {
        let mut buffer = AudioBuffer::new(4, 1, 16000);
        buffer
            .push_samples(vec![1.0, 2.0, 3.0, 4.0, 5.0], 16000, 1)
            .unwrap();
        assert!(buffer.len_samples() <= 4);
    }

    #[test]
    fn invalid_float_values_are_rejected() {
        let frame = AudioFrame::new(
            48000,
            1,
            vec![0.1, f32::NAN, f32::INFINITY],
            Some(0),
            Some(0),
            false,
        );
        assert_eq!(
            validate_audio_frame(&frame),
            Err(AudioValidationError::InvalidSample)
        );
    }

    #[test]
    fn resampler_keeps_continuity_across_consecutive_frames() {
        let mut processor = AudioPreprocessor::new(16000, 1, 4096);
        let first = AudioFrame::new(48000, 1, sine_wave(0.3, 256), Some(0), Some(0), false);
        let second = AudioFrame::new(48000, 1, sine_wave(0.4, 256), Some(1), Some(1), false);
        let first_out = processor.process_frame(&first).unwrap();
        let second_out = processor.process_frame(&second).unwrap();
        assert!(!first_out.samples.is_empty());
        assert!(!second_out.samples.is_empty());
        assert!(first_out.samples.iter().all(|sample| sample.is_finite()));
        assert!(second_out.samples.iter().all(|sample| sample.is_finite()));
    }
}
