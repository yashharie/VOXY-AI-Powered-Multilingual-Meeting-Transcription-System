//! VAD and speaker diarization module — owned by Member 6.
//!
//! Branch: feature/vad-diarization — Directory: src-tauri/src/speaker_analysis/
//!
//! ```text
//! ProcessedAudio ──► VadEngine::is_speech ──► bool  (gate for Whisper)
//!                         │
//!                         └─ finished speech regions ──► embeddings (windows) ──► SpeakerTracker
//!                                                                  │
//!       DiarizationEngine::assign_speaker(start_ms, end_ms) ◄──────┤  best-effort lookup
//!       DiarizationEngine::drain_new_intervals()            ◄──────┘  SpeakerInterval -> transcript_processing
//! ```
//!
//! Interface kept from the stub (Member 2's pipeline compiles unchanged):
//!   * `VadEngine::is_speech(&ProcessedAudio) -> AppResult<bool>`
//!   * `DiarizationEngine::assign_speaker(start_ms, end_ms) -> Option<String>`
//!
//! Two things the pipeline needs for real diarization (see README.md):
//!   1. build the pair with `create_engines(..)` so both share state (diarization needs the audio
//!      that only the VAD call sees);
//!   2. forward `DiarizationEngine::drain_new_intervals()` to `transcript_processing`
//!      (`TranscriptInput::Speakers`), because speaker labels become known *after* a speech
//!      segment ends, i.e. usually after Whisper has already produced the text.
//!
//! Time base: every timestamp is session-relative milliseconds counted from the first block ever
//! passed to `is_speech`. Every block must be passed, in order, or the clock drifts.

pub mod config;
pub mod diarization;
pub mod speaker_tracker;
pub mod types;
pub mod vad;

#[cfg(feature = "sherpa")]
pub mod sherpa;
#[cfg(feature = "sherpa")]
pub mod eval;

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use crate::audio_processing::ProcessedAudio;
use crate::core::error::AppResult;

pub use config::{DiarizationConfig, ModelPaths, SpeakerAnalysisConfig, VadConfig};
pub use diarization::SpeakerEmbedder;
pub use types::{SpeakerInterval, SpeechSegment, SAMPLE_RATE};
pub use vad::{EnergyVad, VadBackend};

use diarization::DiarState;
use types::samples_to_ms;

/// After this many consecutive embedding failures diarization is switched off for the session
/// (transcription continues without speaker labels).
const MAX_DIAR_FAILURES: u32 = 5;
/// Bound on undelivered speech segments if nobody drains them.
const MAX_PENDING_SEGMENTS: usize = 1_000;

/// Counters for performance evaluation (CPU cost proxy = real-time factor; RAM via Performance Monitor).
#[derive(Debug, Clone, Default)]
pub struct AnalysisStats {
    pub vad_backend: String,
    pub diarization_backend: Option<String>,
    pub diarization_disabled_after_errors: bool,
    pub audio_ms: u64,
    pub speech_ms: u64,
    pub speech_segments: u64,
    pub speakers: usize,
    pub vad_time_ms: f64,
    pub diarization_time_ms: f64,
    pub diarization_errors: u64,
}

impl AnalysisStats {
    /// Processing time / audio time. < 1.0 means faster than real time.
    pub fn real_time_factor(&self) -> f64 {
        if self.audio_ms == 0 {
            0.0
        } else {
            (self.vad_time_ms + self.diarization_time_ms) / self.audio_ms as f64
        }
    }
}

struct Shared {
    vad: Box<dyn VadBackend>,
    diar: Option<DiarState>,
    consecutive_diar_failures: u32,
    samples_seen: u64,
    pending_segments: Vec<SpeechSegment>,
    stats: AnalysisStats,
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    // A panic elsewhere must not take the audio pipeline down with it.
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    fn handle_chunks(&mut self, chunks: Vec<types::SpeechChunk>) {
        for chunk in chunks {
            self.stats.speech_segments += 1;
            self.stats.speech_ms += chunk.end_ms().saturating_sub(chunk.start_ms());
            if self.pending_segments.len() < MAX_PENDING_SEGMENTS {
                self.pending_segments.push(chunk.as_segment());
            }
            let Some(diar) = self.diar.as_mut() else { continue };
            let t = Instant::now();
            let result = diar.process_chunk(&chunk);
            self.stats.diarization_time_ms += t.elapsed().as_secs_f64() * 1000.0;
            match result {
                Ok(()) => {
                    self.consecutive_diar_failures = 0;
                    self.stats.speakers = diar.speaker_count();
                }
                Err(e) => {
                    self.stats.diarization_errors += 1;
                    self.consecutive_diar_failures += 1;
                    tracing::warn!(error = %e, "speaker diarization failed for a speech segment");
                    if self.consecutive_diar_failures >= MAX_DIAR_FAILURES {
                        tracing::error!("speaker diarization disabled for this session after repeated errors");
                        self.diar = None;
                        self.stats.diarization_disabled_after_errors = true;
                    }
                }
            }
        }
    }

