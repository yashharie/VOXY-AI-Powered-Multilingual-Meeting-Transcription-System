//! Shared application data structures.
//!
//! All types that cross the IPC boundary (Tauri commands / events) derive
//! Serialize + Deserialize so they can be serialised to JSON automatically.
//! TypeScript counterparts live in frontend/src/lib/types.ts — keep in sync.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Session ─────────────────────────────────────────────────────────────────

/// All lifecycle states a transcription session can occupy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SessionState {
    /// No session exists yet.
    Idle,
    /// Session object created; modules not yet running.
    Initializing,
    /// Audio capture and transcription are running.
    Active,
    /// Capture halted; session data preserved.
    Paused,
    /// Teardown in progress.
    Stopping,
    /// Session has ended cleanly.
    Stopped,
    /// An unrecoverable error occurred.
    Error,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Idle => "Idle",
            Self::Initializing => "Initializing",
            Self::Active => "Active",
            Self::Paused => "Paused",
            Self::Stopping => "Stopping",
            Self::Stopped => "Stopped",
            Self::Error => "Error",
        };
        write!(f, "{s}")
    }
}

/// Runtime information about a session, returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub state: SessionState,
    /// Unix timestamp (seconds) when the session was created.
    pub created_at: i64,
    /// Unix timestamp (seconds) when the session became Active.
    pub started_at: Option<i64>,
    /// Unix timestamp (seconds) when the session ended.
    pub ended_at: Option<i64>,
    /// Human-readable display name, e.g. "Session 2026-11-12 14:30".
    pub display_name: String,
}

impl SessionInfo {
    pub fn new(_config: &SessionConfig) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        let display_name = format!("Session {}", now.format("%Y-%m-%d %H:%M"));
        Self {
            session_id: id,
            state: SessionState::Initializing,
            created_at: now.timestamp(),
            started_at: None,
            ended_at: None,
            display_name,
        }
    }
}

/// User-supplied configuration for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    #[serde(default)]
    pub device_id: Option<String>,
    pub audio_source: AudioSource,
    /// Language hint for Whisper, e.g. "si", "ta", "en", "auto".
    pub language_hint: String,
    pub enable_diarization: bool,
    pub show_partial_results: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            audio_source: AudioSource::Loopback,
            language_hint: "auto".to_string(),
            enable_diarization: false,
            show_partial_results: true,
        }
    }
}

/// Which audio input source(s) to capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioSource {
    /// WASAPI loopback — captures system output audio.
    Loopback,
    /// Default microphone input.
    Microphone,
    /// Both loopback and microphone mixed.
    Both,
}

// ─── Transcript ───────────────────────────────────────────────────────────────

pub use crate::transcript_processing::types::TranscriptSegment;

// ─── Application metadata ─────────────────────────────────────────────────────

/// Version information returned by the get_app_version command.
#[derive(Debug, Serialize, Deserialize)]
pub struct AppVersion {
    pub version: String,
    pub tauri_version: String,
    pub build_target: String,
}
