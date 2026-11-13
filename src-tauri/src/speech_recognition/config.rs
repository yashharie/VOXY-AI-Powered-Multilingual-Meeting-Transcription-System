//! Whisper model configuration and language settings.
//!
//! This module defines the configuration types used to load and run
//! Whisper models through whisper-rs.  It covers language hints,
//! thread counts, and model file resolution.

use std::path::{Path, PathBuf};

// whisper-rs language utilities (get_lang_str, get_lang_id, get_lang_str_full).
use whisper_rs;

// ─── Language support ─────────────────────────────────────────────────────────

/// Languages supported by VOXY for transcription.
///
/// Whisper auto-detection is used when [`WhisperLanguage::Auto`] is selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhisperLanguage {
    /// Auto-detect the spoken language from the audio.
    Auto,
    /// Sinhala (ISO 639-1: `si`).
    Sinhala,
    /// Tamil (ISO 639-1: `ta`).
    Tamil,
    /// English (ISO 639-1: `en`).
    English,
}

impl WhisperLanguage {
    /// Convert to the short code Whisper expects, or `None` for auto-detect.
    pub fn code(&self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Sinhala => Some("si"),
            Self::Tamil => Some("ta"),
            Self::English => Some("en"),
        }
    }

    /// Parse a language hint string received from the frontend / config.
    ///
    /// Accepts `"auto"`, `"si"`, `"ta"`, `"en"` (case-insensitive).
    /// Falls back to [`WhisperLanguage::Auto`] for unknown values.
    pub fn from_hint(hint: &str) -> Self {
        match hint.to_lowercase().as_str() {
            "si" | "sin" | "sinhala" => Self::Sinhala,
            "ta" | "tam" | "tamil" => Self::Tamil,
            "en" | "eng" | "english" => Self::English,
            _ => Self::Auto,
        }
    }
}

impl Default for WhisperLanguage {
    fn default() -> Self {
        Self::Auto
    }
}

impl std::fmt::Display for WhisperLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Sinhala => write!(f, "si"),
            Self::Tamil => write!(f, "ta"),
            Self::English => write!(f, "en"),
        }
    }
}

// ─── Model configuration ───────────────────────────────────────────────────────

/// Configuration for loading and running a Whisper model.
///
/// Created by [`WhisperEngine::new`](super::WhisperEngine::new) and stored
/// inside the engine for the lifetime of the session.
#[derive(Debug, Clone)]
pub struct WhisperModelConfig {
    /// Filesystem path to the `ggml-*.bin` model file.
    pub model_path: PathBuf,
    /// Language hint for transcription.
    pub language: WhisperLanguage,
    /// Number of CPU threads for inference (0 = use Whisper default).
    pub n_threads: u32,
    /// When `true`, Whisper translates non-English speech to English.
    /// VOXY always sets this to `false` to preserve original scripts.
    pub translate: bool,
    /// Suppress Whisper's progress / realtime stderr output.
    pub suppress_output: bool,
}

impl WhisperModelConfig {
    /// Create a config from just a model path, using sensible defaults.
    pub fn new(model_path: impl AsRef<Path>) -> Self {
        Self {
            model_path: model_path.as_ref().to_path_buf(),
            language: WhisperLanguage::Auto,
            n_threads: 0,
            translate: false,
            suppress_output: true,
        }
    }

    /// Set the language hint.
    pub fn with_language(mut self, language: WhisperLanguage) -> Self {
        self.language = language;
        self
    }

    /// Set the number of inference threads.
    pub fn with_threads(mut self, n_threads: u32) -> Self {
        self.n_threads = n_threads;
        self
    }

    /// Returns `true` when the model path points to an existing file.
    pub fn model_exists(&self) -> bool {
        self.model_path.is_file()
    }
}

impl Default for WhisperModelConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            language: WhisperLanguage::Auto,
            n_threads: 0,
            translate: false,
            suppress_output: true,
        }
    }
}

// ─── Multilingual model configuration ──────────────────────────────────────────

/// Result of Whisper language auto-detection.
///
/// Returned after inference when `language_hint` was set to `Auto`.
#[derive(Debug, Clone)]
pub struct LanguageDetectionResult {
    /// ISO 639-1 code detected by Whisper (e.g. `"si"`, `"ta"`, `"en"`).
    pub language_code: String,
    /// Full language name (e.g. `"sinhala"`, `"tamil"`, `"english"`).
    pub language_name: String,
    /// Confidence score from Whisper's language detection (0.0 – 1.0).
    pub confidence: f32,
}

/// Configuration for multilingual model behaviour.
///
/// Controls how the engine handles language detection, fallback, and
/// validation when a multilingual Whisper model is loaded.
#[derive(Debug, Clone)]
pub struct MultilingualConfig {
    /// Whether to use Whisper's built-in auto-detection.
    pub auto_detect: bool,
    /// Fallback language when auto-detection fails or confidence is low.
    pub fallback_language: WhisperLanguage,
    /// Minimum confidence threshold for accepting a detected language.
    /// Below this, the fallback language is used.
    pub min_confidence: f32,
    /// When `true`, the detected language is logged for each transcription.
    pub log_detected_language: bool,
}

