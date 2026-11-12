//! Shared data types used across the audio capture module and by the
//! preprocessing module that consumes this module's output.

/// The source of an audio chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSource {
    /// System loopback (audio played by the computer).
    Loopback,
    /// Microphone input.
    Microphone,
}

/// A single captured audio chunk in the agreed PCM format.
///
/// ## Agreed PCM specification
/// | Parameter      | Value                              |
/// |----------------|------------------------------------|
/// | Encoding       | 32-bit IEEE float (f32)            |
/// | Sample rate    | 16 000 Hz (or native — see `sample_rate_hz`) |
/// | Channels       | 1 (mono) after down-mixing         |
/// | Byte order     | Native (little-endian on Windows)  |
///
/// The preprocessing module (Member 4) is responsible for resampling to 16 kHz
/// with Rubato if the device reports a different native sample rate. The raw
/// native rate is surfaced here so the preprocessor can decide.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// Interleaved f32 PCM samples (mono after down-mix by capture layer).
    ///
    /// Length = `frame_count` samples (since mono).
    pub samples: Vec<f32>,

    /// Native sample rate reported by the capture device at the time this
    /// chunk was captured.
    pub sample_rate_hz: u32,

    /// Number of audio frames (= samples for mono) in this chunk.
    pub frame_count: usize,

    /// Wall-clock timestamp (ms since Unix epoch) of the first sample.
    pub timestamp_ms: u64,

    /// Whether this chunk came from loopback or microphone.
    pub source: AudioSource,
}

impl AudioChunk {
    /// Create a new chunk, computing `frame_count` automatically.
    pub fn new(
        samples: Vec<f32>,
        sample_rate_hz: u32,
        timestamp_ms: u64,
        source: AudioSource,
    ) -> Self {
        let frame_count = samples.len();
        Self { samples, sample_rate_hz, frame_count, timestamp_ms, source }
    }

    /// Duration of this chunk in milliseconds.
    pub fn duration_ms(&self) -> f64 {
        (self.frame_count as f64 / self.sample_rate_hz as f64) * 1000.0
    }
}

/// A thin wrapper that timestamps a system time instant as milliseconds since
/// the Unix epoch. Used to stamp [`AudioChunk::timestamp_ms`].
pub(crate) fn unix_ms_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
