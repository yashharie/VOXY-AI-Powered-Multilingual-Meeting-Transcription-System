//! Live (chunk-by-chunk) diarization: speech chunk -> embedding windows -> speaker tracker -> timeline.

use std::collections::VecDeque;

use crate::core::error::AppResult;
use crate::speaker_analysis::config::DiarizationConfig;
use crate::speaker_analysis::speaker_tracker::SpeakerTracker;
use crate::speaker_analysis::types::{ms_to_samples, speaker_label, SpeakerInterval, SpeechChunk};

/// Turns a block of 16 kHz mono samples into a speaker embedding.
pub trait SpeakerEmbedder: Send {
    fn name(&self) -> &'static str;
    fn embed(&mut self, samples: &[f32]) -> AppResult<Vec<f32>>;
}

/// Split `len` samples into near-equal windows of roughly `window` samples.
/// Returns nothing when the whole span is shorter than `min`.
pub fn plan_windows(len: usize, window: usize, min: usize) -> Vec<(usize, usize)> {
    if len < min || window == 0 {
        return Vec::new();
    }
    let count = ((len + window / 2) / window).max(1);
    (0..count).map(|i| (i * len / count, (i + 1) * len / count)).collect()
}

pub struct DiarState {
    cfg: DiarizationConfig,
    embedder: Box<dyn SpeakerEmbedder>,
    tracker: SpeakerTracker,
    timeline: VecDeque<SpeakerInterval>,
    undelivered: Vec<SpeakerInterval>,
}

impl DiarState {
    pub fn new(cfg: DiarizationConfig, embedder: Box<dyn SpeakerEmbedder>) -> Self {
        Self {
            tracker: SpeakerTracker::new(cfg.clone()),
            cfg,
            embedder,
            timeline: VecDeque::new(),
            undelivered: Vec::new(),
        }
    }