impl Default for MultilingualConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            fallback_language: WhisperLanguage::English,
            min_confidence: 0.5,
            log_detected_language: true,
        }
    }
}

impl MultilingualConfig {
    /// Create a config that forces a specific language (no auto-detection).
    pub fn fixed(language: WhisperLanguage) -> Self {
        Self {
            auto_detect: false,
            fallback_language: language,
            min_confidence: 0.0,
            log_detected_language: false,
        }
    }

    /// Create a config with auto-detection and custom fallback.
    pub fn auto_with_fallback(fallback: WhisperLanguage) -> Self {
        Self {
            auto_detect: true,
            fallback_language: fallback,
            min_confidence: 0.5,
            log_detected_language: true,
        }
    }
}

// ─── Language validation ───────────────────────────────────────────────────────

/// Validate that a language is compatible with the loaded model.
///
/// Monolingual (English-only) Whisper models can only transcribe English.
/// Multilingual models support all languages including Sinhala and Tamil.
///
/// Returns `Ok(())` if compatible, or an error message describing the
/// mismatch.
pub fn validate_language_for_model(
    language: &WhisperLanguage,
    is_multilingual: bool,
) -> Result<(), String> {
    if !is_multilingual {
        match language {
            WhisperLanguage::English | WhisperLanguage::Auto => Ok(()),
            WhisperLanguage::Sinhala => Err(
                "Sinhala requires a multilingual Whisper model. \
                 The loaded model is English-only."
                    .to_string(),
            ),
            WhisperLanguage::Tamil => Err(
                "Tamil requires a multilingual Whisper model. \
                 The loaded model is English-only."
                    .to_string(),
            ),
        }
    } else {
        Ok(())
    }
}

/// Map a Whisper language ID (integer) to a [`WhisperLanguage`].
///
/// Uses whisper-rs `get_lang_str` to convert the ID to a code string,
/// then maps it to the VOXY enum.  Unknown languages fall back to
/// [`WhisperLanguage::English`].
pub fn lang_id_to_enum(lang_id: i32) -> WhisperLanguage {
    let code = whisper_rs::get_lang_str(lang_id).unwrap_or("en");
    WhisperLanguage::from_hint(code)
}

/// Get the full language name from a language code string.
///
/// Returns the ISO 639-1 code if the full name is not available.
pub fn lang_full_name(code: &str) -> String {
    let id = whisper_rs::get_lang_id(code);
    if id >= 0 {
        whisper_rs::get_lang_str_full(id)
            .unwrap_or(code.to_string())
    } else {
        code.to_string()
    }
}

// ─── Model size and quantization ───────────────────────────────────────────────

/// Whisper model sizes available in whisper.cpp.
///
/// Each variant maps to a specific ggml model file.  Smaller models are
/// faster but less accurate; larger models are more accurate but slower.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSize {
    /// Tiny model — fastest, least accurate (~39 M params).
    Tiny,
    /// Base model — good speed/accuracy balance (~74 M params).
    Base,
    /// Small model — better accuracy, slower (~244 M params).
    Small,
    /// Medium model — high accuracy, slow (~769 M params).
    Medium,
    /// Large-v3 model — highest accuracy, very slow (~1550 M params).
    LargeV3,
}

impl ModelSize {
    /// The short identifier used in ggml file names, e.g. `"base"`.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::LargeV3 => "large-v3",
        }
    }

    /// Approximate parameter count in millions.
    pub fn params_m(&self) -> u32 {
        match self {
            Self::Tiny => 39,
            Self::Base => 74,
            Self::Small => 244,
            Self::Medium => 769,
            Self::LargeV3 => 1550,
        }
    }

    /// Whether this model size is available in multilingual form.
    ///
    /// Tiny, Base, Small, and Medium have both English-only and multilingual
    /// variants.  Large-v3 is multilingual only.
    pub fn has_multilingual(&self) -> bool {
        matches!(self, Self::Tiny | Self::Base | Self::Small | Self::Medium | Self::LargeV3)
    }

    /// Recommended model sizes for VOXY, ordered by preference.
    ///
    /// Base and Small offer the best speed/accuracy trade-off on
    /// CPU-only Windows hardware.
    pub fn recommended() -> &'static [ModelSize] {
        &[Self::Base, Self::Small]
    }
}

impl std::fmt::Display for ModelSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}

/// Quantization type for ggml model files.
///
/// Quantized models use less memory and run faster at a small accuracy cost.
/// The naming follows whisper.cpp conventions (e.g. `ggml-base-q5_0.bin`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuantizationType {
    /// No quantization — F16 weights (`ggml-*.bin`).
    F16,
    /// 4-bit quantization, variant 0 (`ggml-*-q4_0.bin`).
    Q4_0,
    /// 4-bit quantization, variant 1 (`ggml-*-q4_1.bin`).
    Q4_1,
    /// 5-bit quantization, variant 0 (`ggml-*-q5_0.bin`).
    Q5_0,
    /// 5-bit quantization, variant 1 (`ggml-*-q5_1.bin`).
    Q5_1,
    /// 8-bit quantization (`ggml-*-q8_0.bin`).
    Q8_0,
}