    fn process(&mut self, samples: &[f32]) -> AppResult<bool> {
        let t = Instant::now();
        let out = self.vad.accept(samples);
        self.stats.vad_time_ms += t.elapsed().as_secs_f64() * 1000.0;
        self.samples_seen += samples.len() as u64;
        self.stats.audio_ms = samples_to_ms(self.samples_seen);
        let out = out?;
        self.handle_chunks(out.completed);
        Ok(out.in_speech)
    }

    fn finish(&mut self) -> AppResult<()> {
        let chunks = self.vad.flush()?;
        self.handle_chunks(chunks);
        Ok(())
    }
}

// ── Public engines ───────────────────────────────────────────────────────────

/// Speech / non-speech gate. Cheap to clone; clones share state.
#[derive(Clone)]
pub struct VadEngine {
    shared: Arc<Mutex<Shared>>,
}

/// Speaker labelling. Cheap to clone; clones share state with the `VadEngine` it was created with.
#[derive(Clone)]
pub struct DiarizationEngine {
    shared: Arc<Mutex<Shared>>,
}

impl VadEngine {
    /// Stand-alone VAD (no diarization). Uses Silero when built with `sherpa` and the model
    /// exists, otherwise the energy fallback. Never fails.
    pub fn new() -> Self {
        let cfg = SpeakerAnalysisConfig::default();
        create_engines(&cfg, false)
            .map(|(v, _)| v)
            .unwrap_or_else(|_| Self::with_backend(Box::new(EnergyVad::new(cfg.vad.clone()))))
    }

    pub fn with_backend(backend: Box<dyn VadBackend>) -> Self {
        Self {
            shared: new_shared(backend, None),
        }
    }

    /// True while speech is in progress (including a short hang-over after the last word).
    /// Pass every block, in order. Errors are recoverable; the pipeline passes the block through.
    pub fn is_speech(&self, audio: &ProcessedAudio) -> AppResult<bool> {
        self.process_samples(&audio.samples)
    }

    /// Same as `is_speech` for raw 16 kHz mono samples (used by tests and benchmarks).
    pub fn process_samples(&self, samples: &[f32]) -> AppResult<bool> {
        lock(&self.shared).process(samples)
    }

    /// Call at the end of a session so a speech region still open is closed and labelled.
    pub fn finish(&self) -> AppResult<()> {
        lock(&self.shared).finish()
    }

    /// Speech regions completed since the last call (session-relative timestamps).
    pub fn take_speech_segments(&self) -> Vec<SpeechSegment> {
        std::mem::take(&mut lock(&self.shared).pending_segments)
    }

    pub fn stats(&self) -> AnalysisStats {
        lock(&self.shared).stats.clone()
    }
}

impl Default for VadEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DiarizationEngine {
    /// Stand-alone engine with no audio source: always answers `None`. Real diarization needs
    /// the pair from `create_engines`.
    pub fn new() -> Self {
        let cfg = SpeakerAnalysisConfig::default();
        Self {
            shared: new_shared(Box::new(EnergyVad::new(cfg.vad)), None),
        }
    }

    /// Generic speaker label for the time range, if known yet. `None` means "not known (yet)";
    /// the label may still arrive later through `drain_new_intervals`.
    pub fn assign_speaker(&self, start_ms: u64, end_ms: u64) -> Option<String> {
        lock(&self.shared).diar.as_ref()?.speaker_for(start_ms, end_ms)
    }

    /// Speaker intervals produced since the last call, for `transcript_processing`.
    pub fn drain_new_intervals(&self) -> Vec<SpeakerInterval> {
        match lock(&self.shared).diar.as_mut() {
            Some(d) => d.drain_new_intervals(),
            None => Vec::new(),
        }
    }

