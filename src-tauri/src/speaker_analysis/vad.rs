//! Voice activity detection backends.
//!
//! * `SherpaSileroVad` (feature `sherpa`, see `sherpa.rs`) — Silero VAD via sherpa-onnx / ONNX Runtime. Primary.
//! * `EnergyVad` — pure-Rust RMS gate with adaptive noise floor. Fallback used when the
//!   Silero model is missing or fails to load, so the pipeline never stops because of VAD.

use crate::core::error::AppResult;
use crate::speaker_analysis::config::VadConfig;
use crate::speaker_analysis::types::{ms_to_samples, SpeechChunk, SAMPLE_RATE};

/// Result of feeding one block of audio to a VAD backend.
#[derive(Debug, Default)]
pub struct VadOutput {
    /// True while speech is in progress (includes the hang-over after the last voiced sample),
    /// i.e. the block should be passed to speech recognition.
    pub in_speech: bool,
    /// Speech regions that were completed by this block.
    pub completed: Vec<SpeechChunk>,
}

pub trait VadBackend: Send {
    fn name(&self) -> &'static str;
    /// Feed consecutive 16 kHz mono samples. Must be called with every block, in order.
    fn accept(&mut self, samples: &[f32]) -> AppResult<VadOutput>;
    /// End of stream: close any open speech region.
    fn flush(&mut self) -> AppResult<Vec<SpeechChunk>>;
}

const WINDOW: usize = 512; // 32 ms at 16 kHz, same window size Silero uses
const WINDOW_MS: u64 = WINDOW as u64 * 1000 / SAMPLE_RATE as u64;

struct OpenSegment {
    start_sample: u64,
    samples: Vec<f32>,
    speech_windows: u64,
    /// Length of `samples` at the end of the last voiced window.
    last_voiced_len: usize,
}

pub struct EnergyVad {
    cfg: VadConfig,
    carry: Vec<f32>,
    /// Stream position (in samples) of the first sample of the next window.
    clock: u64,
    noise_floor: f32,
    seg: Option<OpenSegment>,
    silence_windows: u64,
}

impl EnergyVad {
    pub fn new(cfg: VadConfig) -> Self {
        Self {
            cfg,
            carry: Vec::new(),
            clock: 0,
            noise_floor: 0.003,
            seg: None,
            silence_windows: 0,
        }
    }

    fn close_segment(&mut self, out: &mut Vec<SpeechChunk>) {
        if let Some(mut seg) = self.seg.take() {
            self.silence_windows = 0;
            if seg.speech_windows * WINDOW_MS < self.cfg.min_speech_ms {
                return; // too short: treat as noise
            }
            // Keep two windows of tail so word endings are not clipped.
            let keep = (seg.last_voiced_len + 2 * WINDOW).min(seg.samples.len());
            seg.samples.truncate(keep);
            out.push(SpeechChunk {
                start_sample: seg.start_sample,
                samples: seg.samples,
            });
        }
    }

    fn step(&mut self, win: &[f32], out: &mut Vec<SpeechChunk>) {
        let rms = (win.iter().map(|x| x * x).sum::<f32>() / win.len() as f32).sqrt();
        let threshold = (self.noise_floor * self.cfg.energy_noise_factor).max(self.cfg.energy_min_rms);
        let voiced = rms > threshold;
        if !voiced {
            // Track the noise floor only on non-speech windows.
            self.noise_floor = 0.97 * self.noise_floor + 0.03 * rms;
        }
        let win_start = self.clock;
        self.clock += WINDOW as u64;

        match self.seg.as_mut() {
            None => {
                if voiced {
                    self.silence_windows = 0;
                    self.seg = Some(OpenSegment {
                        start_sample: win_start,
                        samples: win.to_vec(),
                        speech_windows: 1,
                        last_voiced_len: WINDOW,
                    });
                }
            }
            Some(seg) => {
                seg.samples.extend_from_slice(win);
                if voiced {
                    seg.speech_windows += 1;
                    seg.last_voiced_len = seg.samples.len();
                    self.silence_windows = 0;
                } else {
                    self.silence_windows += 1;
                }
                let silence_ms = self.silence_windows * WINDOW_MS;
                let too_long = seg.samples.len() as u64 >= ms_to_samples(self.cfg.max_speech_ms);
                if silence_ms >= self.cfg.min_silence_ms || too_long {
                    self.close_segment(out);
                }
            }
        }
    }
}