impl QuantizationType {
    /// The suffix appended to the model file name, or empty for F16.
    ///
    /// Examples: `"-q5_0"`, `"-q8_0"`, `""` (F16).
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::F16 => "",
            Self::Q4_0 => "-q4_0",
            Self::Q4_1 => "-q4_1",
            Self::Q5_0 => "-q5_0",
            Self::Q5_1 => "-q5_1",
            Self::Q8_0 => "-q8_0",
        }
    }

    /// Approximate memory reduction factor relative to F16.
    ///
    /// E.g. Q5_0 uses roughly half the memory of F16.
    pub fn memory_factor(&self) -> f32 {
        match self {
            Self::F16 => 1.0,
            Self::Q4_0 | Self::Q4_1 => 0.25,
            Self::Q5_0 | Self::Q5_1 => 0.31,
            Self::Q8_0 => 0.5,
        }
    }
}

impl Default for QuantizationType {
    fn default() -> Self {
        // Q5_0 offers the best accuracy/memory trade-off for VOXY.
        Self::Q5_0
    }
}

impl std::fmt::Display for QuantizationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::F16 => write!(f, "f16"),
            _ => write!(f, "{}", &self.suffix()[1..]), // strip leading '-'
        }
    }
}

// ─── Model descriptor and path resolution ─────────────────────────────────────

/// A complete description of a Whisper model file.
///
/// Combines model size, quantization, and language capability to produce
/// the expected ggml file name.
#[derive(Debug, Clone)]
pub struct ModelDescriptor {
    /// Model size (Tiny, Base, Small, Medium, LargeV3).
    pub size: ModelSize,
    /// Quantization type.
    pub quantization: QuantizationType,
    /// Whether this is the multilingual variant.
    pub multilingual: bool,
}

impl ModelDescriptor {
    /// Construct the ggml model file name.
    ///
    /// Examples:
    /// - `ModelDescriptor { size: Base, quantization: F16, multilingual: true }`
    ///   → `"ggml-base.bin"`
    /// - `ModelDescriptor { size: Small, quantization: Q5_0, multilingual: true }`
    ///   → `"ggml-small-q5_0.bin"`
    /// - `ModelDescriptor { size: Base, quantization: F16, multilingual: false }`
    ///   → `"ggml-base.en.bin"`
    pub fn file_name(&self) -> String {
        let en_suffix = if self.multilingual { "" } else { ".en" };
        format!(
            "ggml-{}{}{}.bin",
            self.size.id(),
            en_suffix,
            self.quantization.suffix(),
        )
    }

    /// Resolve the full path to the model file in the given directory.
    pub fn resolve_path(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.file_name())
    }
}

impl Default for ModelDescriptor {
    fn default() -> Self {
        Self {
            size: ModelSize::Base,
            quantization: QuantizationType::Q5_0,
            multilingual: true,
        }
    }
}

// ─── Model evaluation configuration ───────────────────────────────────────────

/// Configuration for evaluating different Whisper model configurations.
///
/// Used to benchmark model sizes and quantization levels to determine
/// the best trade-off for VOXY on the target hardware.
#[derive(Debug, Clone)]
pub struct ModelEvaluationConfig {
    /// Model sizes to evaluate.
    pub sizes: Vec<ModelSize>,
    /// Quantization types to evaluate for each size.
    pub quantizations: Vec<QuantizationType>,
    /// Directory containing model files.
    pub models_dir: PathBuf,
    /// Maximum audio duration (seconds) to use for evaluation samples.
    pub max_audio_duration_s: u32,
    /// Number of warm-up inference calls before measuring.
    pub warmup_iterations: u32,
    /// Number of timed inference calls for averaging.
    pub benchmark_iterations: u32,
}

impl Default for ModelEvaluationConfig {
    fn default() -> Self {
        Self {
            sizes: ModelSize::recommended().to_vec(),
            quantizations: vec![
                QuantizationType::F16,
                QuantizationType::Q5_0,
                QuantizationType::Q5_1,
                QuantizationType::Q8_0,
            ],
            models_dir: PathBuf::from("models"),
            max_audio_duration_s: 30,
            warmup_iterations: 1,
            benchmark_iterations: 3,
        }
    }
}

impl ModelEvaluationConfig {
    /// Generate all model descriptors to evaluate.
    ///
    /// Only returns descriptors whose model file exists in `models_dir`.
    pub fn available_models(&self) -> Vec<ModelDescriptor> {
        let mut descriptors = Vec::new();
        for size in &self.sizes {
            for quant in &self.quantizations {
                let descriptor = ModelDescriptor {
                    size: size.clone(),
                    quantization: quant.clone(),
                    multilingual: true,
                };
                if descriptor.resolve_path(&self.models_dir).is_file() {
                    descriptors.push(descriptor);
                }
            }
        }
        descriptors
    }
}
