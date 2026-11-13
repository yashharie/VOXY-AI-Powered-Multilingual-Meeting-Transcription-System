//! sherpa-onnx backed implementations (Cargo feature `sherpa`, crate `sherpa-rs` 0.6.x).
//!
//! * `SherpaSileroVad`  — Silero VAD through sherpa-onnx's VoiceActivityDetector.
//! * `SherpaEmbedder`   — speaker embeddings for the live tracker.
//! * `OfflineDiarizer`  — sherpa-onnx offline diarization (pyannote segmentation + embedding +
//!                        clustering). Whole-recording only; used for post-meeting refinement and
//!                        for measuring how good the live tracker is.

use std::path::Path;

use sherpa_rs::diarize::{Diarize, DiarizeConfig};
use sherpa_rs::silero_vad::{SileroVad, SileroVadConfig};
use sherpa_rs::speaker_id::{EmbeddingExtractor, ExtractorConfig};

use crate::core::error::{AppError, AppResult};
use crate::speaker_analysis::config::VadConfig;
use crate::speaker_analysis::diarization::SpeakerEmbedder;
use crate::speaker_analysis::types::{speaker_label, SpeakerInterval, SpeechChunk, SAMPLE_RATE};
use crate::speaker_analysis::vad::{VadBackend, VadOutput};

fn require_model(path: &Path) -> AppResult<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(AppError::ModelNotFound(path.display().to_string()))
    }
}

// ── Silero VAD ───────────────────────────────────────────────────────────────

pub struct SherpaSileroVad {
    vad: SileroVad,
}

impl SherpaSileroVad {
    pub fn new(model: &Path, cfg: &VadConfig) -> AppResult<Self> {
        require_model(model)?;
        let config = SileroVadConfig {
            model: model.to_string_lossy().into_owned(),
            min_silence_duration: cfg.min_silence_ms as f32 / 1000.0,
            min_speech_duration: cfg.min_speech_ms as f32 / 1000.0,
            max_speech_duration: cfg.max_speech_ms as f32 / 1000.0,
            threshold: cfg.threshold,
            sample_rate: SAMPLE_RATE,
            window_size: 512,
            provider: None,
            num_threads: Some(1),
            debug: false,
        };
        let vad = SileroVad::new(config, 60.0)
            .map_err(|e| AppError::Internal(format!("Silero VAD init failed: {e}")))?;
        Ok(Self { vad })
    }

    fn drain(&mut self) -> Vec<SpeechChunk> {
        let mut out = Vec::new();
        while !self.vad.is_empty() {
            let seg = self.vad.front();
            self.vad.pop();
            out.push(SpeechChunk {
                start_sample: seg.start.max(0) as u64,
                samples: seg.samples,
            });
        }
        out
    }
}

impl VadBackend for SherpaSileroVad {
    fn name(&self) -> &'static str {
        "silero (sherpa-onnx)"
    }

    fn accept(&mut self, samples: &[f32]) -> AppResult<VadOutput> {
        if samples.is_empty() {
            return Ok(VadOutput::default());
        }
        self.vad.accept_waveform(samples.to_vec());
        let in_speech = self.vad.is_speech();
        Ok(VadOutput {
            in_speech,
            completed: self.drain(),
        })
    }

    fn flush(&mut self) -> AppResult<Vec<SpeechChunk>> {
        self.vad.flush();
        Ok(self.drain())
    }
}

// ── Speaker embeddings ───────────────────────────────────────────────────────

pub struct SherpaEmbedder {
    extractor: EmbeddingExtractor,
}

impl SherpaEmbedder {
    pub fn new(model: &Path) -> AppResult<Self> {
        require_model(model)?;
        let extractor = EmbeddingExtractor::new(ExtractorConfig {
            model: model.to_string_lossy().into_owned(),
            provider: None,
            num_threads: Some(1),
            debug: false,
        })
        .map_err(|e| AppError::Internal(format!("speaker embedding model init failed: {e}")))?;
        Ok(Self { extractor })
    }
}

impl SpeakerEmbedder for SherpaEmbedder {
    fn name(&self) -> &'static str {
        "sherpa-onnx speaker embedding"
    }

    fn embed(&mut self, samples: &[f32]) -> AppResult<Vec<f32>> {
        self.extractor
            .compute_speaker_embedding(samples.to_vec(), SAMPLE_RATE)
            .map_err(|e| AppError::Internal(format!("speaker embedding failed: {e}")))
    }
}

// ── Offline diarization ──────────────────────────────────────────────────────

pub struct OfflineDiarizer {
    inner: Diarize,
}

impl OfflineDiarizer {
    /// `num_speakers = None` lets clustering decide using `threshold` (lower => more speakers).
    pub fn new(
        segmentation_model: &Path,
        embedding_model: &Path,
        num_speakers: Option<i32>,
        threshold: f32,
    ) -> AppResult<Self> {
        require_model(segmentation_model)?;
        require_model(embedding_model)?;
        let cfg = DiarizeConfig {
            // sherpa-rs maps `None` to 4 clusters, so "auto" must be passed as -1 explicitly.
            num_clusters: Some(num_speakers.unwrap_or(-1)),
            threshold: Some(threshold),
            min_duration_on: Some(0.3),
            min_duration_off: Some(0.5),
            provider: None,
            debug: false,
        };
        let inner = Diarize::new(segmentation_model, embedding_model, cfg)
            .map_err(|e| AppError::Internal(format!("offline diarization init failed: {e}")))?;
        Ok(Self { inner })
    }

    /// Diarize a whole recording (16 kHz mono f32). Segments may overlap in time when the
    /// segmentation model detects simultaneous speakers.
    pub fn diarize(&mut self, samples: Vec<f32>) -> AppResult<Vec<SpeakerInterval>> {
        let segments = match self.inner.compute(samples, None) {
            Ok(s) => s,
            // sherpa-rs reports "no speech found" as an error; for us that is an empty result.
            Err(e) if e.to_string().contains("No segments") => return Ok(Vec::new()),
            Err(e) => return Err(AppError::Internal(format!("offline diarization failed: {e}"))),
        };
        let mut order: Vec<i32> = Vec::new();
        Ok(segments
            .into_iter()
            .map(|s| {
                let idx = match order.iter().position(|x| *x == s.speaker) {
                    Some(i) => i,
                    None => {
                        order.push(s.speaker);
                        order.len() - 1
                    }
                };
                SpeakerInterval {
                    start_ms: (s.start.max(0.0) * 1000.0) as u64,
                    end_ms: (s.end.max(0.0) * 1000.0) as u64,
                    speaker: speaker_label(idx as u32),
                }
            })
            .collect())
    }
}