impl VadBackend for EnergyVad {
    fn name(&self) -> &'static str {
        "energy"
    }

    fn accept(&mut self, samples: &[f32]) -> AppResult<VadOutput> {
        self.carry.extend_from_slice(samples);
        let buf = std::mem::take(&mut self.carry);
        let mut completed = Vec::new();
        let mut chunks = buf.chunks_exact(WINDOW);
        for win in &mut chunks {
            self.step(win, &mut completed);
        }
        self.carry = chunks.remainder().to_vec();
        Ok(VadOutput {
            in_speech: self.seg.is_some(),
            completed,
        })
    }

    fn flush(&mut self) -> AppResult<Vec<SpeechChunk>> {
        let mut out = Vec::new();
        self.close_segment(&mut out);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn tone(freq: f32, ms: u64, amp: f32) -> Vec<f32> {
        let n = ms_to_samples(ms) as usize;
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SAMPLE_RATE as f32).sin())
            .collect()
    }

    pub fn quiet(ms: u64) -> Vec<f32> {
        // very low-level deterministic "room noise"
        (0..ms_to_samples(ms) as usize)
            .map(|i| 0.001 * ((i * 7919 % 13) as f32 / 13.0 - 0.5))
            .collect()
    }

    #[test]
    fn detects_speech_and_silence_regions() {
        let mut vad = EnergyVad::new(VadConfig::default());
        let mut audio = quiet(1000);
        audio.extend(tone(220.0, 1500, 0.3));
        audio.extend(quiet(1500));
        audio.extend(tone(220.0, 800, 0.3));
        audio.extend(quiet(1000));

        let mut chunks = Vec::new();
        // feed in odd-sized blocks to exercise the carry buffer
        for block in audio.chunks(1234) {
            chunks.extend(vad.accept(block).unwrap().completed);
        }
        chunks.extend(vad.flush().unwrap());

        assert_eq!(chunks.len(), 2, "{chunks:?}");
        let (a, b) = (&chunks[0], &chunks[1]);
        assert!((a.start_ms() as i64 - 1000).abs() < 100, "start {}", a.start_ms());
        assert!((a.end_ms() as i64 - 2500).abs() < 200, "end {}", a.end_ms());
        assert!((b.start_ms() as i64 - 4000).abs() < 100, "start {}", b.start_ms());
    }

    #[test]
    fn in_speech_flag_follows_activity() {
        let mut vad = EnergyVad::new(VadConfig::default());
        assert!(!vad.accept(&quiet(500)).unwrap().in_speech);
        assert!(vad.accept(&tone(220.0, 500, 0.3)).unwrap().in_speech);
        // still true during the hang-over
        assert!(vad.accept(&quiet(200)).unwrap().in_speech);
        assert!(!vad.accept(&quiet(1000)).unwrap().in_speech);
    }

    #[test]
    fn short_blips_are_ignored() {
        let mut vad = EnergyVad::new(VadConfig::default());
        let mut audio = quiet(500);
        audio.extend(tone(220.0, 100, 0.3)); // < min_speech_ms
        audio.extend(quiet(1500));
        let mut n = 0;
        for block in audio.chunks(800) {
            n += vad.accept(block).unwrap().completed.len();
        }
        n += vad.flush().unwrap().len();
        assert_eq!(n, 0);
    }

    #[test]
    fn long_speech_is_force_split() {
        let mut cfg = VadConfig::default();
        cfg.max_speech_ms = 2_000;
        let mut vad = EnergyVad::new(cfg);
        let mut n = 0;
        for block in tone(220.0, 5_000, 0.3).chunks(1600) {
            n += vad.accept(block).unwrap().completed.len();
        }
        n += vad.flush().unwrap().len();
        assert!(n >= 2, "expected several segments, got {n}");
    }
}