    /// All retained intervals (bounded), oldest first.
    pub fn intervals(&self) -> Vec<SpeakerInterval> {
        lock(&self.shared).diar.as_ref().map(|d| d.intervals()).unwrap_or_default()
    }

    pub fn speaker_count(&self) -> usize {
        lock(&self.shared).diar.as_ref().map_or(0, |d| d.speaker_count())
    }

    /// False when diarization is off, failed to initialise, or was disabled after repeated errors.
    pub fn is_active(&self) -> bool {
        lock(&self.shared).diar.is_some()
    }

    pub fn stats(&self) -> AnalysisStats {
        lock(&self.shared).stats.clone()
    }
}

impl Default for DiarizationEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn new_shared(vad: Box<dyn VadBackend>, diar: Option<DiarState>) -> Arc<Mutex<Shared>> {
    let stats = AnalysisStats {
        vad_backend: vad.name().to_string(),
        diarization_backend: diar.as_ref().map(|d| d.embedder_name().to_string()),
        ..Default::default()
    };
    Arc::new(Mutex::new(Shared {
        vad,
        diar,
        consecutive_diar_failures: 0,
        samples_seen: 0,
        pending_segments: Vec::new(),
        stats,
    }))
}

/// Build the VAD and diarization engines for a session. Both share one state.
///
/// `enable_diarization` should come from `SessionConfig::enable_diarization`.
/// With `config.fallback_on_error` (default) model problems never fail the session: a missing
/// Silero model falls back to the energy VAD, a missing speaker model turns labels off, and a
/// warning is logged. With it off, the first initialisation error is returned.
pub fn create_engines(
    config: &SpeakerAnalysisConfig,
    enable_diarization: bool,
) -> AppResult<(VadEngine, DiarizationEngine)> {
    let vad: Box<dyn VadBackend> = match build_vad(config) {
        Ok(v) => v,
        Err(e) if config.fallback_on_error => {
            tracing::warn!(error = %e, "Silero VAD unavailable, using energy-based VAD fallback");
            Box::new(EnergyVad::new(config.vad.clone()))
        }
        Err(e) => return Err(e),
    };

    let diar = if enable_diarization {
        match build_embedder(config) {
            Ok(embedder) => Some(DiarState::new(config.diarization.clone(), embedder)),
            Err(e) if config.fallback_on_error => {
                tracing::warn!(error = %e, "speaker diarization unavailable, continuing without speaker labels");
                None
            }
            Err(e) => return Err(e),
        }
    } else {
        None
    };

    tracing::info!(
        vad = vad.name(),
        diarization = diar.is_some(),
        "speaker_analysis initialised"
    );
    let shared = new_shared(vad, diar);
    Ok((VadEngine { shared: shared.clone() }, DiarizationEngine { shared }))
}

/// Build engines around caller-supplied backends (tests, benchmarks, alternative models).
pub fn create_engines_with(
    vad: Box<dyn VadBackend>,
    embedder: Option<Box<dyn SpeakerEmbedder>>,
    diarization: DiarizationConfig,
) -> (VadEngine, DiarizationEngine) {
    let shared = new_shared(vad, embedder.map(|e| DiarState::new(diarization, e)));
    (VadEngine { shared: shared.clone() }, DiarizationEngine { shared })
}

#[cfg(feature = "sherpa")]
fn build_vad(config: &SpeakerAnalysisConfig) -> AppResult<Box<dyn VadBackend>> {
    Ok(Box::new(sherpa::SherpaSileroVad::new(&config.models.vad_path(), &config.vad)?))
}

#[cfg(not(feature = "sherpa"))]
fn build_vad(_config: &SpeakerAnalysisConfig) -> AppResult<Box<dyn VadBackend>> {
    Err(crate::core::error::AppError::Internal("built without the `sherpa` feature: Silero VAD unavailable".into()))
}

#[cfg(feature = "sherpa")]
fn build_embedder(config: &SpeakerAnalysisConfig) -> AppResult<Box<dyn SpeakerEmbedder>> {
    Ok(Box::new(sherpa::SherpaEmbedder::new(&config.models.embedding_path())?))
}

#[cfg(not(feature = "sherpa"))]
fn build_embedder(_config: &SpeakerAnalysisConfig) -> AppResult<Box<dyn SpeakerEmbedder>> {
    Err(crate::core::error::AppError::Internal("built without the `sherpa` feature: speaker embeddings unavailable".into()))
}

#[cfg(test)]
mod tests;
