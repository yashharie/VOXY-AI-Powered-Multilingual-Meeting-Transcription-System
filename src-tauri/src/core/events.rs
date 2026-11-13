//! Tauri event channel definitions and emission helpers.
//!
//! All event names are defined as constants here so both Rust and
//! TypeScript (frontend/src/lib/events.ts) can reference the same strings.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::error::AppError;
use super::types::{SessionState, TranscriptSegment};
use crate::transcript_processing::{TranscriptUpdate, TRANSCRIPT_UPDATE_EVENT};

/// Canonical session-scoped updates, including partial/final and late labels.
pub fn emit_transcript_update(app: &AppHandle, update: TranscriptUpdate) {
    if let Err(error) = app.emit(TRANSCRIPT_UPDATE_EVENT, update) {
        tracing::warn!(%error, "Failed to emit transcript update");
    }
}

pub fn emit_transcript_deleted(
    app: &AppHandle,
    deletion: crate::transcript_management::models::TranscriptDeletion,
) {
    if let Err(error) = app.emit("transcript:deleted", deletion) {
        tracing::warn!(%error, "Failed to emit transcript deletion");
    }
}

// ─── Event channel names ──────────────────────────────────────────────────────

pub const EVT_SESSION_STATE_CHANGED: &str = "voxy://session-state-changed";
pub const EVT_TRANSCRIPT_SEGMENT: &str = "voxy://transcript-segment";
pub const EVT_ERROR: &str = "voxy://error";
pub const EVT_AUDIO_LEVEL: &str = "voxy://audio-level";
pub const EVT_SESSION_CLEANUP_DONE: &str = "voxy://session-cleanup-done";

// ─── Event payloads ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct SessionStateChangedPayload {
    pub session_id: String,
    pub previous_state: SessionState,
    pub new_state: SessionState,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct TranscriptSegmentPayload {
    pub session_id: String,
    pub segment: TranscriptSegment,
}

#[derive(Debug, Serialize, Clone)]
pub struct ErrorEventPayload {
    pub session_id: Option<String>,
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct AudioLevelPayload {
    pub session_id: String,
    /// RMS level in range 0.0 – 1.0.
    pub level: f32,
}

#[derive(Debug, Serialize, Clone)]
pub struct SessionCleanupDonePayload {
    pub session_id: String,
}

// ─── Emission helpers ─────────────────────────────────────────────────────────

/// Emit a session state change to all frontend windows.
pub fn emit_session_state_changed(
    app: &AppHandle,
    session_id: &str,
    previous: SessionState,
    new: SessionState,
    reason: Option<String>,
) {
    let payload = SessionStateChangedPayload {
        session_id: session_id.to_string(),
        previous_state: previous,
        new_state: new,
        reason,
    };
    if let Err(e) = app.emit(EVT_SESSION_STATE_CHANGED, payload) {
        tracing::warn!(error = %e, "Failed to emit session state changed event");
    }
}

/// Emit a transcript segment (partial or final).
pub fn emit_transcript_segment(app: &AppHandle, session_id: &str, segment: TranscriptSegment) {
    let payload = TranscriptSegmentPayload {
        session_id: session_id.to_string(),
        segment,
    };
    if let Err(e) = app.emit(EVT_TRANSCRIPT_SEGMENT, payload) {
        tracing::warn!(error = %e, "Failed to emit transcript segment event");
    }
}

/// Emit an application error event.
pub fn emit_error(app: &AppHandle, session_id: Option<&str>, error: &AppError) {
    let payload = ErrorEventPayload {
        session_id: session_id.map(str::to_string),
        code: error.code().to_string(),
        message: error.to_string(),
        recoverable: error.is_recoverable(),
    };
    if let Err(e) = app.emit(EVT_ERROR, payload) {
        tracing::warn!(error = %e, "Failed to emit error event");
    }
}

/// Emit an audio level update for the VU meter.
pub fn emit_audio_level(app: &AppHandle, session_id: &str, level: f32) {
    let payload = AudioLevelPayload {
        session_id: session_id.to_string(),
        level,
    };
    if let Err(e) = app.emit(EVT_AUDIO_LEVEL, payload) {
        tracing::warn!(error = %e, "Failed to emit audio level event");
    }
}

/// Emit when session cleanup has finished and the UI can reset.
pub fn emit_session_cleanup_done(app: &AppHandle, session_id: &str) {
    let payload = SessionCleanupDonePayload {
        session_id: session_id.to_string(),
    };
    if let Err(e) = app.emit(EVT_SESSION_CLEANUP_DONE, payload) {
        tracing::warn!(error = %e, "Failed to emit session cleanup done event");
    }
}
