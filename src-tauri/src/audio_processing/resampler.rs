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
    /// Per-channel input waiting until a complete fixed-size Rubato chunk is available.
    input_carry: Vec<Vec<f32>>,
    /// Total source frames accepted by this resampler instance.
    total_input_frames: usize,
    /// Total frames produced by Rubato before startup-delay compensation.
    raw_output_frames: usize,
    /// Initial filter delay reported by Rubato, in output frames.
    startup_delay_frames: usize,
    /// Initial delay still waiting to be discarded from output.
    startup_delay_remaining: usize,
    /// Output frames already returned to the caller after delay compensation.
    output_frames_emitted: usize,
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

        let startup_delay_frames = resampler.output_delay();

        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            channels,
            resampler: Some(resampler),
            input_carry: vec![Vec::new(); channels],
            total_input_frames: 0,
            raw_output_frames: 0,
            startup_delay_frames,
            startup_delay_remaining: startup_delay_frames,
            output_frames_emitted: 0,
        })
    }

    /// Feed arbitrary interleaved input blocks into the fixed-input Rubato
    /// resampler. WASAPI blocks (typically 4,800 frames) are larger than the
    /// 2,048-frame resampler chunk, so retain the remainder and process only
    /// complete chunks. Padding every capture callback with process_partial()
    /// would insert silence and distort the audio timeline.
    pub fn process(&mut self, samples: &[f32]) -> Result<Vec<f32>, ResamplingError> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let channel_count = self.channels.max(1);
        if samples.len() % channel_count != 0 {
            return Err(ResamplingError::InvalidConfiguration);
        }

        if self.input_sample_rate == self.output_sample_rate {
            return Ok(samples.to_vec());
        }

        self.total_input_frames += samples.len() / channel_count;

        for frame in samples.chunks_exact(channel_count) {
            for (channel, sample) in frame.iter().enumerate() {
                self.input_carry[channel].push(*sample);
            }
        }

        let Some(resampler) = &mut self.resampler else {
            return Ok(samples.to_vec());
        };

        let mut interleaved_output = Vec::new();
        let chunk_frames = resampler.input_frames_next();

        while self.input_carry[0].len() >= chunk_frames {
            let mut input_chunk = Vec::with_capacity(channel_count);
            for channel in 0..channel_count {
                let chunk = self.input_carry[channel]
                    .drain(..chunk_frames)
                    .collect::<Vec<f32>>();
                input_chunk.push(chunk);
            }

            let output = resampler
                .process(&input_chunk, None)
                .map_err(|_| ResamplingError::ProcessingFailed)?;
            let output_frames = output.first().map_or(0, Vec::len);
            self.raw_output_frames += output_frames;

            // Rubato's sinc filter has a startup delay. Discard it once per
            // stream so transcript timestamps remain aligned with captured audio.
            let frames_to_trim = self.startup_delay_remaining.min(output_frames);
            self.startup_delay_remaining -= frames_to_trim;

            // Rubato returns one Vec per channel. Convert back to interleaved
            // samples because the rest of VOXY's audio pipeline uses interleaved PCM.
            for frame_idx in frames_to_trim..output_frames {
                for channel in 0..channel_count {
                    if let Some(sample) = output.get(channel).and_then(|data| data.get(frame_idx)) {
                        interleaved_output.push(*sample);
                    }
                }
            }
        }

        self.output_frames_emitted += interleaved_output.len() / channel_count;
        Ok(interleaved_output)
    }

    /// Process the final input remainder and drain the sinc filter's delayed tail.
    /// Rubato documents that a complete clip needs output_delay() extra frames,
    /// after which the initial delay is removed to recover the original timeline.
    pub fn flush(&mut self) -> Result<Vec<f32>, ResamplingError> {
        if self.input_sample_rate == self.output_sample_rate {
            return Ok(Vec::new());
        }

        let Some(resampler) = &mut self.resampler else {
            return Ok(Vec::new());
        };

        let remaining_frames = self.input_carry.first().map_or(0, Vec::len);
        let mut interleaved_output = Vec::new();

        if remaining_frames > 0 {
            let input = std::mem::replace(
                &mut self.input_carry,
                vec![Vec::new(); self.channels],
            );
            let output = resampler
                .process_partial(Some(&input), None)
                .map_err(|_| ResamplingError::ProcessingFailed)?;
            let output_frames = output.first().map_or(0, Vec::len);
            self.raw_output_frames += output_frames;

            for frame_idx in 0..output_frames {
                for channel in 0..self.channels {
                    if let Some(sample) = output.get(channel).and_then(|data| data.get(frame_idx)) {
                        interleaved_output.push(*sample);
                    }
                }
            }
        }

        let ratio = self.output_sample_rate as f64 / self.input_sample_rate as f64;
        let expected_frames = (self.total_input_frames as f64 * ratio).ceil() as usize;
        let required_raw_frames = expected_frames + self.startup_delay_frames;

        // The sinc filter may still hold valid tail samples after the last
        // source frame. Feed zero-length partial chunks until that delayed tail
        // has been emitted; cap the returned result to the exact resampled length.
        while self.raw_output_frames < required_raw_frames {
            let output = resampler
                .process_partial::<&[f32]>(None, None)
                .map_err(|_| ResamplingError::ProcessingFailed)?;
            let output_frames = output.first().map_or(0, Vec::len);
            if output_frames == 0 {
                break;
            }
            self.raw_output_frames += output_frames;

            for frame_idx in 0..output_frames {
                for channel in 0..self.channels {
                    if let Some(sample) = output.get(channel).and_then(|data| data.get(frame_idx)) {
                        interleaved_output.push(*sample);
                    }
                }
            }
        }

        let channel_count = self.channels.max(1);
        let frames_to_trim = self.startup_delay_remaining.min(
            interleaved_output.len() / channel_count,
        );
        if frames_to_trim > 0 {
            interleaved_output.drain(..frames_to_trim * channel_count);
            self.startup_delay_remaining -= frames_to_trim;
        }

        let frames_needed = expected_frames.saturating_sub(self.output_frames_emitted);
        interleaved_output.truncate(
            interleaved_output.len().min(frames_needed * channel_count),
        );
        self.output_frames_emitted += interleaved_output.len() / channel_count;

        Ok(interleaved_output)
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

        let mut output = self
            .process(&input_samples)
            .map_err(|_| AudioValidationError::InvalidSampleRate)?;
        if frame.is_last {
            output.extend(
                self.flush()
                    .map_err(|_| AudioValidationError::InvalidSampleRate)?,
            );
        }
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
    let mut output = resampler.process(samples)?;
    output.extend(resampler.flush()?);
    Ok(output)
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
    let processed = resampler.process_frame(frame)?;
    let mut output = processed.samples;
    output.extend(
        resampler
            .flush()
            .map_err(|_| AudioValidationError::InvalidSampleRate)?,
    );
    Ok(AudioFrame::new(
        target_sample_rate,
        target_channels as u16,
        output,
        frame.timestamp_ms,
        frame.frame_index,
        frame.is_last,
    ))
}


