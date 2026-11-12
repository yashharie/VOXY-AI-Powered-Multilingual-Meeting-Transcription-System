//! Application-level configuration loading and persistence.
//!
//! Config is stored as JSON in the OS app-data directory and loaded on startup.
//! Members should not modify this file; request changes via Member 2.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::error::{AppError, AppResult};

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Application schema version — bump when the structure changes.
    pub version: u32,
    pub logging: LoggingConfig,
    pub session: SessionDefaults,
    pub models: ModelConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            logging: LoggingConfig::default(),
            session: SessionDefaults::default(),
            models: ModelConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Tracing filter directive, e.g. "voxy=debug,info".
    pub level: String,
    /// Whether to write a rotating log file.
    pub file_enabled: bool,
    /// Directory where log files are written.
    pub log_dir: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            file_enabled: true,
            log_dir: None, // resolved at runtime to %APPDATA%\VOXY\logs
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDefaults {
    pub language_hint: String,
    pub enable_diarization: bool,
    pub show_partial_results: bool,
}

impl Default for SessionDefaults {
    fn default() -> Self {
        Self {
            language_hint: "auto".to_string(),
            enable_diarization: false,
            show_partial_results: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Path to the whisper.cpp model file (.bin).
    pub whisper_model_path: Option<PathBuf>,
    /// Path to the Silero VAD ONNX model.
    pub silero_model_path: Option<PathBuf>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            whisper_model_path: None,
            silero_model_path: None,
        }
    }
}

// ─── Load / save ─────────────────────────────────────────────────────────────

/// Returns the path to the config file, creating parent directories if needed.
pub fn config_path() -> AppResult<PathBuf> {
    let base = dirs_config_dir().ok_or_else(|| {
        AppError::IoError("Cannot determine app config directory".to_string())
    })?;
    let dir = base.join("VOXY");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

/// Loads config from disk, returning defaults if the file does not exist.
pub fn load_config() -> AppResult<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        tracing::info!("No config file found at {path:?}, using defaults");
        return Ok(AppConfig::default());
    }

    let raw = std::fs::read_to_string(&path)?;
    let cfg: AppConfig = serde_json::from_str(&raw)?;
    tracing::info!("Config loaded from {path:?}");
    Ok(cfg)
}

/// Persists the current config to disk.
pub fn save_config(cfg: &AppConfig) -> AppResult<()> {
    let path = config_path()?;
    let json = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&path, json)?;
    tracing::info!("Config saved to {path:?}");
    Ok(())
}

// Thin platform helper — avoids pulling in `dirs` crate just for one call.
fn dirs_config_dir() -> Option<PathBuf> {
    // Windows: %APPDATA%
    std::env::var("APPDATA").ok().map(PathBuf::from)
}
