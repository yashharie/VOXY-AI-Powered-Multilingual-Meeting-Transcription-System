use rubato::{
    Resampler as RubatoResampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};

use crate::audio_processing::audio_format::AudioFrame;
use crate::audio_processing::validator::{validate_audio_frame, AudioValidationError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResampleConfig {
    pub input_sample_rate: u32,
    pub output_sample_rate: u32,
    pub channels: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResamplingError {
    UnsupportedRatio,
    InvalidConfiguration,
    ProcessingFailed,
}

pub struct Resampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    channels: usize,
    resampler: Option<SincFixedIn<f32>>,
}

impl Resampler {
    pub fn new(
        input_sample_rate: u32,
        output_sample_rate: u32,
        channels: usize,
    ) -> Result<Self, ResamplingError> {
        if input_sample_rate == 0 || output_sample_rate == 0 || channels == 0 {
            return Err(ResamplingError::InvalidConfiguration);
        }

        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };

        let resampler = SincFixedIn::<f32>::new(
            output_sample_rate as f64 / input_sample_rate as f64,
            2.0,
            params,
            2048,
            channels,
        )
        .map_err(|_| ResamplingError::UnsupportedRatio)?;

        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            channels,
            resampler: Some(resampler),
        })
    }

    fn linear_resample(
        samples: &[f32],
        input_sample_rate: u32,
        output_sample_rate: u32,
        channels: usize,
    ) -> Vec<f32> {
        if samples.is_empty() || input_sample_rate == 0 || output_sample_rate == 0 || channels == 0
        {
            return samples.to_vec();
        }
        if input_sample_rate == output_sample_rate {
            return samples.to_vec();
        }

        let input_frames = samples.len().div_ceil(channels);
        let output_frames = ((input_frames as f64 * output_sample_rate as f64
            / input_sample_rate as f64)
            .ceil() as usize)
            .max(1);
        let mut output = vec![0.0; output_frames * channels];

        for channel in 0..channels {
            for output_frame in 0..output_frames {
                let position =
                    output_frame as f64 * input_sample_rate as f64 / output_sample_rate as f64;
                let left_index = position.floor() as usize;
                let right_index = (left_index + 1).min(input_frames.saturating_sub(1));
                let left_sample = samples
                    .get(channel + left_index * channels)
                    .copied()
                    .unwrap_or(0.0);
                let right_sample = samples
                    .get(channel + right_index * channels)
                    .copied()
                    .unwrap_or(left_sample);
                let fraction = position - left_index as f64;
                let value = left_sample + (right_sample - left_sample) * fraction as f32;
                output[output_frame * channels + channel] = value;
            }
        }

        output
    }

    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<f32>, ResamplingError> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let channel_count = self.channels.max(1);
        let frames = samples.len().div_ceil(channel_count);
        if frames < 32 {
            return Ok(Self::linear_resample(
                samples,
                self.input_sample_rate(),
                self.output_sample_rate,
                channel_count,
            ));
        }

        let channel_data = if channel_count == 1 {
            vec![samples.to_vec()]
        } else {
            let mut channels = vec![Vec::with_capacity(frames); channel_count];
            for frame_idx in 0..frames {
                for chan in 0..channel_count {
                    let sample_idx = frame_idx * channel_count + chan;
                    if let Some(sample) = samples.get(sample_idx) {
                        channels[chan].push(*sample);
                    }
                }
            }
            channels
        };

        if let Some(resampler) = &mut self.resampler {
            let output = resampler
                .process_partial(Some(&channel_data), None)
                .map_err(|_| ResamplingError::ProcessingFailed)?;
            let mut flattened = Vec::new();
            for channel in &output {
                flattened.extend_from_slice(channel);
            }
            Ok(flattened)
        } else {
            Ok(samples.to_vec())
        }
    }

    pub fn input_sample_rate(&self) -> u32 {
        self.input_sample_rate
    }

    pub fn process_frame(
        &mut self,
        frame: &AudioFrame,
    ) -> Result<AudioFrame, AudioValidationError> {
        if let Err(err) = validate_audio_frame(frame) {
            return Err(err);
        }
        let input_samples = if frame.channels == self.channels as u16 {
            frame.samples.clone()
        } else {
            let mono = if frame.channels > 1 && self.channels == 1 {
                let mut samples = Vec::with_capacity(frame.samples.len() / frame.channels as usize);
                for index in 0..frame.samples.len() / frame.channels as usize {
                    let mut sum = 0.0f32;
                    for channel in 0..frame.channels as usize {
                        let pos = index * frame.channels as usize + channel;
                        if let Some(sample) = frame.samples.get(pos) {
                            sum += *sample;
                        }
                    }
                    samples.push(sum / frame.channels as f32);
                }
                samples
            } else {
                frame.samples.clone()
            };
            mono
        };

        let output = self
            .process(&input_samples)
            .map_err(|_| AudioValidationError::InvalidSampleRate)?;
        Ok(AudioFrame::new(
            self.output_sample_rate,
            self.channels as u16,
            output,
            frame.timestamp_ms,
            frame.frame_index,
            frame.is_last,
        ))
    }
}

pub fn resample_pcm(
    samples: &[f32],
    input_sample_rate: u32,
    output_sample_rate: u32,
    channels: usize,
) -> Result<Vec<f32>, ResamplingError> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    if input_sample_rate == 0 || output_sample_rate == 0 || channels == 0 {
        return Err(ResamplingError::InvalidConfiguration);
    }
    let mut resampler = Resampler::new(input_sample_rate, output_sample_rate, channels)?;
    resampler.process(samples)
}

pub fn resample_audio(
    samples: &[f32],
    input_sample_rate: u32,
    output_sample_rate: u32,
    channels: usize,
) -> Result<Vec<f32>, ResamplingError> {
    resample_pcm(samples, input_sample_rate, output_sample_rate, channels)
}

pub fn resample_frame(
    frame: &AudioFrame,
    target_sample_rate: u32,
    target_channels: usize,
) -> Result<AudioFrame, AudioValidationError> {
    if let Err(err) = validate_audio_frame(frame) {
        return Err(err);
    }
    let mut resampler = Resampler::new(frame.sample_rate, target_sample_rate, target_channels)
        .map_err(|_| AudioValidationError::InvalidSampleRate)?;
    let output = resampler
        .process(&frame.samples)
        .map_err(|_| AudioValidationError::InvalidSampleRate)?;
    Ok(AudioFrame::new(
        target_sample_rate,
        target_channels as u16,
        output,
        frame.timestamp_ms,
        frame.frame_index,
        frame.is_last,
    ))
}
