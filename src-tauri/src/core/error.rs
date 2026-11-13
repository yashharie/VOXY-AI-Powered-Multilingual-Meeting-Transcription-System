//! Centralised error handling for the VOXY application.
//!
//! `AppError` is the single error type used throughout the Rust backend.
//! It implements `Serialize` so it can be returned directly from Tauri
//! commands as a structured JSON error payload.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Top-level application error type.
#[derive(Debug, Error, Serialize, Deserialize, Clone)]
#[serde(tag = "code", content = "message")]
pub enum AppError {
    // ── Session errors ───────────────────────────────────────────────────────
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("A session is already active: {0}")]
    SessionAlreadyActive(String),

    #[error("Session configuration is invalid: {0}")]
    InvalidConfig(String),

    // ── Audio errors ─────────────────────────────────────────────────────────
    #[error("Audio device not available: {0}")]
    AudioDeviceUnavailable(String),

    #[error("Audio capture failed: {0}")]
    AudioCaptureFailed(String),

    // ── Transcription errors ─────────────────────────────────────────────────
    #[error("Speech recognition error: {0}")]
    SpeechRecognitionError(String),

    #[error("Model not found at path: {0}")]
    ModelNotFound(String),

    // ── IO / file errors ─────────────────────────────────────────────────────
    #[error("File operation failed: {0}")]
    IoError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    // ── Generic ──────────────────────────────────────────────────────────────
    #[error("Internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// A short machine-readable error code string, mirroring the enum variant.
    pub fn code(&self) -> &'static str {
        match self {
            Self::SessionNotFound(_) => "SESSION_NOT_FOUND",
            Self::InvalidStateTransition { .. } => "INVALID_STATE_TRANSITION",
            Self::SessionAlreadyActive(_) => "SESSION_ALREADY_ACTIVE",
            Self::InvalidConfig(_) => "INVALID_CONFIG",
            Self::AudioDeviceUnavailable(_) => "AUDIO_DEVICE_UNAVAILABLE",
            Self::AudioCaptureFailed(_) => "AUDIO_CAPTURE_FAILED",
            Self::SpeechRecognitionError(_) => "SPEECH_RECOGNITION_ERROR",
            Self::ModelNotFound(_) => "MODEL_NOT_FOUND",
            Self::IoError(_) => "IO_ERROR",
            Self::SerializationError(_) => "SERIALIZATION_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    /// Whether the error allows the session to continue (true) or not (false).
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::AudioDeviceUnavailable(_) | Self::SpeechRecognitionError(_)
        )
    }
}

// Allow converting std::io::Error into AppError without boilerplate.
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerializationError(e.to_string())
    }
}

/// Convenience alias used throughout the codebase.
pub type AppResult<T> = Result<T, AppError>;

/// IPC-safe result wrapper.
///
/// Tauri commands must return `Result<T, String>` or a serializable type.
/// We use this wrapper so the frontend receives a structured
/// `{ Ok: T } | { Err: { code, message } }` envelope.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IpcResult<T> {
    Ok(T),
    Err(IpcError),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl<T: Serialize> From<AppResult<T>> for IpcResult<T> {
    fn from(result: AppResult<T>) -> Self {
        match result {
            Ok(v) => IpcResult::Ok(v),
            Err(e) => IpcResult::Err(IpcError {
                code: e.code().to_string(),
                message: e.to_string(),
            }),
        }
    }
}
