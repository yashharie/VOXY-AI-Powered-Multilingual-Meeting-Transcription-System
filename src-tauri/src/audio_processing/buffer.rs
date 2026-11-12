use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioBufferError {
    Overflow,
    InvalidSampleCount,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioBuffer {
    max_samples: usize,
    sample_rate: u32,
    channels: u16,
    queue: VecDeque<f32>,
}

impl AudioBuffer {
    pub fn new(max_samples: usize, channels: u16, sample_rate: u32) -> Self {
        Self {
            max_samples: max_samples.max(1),
            sample_rate: sample_rate.max(1),
            channels: channels.max(1),
            queue: VecDeque::new(),
        }
    }

    pub fn push_samples(
        &mut self,
        mut samples: Vec<f32>,
        sample_rate: u32,
        channels: u16,
    ) -> Result<(), AudioBufferError> {
        if samples.is_empty() {
            return Ok(());
        }
        if channels == 0 || sample_rate == 0 {
            return Err(AudioBufferError::InvalidSampleCount);
        }

        let total = self.queue.len() + samples.len();
        if total > self.max_samples {
            let overflow = total - self.max_samples;
            for _ in 0..overflow.min(self.queue.len()) {
                self.queue.pop_front();
            }
            if self.queue.len() + samples.len() > self.max_samples {
                samples.truncate(self.max_samples.saturating_sub(self.queue.len()));
            }
        }

        self.queue.extend(samples);
        self.sample_rate = sample_rate;
        self.channels = channels;
        Ok(())
    }

    pub fn len_samples(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn drain_all(&mut self) -> Vec<f32> {
        let mut drained = Vec::with_capacity(self.queue.len());
        while let Some(sample) = self.queue.pop_front() {
            drained.push(sample);
        }
        drained
    }

    pub fn drain(&mut self, count: usize) -> Vec<f32> {
        let count = count.min(self.queue.len());
        let mut drained = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(sample) = self.queue.pop_front() {
                drained.push(sample);
            }
        }
        drained
    }
}

impl Default for AudioBuffer {
    fn default() -> Self {
        Self::new(4096, 1, 16000)
    }
}