    pub fn embedder_name(&self) -> &'static str {
        self.embedder.name()
    }

    pub fn speaker_count(&self) -> usize {
        self.tracker.speaker_count()
    }

    /// Label one finished speech region. Windows inside it are labelled separately, so a speaker
    /// change *without* a pause is still caught (at window resolution, ~2 s by default).
    pub fn process_chunk(&mut self, chunk: &SpeechChunk) -> AppResult<()> {
        let windows = plan_windows(
            chunk.samples.len(),
            ms_to_samples(self.cfg.window_ms) as usize,
            ms_to_samples(self.cfg.min_window_ms) as usize,
        );
        let mut produced: Vec<SpeakerInterval> = Vec::new();
        for (a, b) in windows {
            let emb = self.embedder.embed(&chunk.samples[a..b])?;
            let assignment = self.tracker.assign(&emb)?;
            let speaker = speaker_label(assignment.speaker_id);
            let start_ms = crate::speaker_analysis::types::samples_to_ms(chunk.start_sample + a as u64);
            let end_ms = crate::speaker_analysis::types::samples_to_ms(chunk.start_sample + b as u64);
            match produced.last_mut() {
                Some(last) if last.speaker == speaker => last.end_ms = end_ms,
                _ => produced.push(SpeakerInterval { start_ms, end_ms, speaker }),
            }
        }
        for iv in produced {
            self.timeline.push_back(iv.clone());
            self.undelivered.push(iv);
        }
        while self.timeline.len() > self.cfg.max_intervals {
            self.timeline.pop_front();
        }
        Ok(())
    }

    /// Intervals produced since the last call (to forward to the transcript module).
    pub fn drain_new_intervals(&mut self) -> Vec<SpeakerInterval> {
        std::mem::take(&mut self.undelivered)
    }

    pub fn intervals(&self) -> Vec<SpeakerInterval> {
        self.timeline.iter().cloned().collect()
    }

    /// Speaker with the largest time overlap with `[start_ms, end_ms)`. If nothing overlaps,
    /// the nearest interval within `lookup_tolerance_ms` is used; otherwise `None`.
    pub fn speaker_for(&self, start_ms: u64, end_ms: u64) -> Option<String> {
        let end_ms = end_ms.max(start_ms);
        let mut best: Option<(&str, u64)> = None;
        let mut per_speaker: Vec<(&str, u64)> = Vec::new();
        for iv in self.timeline.iter().rev() {
            let overlap = end_ms.min(iv.end_ms).saturating_sub(start_ms.max(iv.start_ms));
            if overlap > 0 {
                match per_speaker.iter_mut().find(|(s, _)| *s == iv.speaker) {
                    Some(e) => e.1 += overlap,
                    None => per_speaker.push((iv.speaker.as_str(), overlap)),
                }
            }
        }
        for (s, o) in per_speaker {
            if best.map_or(true, |(_, bo)| o > bo) {
                best = Some((s, o));
            }
        }
        if let Some((s, _)) = best {
            return Some(s.to_string());
        }
        // No overlap: nearest interval within tolerance.
        let mid = (start_ms + end_ms) / 2;
        self.timeline
            .iter()
            .map(|iv| {
                let d = if mid < iv.start_ms {
                    iv.start_ms - mid
                } else {
                    mid.saturating_sub(iv.end_ms)
                };
                (d, iv)
            })
            .filter(|(d, _)| *d <= self.cfg.lookup_tolerance_ms)
            .min_by_key(|(d, _)| *d)
            .map(|(_, iv)| iv.speaker.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::AppError;

    /// Test embedder: classifies a block by its dominant frequency (zero-crossing rate).
    pub struct FreqEmbedder {
        pub fail: bool,
    }
    impl SpeakerEmbedder for FreqEmbedder {
        fn name(&self) -> &'static str {
            "freq-test"
        }
        fn embed(&mut self, s: &[f32]) -> AppResult<Vec<f32>> {
            if self.fail {
                return Err(AppError::Internal("boom".into()));
            }
            let zc = s.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count() as f32;
            let freq = zc / 2.0 / (s.len() as f32 / 16_000.0);
            Ok(if freq < 300.0 { vec![1.0, 0.0, 0.0] } else if freq < 600.0 { vec![0.0, 1.0, 0.0] } else { vec![0.0, 0.0, 1.0] })
        }
    }

    fn tone(freq: f32, ms: u64) -> Vec<f32> {
        (0..ms_to_samples(ms) as usize)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * freq * i as f32 / 16_000.0).sin())
            .collect()
    }

    fn state() -> DiarState {
        DiarState::new(DiarizationConfig::default(), Box::new(FreqEmbedder { fail: false }))
    }

    #[test]
    fn windows_cover_chunk_and_skip_short_ones() {
        assert!(plan_windows(8_000, 32_000, 12_800).is_empty()); // 0.5 s < min
        let w = plan_windows(96_000, 32_000, 12_800); // 6 s -> 3 windows
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].0, 0);
        assert_eq!(w[2].1, 96_000);
        assert!(w.windows(2).all(|p| p[0].1 == p[1].0));
    }

    #[test]
    fn labels_turns_and_keeps_them_stable() {
        let mut d = state();
        let mk = |start_ms: u64, f: f32| SpeechChunk { start_sample: ms_to_samples(start_ms), samples: tone(f, 1500) };
        d.process_chunk(&mk(1000, 200.0)).unwrap();
        d.process_chunk(&mk(3500, 400.0)).unwrap();
        d.process_chunk(&mk(6000, 200.0)).unwrap();
        let iv = d.intervals();
        let names: Vec<&str> = iv.iter().map(|i| i.speaker.as_str()).collect();
        assert_eq!(names, ["Speaker 1", "Speaker 2", "Speaker 1"]);
        assert_eq!((iv[1].start_ms, iv[1].end_ms), (3500, 5000));
    }

    #[test]
    fn speaker_change_inside_one_chunk_is_split() {
        let mut d = state();
        let mut s = tone(200.0, 2000);
        s.extend(tone(400.0, 2000));
        d.process_chunk(&SpeechChunk { start_sample: 0, samples: s }).unwrap();
        let iv = d.intervals();
        assert_eq!(iv.len(), 2, "{iv:?}");
        assert_eq!((iv[0].speaker.as_str(), iv[1].speaker.as_str()), ("Speaker 1", "Speaker 2"));
    }

    #[test]
    fn lookup_uses_overlap_then_nearest() {
        let mut d = state();
        d.process_chunk(&SpeechChunk { start_sample: ms_to_samples(1000), samples: tone(200.0, 2000) }).unwrap();
        d.process_chunk(&SpeechChunk { start_sample: ms_to_samples(4000), samples: tone(400.0, 2000) }).unwrap();
        assert_eq!(d.speaker_for(1200, 2800).as_deref(), Some("Speaker 1"));
        assert_eq!(d.speaker_for(2800, 4600).as_deref(), Some("Speaker 2")); // 600 ms vs 200 ms overlap
        assert_eq!(d.speaker_for(3200, 3600).as_deref(), Some("Speaker 1")); // gap, nearest = S1 (200 ms away)
        assert_eq!(d.speaker_for(20_000, 21_000), None);
    }

    #[test]
    fn drain_returns_each_interval_once() {
        let mut d = state();
        d.process_chunk(&SpeechChunk { start_sample: 0, samples: tone(200.0, 1500) }).unwrap();
        assert_eq!(d.drain_new_intervals().len(), 1);
        assert!(d.drain_new_intervals().is_empty());
    }

    #[test]
    fn embedder_error_is_returned() {
        let mut d = DiarState::new(DiarizationConfig::default(), Box::new(FreqEmbedder { fail: true }));
        assert!(d.process_chunk(&SpeechChunk { start_sample: 0, samples: tone(200.0, 1500) }).is_err());
    }
}
