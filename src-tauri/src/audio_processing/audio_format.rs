use std::ops::Deref;

#[derive(Debug, Clone, PartialEq)]
pub struct PcmFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
}

impl PcmFormat {
    pub fn new(sample_rate: u32, channels: u16, bits_per_sample: u16) -> Self {
        Self {
            sample_rate: sample_rate.max(1),
            channels: channels.max(1),
            bits_per_sample: bits_per_sample.max(1),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.sample_rate > 0 && self.channels > 0 && self.bits_per_sample > 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioTiming {
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub frame_index: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
    pub timestamp_ms: Option<u64>,
    pub frame_index: Option<u64>,
    pub is_last: bool,
}

impl AudioFrame {
    pub fn new(
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        timestamp_ms: Option<u64>,
        frame_index: Option<u64>,
        is_last: bool,
    ) -> Self {
        Self {
            sample_rate,
            channels,
            samples,
            timestamp_ms,
            frame_index,
            is_last,
        }
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 || self.samples.is_empty() {
            return 0;
        }
        let frames = self.samples.len() as u64 / self.channels as u64;
        frames.saturating_mul(1000) / self.sample_rate as u64
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
    pub timestamp_ms: Option<u64>,
    pub sequence: Option<u64>,
}

impl AudioChunk {
    pub fn from_frame(frame: &AudioFrame, sequence: Option<u64>) -> Self {
        Self {
            sample_rate: frame.sample_rate,
            channels: frame.channels,
            samples: frame.samples.clone(),
            timestamp_ms: frame.timestamp_ms,
            sequence,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedAudio {
    pub format: PcmFormat,
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub samples_per_channel: usize,
    pub timestamp_ms: Option<u64>,
    pub timing: AudioTiming,
}

impl ProcessedAudio {
    pub fn empty_for(sample_rate: u32, channels: u16) -> Self {
        Self {
            format: PcmFormat::new(sample_rate, channels, 32),
            samples: Vec::new(),
            sample_rate,
            samples_per_channel: 0,
            timestamp_ms: None,
            timing: AudioTiming {
                start_ms: None,
                end_ms: None,
                frame_index: None,
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

pub trait SampleBuffer {
    fn as_f32_slice(&self) -> &[f32];
}

impl SampleBuffer for [f32] {
    fn as_f32_slice(&self) -> &[f32] {
        self
    }
}

impl Deref for AudioFrame {
    type Target = [f32];

    fn deref(&self) -> &Self::Target {
        self.samples.as_slice()
    }
}
