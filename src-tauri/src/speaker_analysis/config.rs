//! Configuration for VAD and diarization. All values are tunable; the defaults are
//! starting points that still have to be validated on real Tamil/Sinhala/English audio.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct VadConfig {
    /// Silero speech-probability threshold (0..1). Ignored by the energy fallback.
    pub threshold: f32,
    /// Speech shorter than this is dropped as noise.
    pub min_speech_ms: u64,
    /// Silence this long closes a speech segment (also the "hang-over" so word tails are kept).
    pub min_silence_ms: u64,
    /// Segments are force-split at this length so memory and latency stay bounded.
    pub max_speech_ms: u64,
    /// Energy fallback: absolute RMS floor below which audio is never speech.
    pub energy_min_rms: f32,
    /// Energy fallback: speech if RMS > noise_floor * factor.
    pub energy_noise_factor: f32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            min_speech_ms: 250,
            min_silence_ms: 500,
            max_speech_ms: 30_000,
            energy_min_rms: 0.01,
            energy_noise_factor: 3.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiarizationConfig {
    /// Cosine similarity needed to join an existing speaker; below it a new speaker is created.
    pub similarity_threshold: f32,
    /// Keep the previous speaker when it scores within this margin of the best one
    /// (reduces label flip-flopping on ambiguous windows).
    pub stickiness: f32,
    /// Hard cap on distinct speakers; beyond it windows go to the closest existing speaker.
    pub max_speakers: usize,
    /// Target length of one embedding window inside a speech segment.
    pub window_ms: u64,
    /// Windows shorter than this give unreliable embeddings and are not labelled.
    pub min_window_ms: u64,
    /// Max speaker intervals kept for `assign_speaker` lookups.
    pub max_intervals: usize,
    /// A Whisper segment with no overlapping interval borrows the nearest one within this distance.
    pub lookup_tolerance_ms: u64,
}

impl Default for DiarizationConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.5,
            stickiness: 0.05,
            max_speakers: 8,
            window_ms: 2_000,
            min_window_ms: 800,
            max_intervals: 10_000,
            lookup_tolerance_ms: 1_500,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub models_dir: PathBuf,
    /// Silero VAD ONNX file.
    pub vad_model: String,
    /// Speaker-embedding ONNX file (live diarization).
    pub embedding_model: String,
    /// pyannote segmentation ONNX file (offline / post-meeting diarization only).
    pub segmentation_model: String,
}

impl Default for ModelPaths {
    fn default() -> Self {
        // Same convention as the Whisper model in the pipeline: <cwd>/models.
        let dir = std::env::var_os("VOXY_MODELS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("models"));
        Self {
            models_dir: dir,
            vad_model: "silero_vad.onnx".into(),
            embedding_model: "3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx".into(),
            segmentation_model: "sherpa-onnx-pyannote-segmentation-3-0/model.onnx".into(),
        }
    }
}

impl ModelPaths {
    pub fn vad_path(&self) -> PathBuf {
        self.models_dir.join(&self.vad_model)
    }
    pub fn embedding_path(&self) -> PathBuf {
        self.models_dir.join(&self.embedding_model)
    }
    pub fn segmentation_path(&self) -> PathBuf {
        self.models_dir.join(&self.segmentation_model)
    }
}

#[derive(Debug, Clone)]
pub struct SpeakerAnalysisConfig {
    pub vad: VadConfig,
    pub diarization: DiarizationConfig,
    pub models: ModelPaths,
    /// true  (default): a missing/broken model degrades gracefully (energy VAD, no speaker labels)
    ///                   and the transcription pipeline keeps running.
    /// false: initialisation returns the error instead (useful for tests / diagnostics).
    pub fallback_on_error: bool,
}

impl Default for SpeakerAnalysisConfig {
    fn default() -> Self {
        Self {
            vad: VadConfig::default(),
            diarization: DiarizationConfig::default(),
            models: ModelPaths::default(),
            fallback_on_error: true,
        }
    }
}
