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
