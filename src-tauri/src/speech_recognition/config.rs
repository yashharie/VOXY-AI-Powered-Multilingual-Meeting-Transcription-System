//! Whisper model configuration and language settings.
//!
//! This module defines the configuration types used to load and run
//! Whisper models through whisper-rs.  It covers language hints,
//! thread counts, and model file resolution.

use std::path::{Path, PathBuf};

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
