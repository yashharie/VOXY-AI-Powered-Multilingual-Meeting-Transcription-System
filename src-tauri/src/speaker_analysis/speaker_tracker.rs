//! Online speaker tracking: keeps one centroid per speaker and matches each new
//! speaker embedding by cosine similarity. This is what keeps "Speaker 1" stable
//! across consecutive audio chunks (sherpa-onnx's own diarization is offline-only).

use crate::core::error::{AppError, AppResult};
use crate::speaker_analysis::config::DiarizationConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    /// 0-based speaker id, numbered in order of first appearance.
    pub speaker_id: u32,
    pub score: f32,
    pub is_new: bool,
}

#[derive(Debug, Clone)]
struct Speaker {
    /// Running sum of unit-length embeddings (direction = centroid).
    sum: Vec<f32>,
    count: u32,
}

#[derive(Debug, Clone)]
pub struct SpeakerTracker {
    cfg: DiarizationConfig,
    speakers: Vec<Speaker>,
    last: Option<u32>,
    dim: Option<usize>,
}

/// Counts beyond this are down-weighted so a long-running speaker's centroid can still adapt.
const MAX_WEIGHT: u32 = 50;

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (na, nb) = (norm(a), norm(b));
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>() / (na * nb)
}

impl SpeakerTracker {
    pub fn new(cfg: DiarizationConfig) -> Self {
        Self {
            cfg,
            speakers: Vec::new(),
            last: None,
            dim: None,
        }
    }

    pub fn speaker_count(&self) -> usize {
        self.speakers.len()
    }

    pub fn reset(&mut self) {
        self.speakers.clear();
        self.last = None;
        self.dim = None;
    }

    pub fn assign(&mut self, embedding: &[f32]) -> AppResult<Assignment> {
        if embedding.is_empty() || embedding.iter().any(|x| !x.is_finite()) {
            return Err(AppError::Internal("invalid speaker embedding".into()));
        }
        let n = norm(embedding);
        if n == 0.0 {
            return Err(AppError::Internal("zero-length speaker embedding".into()));
        }
        match self.dim {
            None => self.dim = Some(embedding.len()),
            Some(d) if d != embedding.len() => {
                return Err(AppError::Internal(format!(
                    "embedding size changed from {d} to {}",
                    embedding.len()
                )))
            }
            _ => {}
        }
        let unit: Vec<f32> = embedding.iter().map(|x| x / n).collect();

        // Score against every known speaker.
        let scores: Vec<f32> = self.speakers.iter().map(|s| cosine(&s.sum, &unit)).collect();
        let best = scores
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, s)| (i as u32, *s));

        let chosen: (u32, f32, bool) = match best {
            None => (self.push_new(&unit), 1.0, true),
            Some((mut id, mut score)) => {
                // Stickiness: prefer the previous speaker when it is nearly as good.
                if let Some(last) = self.last {
                    let ls = scores[last as usize];
                    if last != id && ls >= self.cfg.similarity_threshold && score - ls < self.cfg.stickiness {
                        id = last;
                        score = ls;
                    }
                }
                if score >= self.cfg.similarity_threshold {
                    (id, score, false)
                } else if self.speakers.len() < self.cfg.max_speakers {
                    (self.push_new(&unit), score, true)
                } else {
                    // Speaker cap reached: attach to the closest one rather than invent more.
                    (id, score, false)
                }
            }
        };

        if !chosen.2 {
            let s = &mut self.speakers[chosen.0 as usize];
            // Keep the centroid on a comparable scale: sum of up to MAX_WEIGHT unit vectors.
            if s.count >= MAX_WEIGHT {
                let k = (MAX_WEIGHT - 1) as f32 / MAX_WEIGHT as f32;
                s.sum.iter_mut().for_each(|x| *x *= k);
            } else {
                s.count += 1;
            }
            s.sum.iter_mut().zip(&unit).for_each(|(a, b)| *a += b);
        }
        self.last = Some(chosen.0);
        Ok(Assignment {
            speaker_id: chosen.0,
            score: chosen.1,
            is_new: chosen.2,
        })
    }

    fn push_new(&mut self, unit: &[f32]) -> u32 {
        self.speakers.push(Speaker {
            sum: unit.to_vec(),
            count: 1,
        });
        (self.speakers.len() - 1) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny deterministic noise source (no rand dependency).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) - 0.5
        }
    }

    fn emb(center: &[f32], noise: f32, rng: &mut Lcg) -> Vec<f32> {
        center.iter().map(|c| c + noise * rng.next()).collect()
    }

    fn tracker() -> SpeakerTracker {
        SpeakerTracker::new(DiarizationConfig::default())
    }

    #[test]
    fn first_embedding_creates_speaker_one() {
        let mut t = tracker();
        let a = t.assign(&[1.0, 0.0, 0.0]).unwrap();
        assert_eq!((a.speaker_id, a.is_new), (0, true));
    }

    #[test]
    fn labels_stay_stable_across_turns() {
        let (a, b, c) = ([1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]);
        let mut rng = Lcg(7);
        let mut t = tracker();
        let order = [&a, &b, &a, &c, &b, &a, &c];
        let ids: Vec<u32> = order
            .iter()
            .map(|c| t.assign(&emb(*c, 0.3, &mut rng)).unwrap().speaker_id)
            .collect();
        assert_eq!(ids, vec![0, 1, 0, 2, 1, 0, 2]);
        assert_eq!(t.speaker_count(), 3);
    }

    #[test]
    fn speaker_cap_is_respected() {
        let mut cfg = DiarizationConfig::default();
        cfg.max_speakers = 2;
        let mut t = SpeakerTracker::new(cfg);
        t.assign(&[1.0, 0.0, 0.0]).unwrap();
        t.assign(&[0.0, 1.0, 0.0]).unwrap();
        let third = t.assign(&[0.0, 0.0, 1.0]).unwrap();
        assert!(!third.is_new);
        assert_eq!(t.speaker_count(), 2);
    }

    #[test]
    fn rejects_bad_embeddings() {
        let mut t = tracker();
        assert!(t.assign(&[]).is_err());
        assert!(t.assign(&[0.0, 0.0]).is_err());
        assert!(t.assign(&[f32::NAN, 1.0]).is_err());
        t.assign(&[1.0, 0.0]).unwrap();
        assert!(t.assign(&[1.0, 0.0, 0.0]).is_err()); // dimension changed
    }

    #[test]
    fn stickiness_keeps_previous_speaker_on_ambiguous_window() {
        let mut t = tracker();
        t.assign(&[1.0, 0.0]).unwrap(); // speaker 0
        t.assign(&[0.0, 1.0]).unwrap(); // speaker 1 (last)
        // Slightly closer to speaker 0 than speaker 1, but within the stickiness margin.
        let a = t.assign(&[0.72, 0.70]).unwrap();
        assert_eq!(a.speaker_id, 1);
    }
}