#[cfg(test)]
mod tests {
    use super::Resampler;

    #[test]
    fn buffers_capture_blocks_larger_than_rubato_fixed_chunk() {
        let mut resampler = Resampler::new(48_000, 16_000, 1).unwrap();
        let input = vec![0.25f32; 4_800];

        let mut output = resampler.process(&input).unwrap();
        assert!(!output.is_empty(), "complete 2,048-frame chunks should be processed");
        assert_eq!(
            resampler.input_carry[0].len(),
            704,
            "the unprocessed remainder must be retained for the next capture block"
        );

        output.extend(resampler.flush().unwrap());
        assert!(
            (output.len() as isize - 1_600).abs() <= 8,
            "4,800 frames at 48 kHz should produce about 1,600 frames at 16 kHz, got {}",
            output.len()
        );
    }

    #[test]
    fn combines_small_capture_blocks_until_a_full_resampler_chunk_exists() {
        let mut resampler = Resampler::new(48_000, 16_000, 1).unwrap();

        let first = resampler.process(&vec![0.1f32; 1_000]).unwrap();
        assert!(first.is_empty(), "a partial input block must be buffered, not padded");

        let second = resampler.process(&vec![0.1f32; 1_048]).unwrap();
        assert!(!second.is_empty(), "the completed fixed-size chunk should now be processed");
        assert!(resampler.input_carry[0].is_empty());
    }
}
