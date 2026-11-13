//! Transcription session lifecycle and state machine.
//!
//! A `Session` owns all runtime data for a single recording session.
//! State transitions are strictly validated — invalid transitions return
//! `AppError::InvalidStateTransition` without mutating state.

use chrono::Utc;
use parking_lot::RwLock;
use std::sync::Arc;

use super::error::{AppError, AppResult};
use super::types::{SessionConfig, SessionInfo, SessionState, TranscriptSegment};
use crate::transcript_management::{
    manager::{search_segments, TranscriptManager},
    models::TranscriptDeletion,
};
use crate::transcript_processing::{
    RecognizedChunk, SpeakerInterval, TranscriptProcessor, TranscriptUpdate,
};

// ─── Valid state transitions ──────────────────────────────────────────────────
//
//   Idle          (not stored, used by AppState)
//   Initializing  → Active  (start)
//   Active        → Paused  (pause)
//   Active        → Stopping (stop)
//   Paused        → Active  (resume)
//   Paused        → Stopping (stop)
//   Stopping      → Stopped
//   any           → Error

fn is_valid_transition(from: &SessionState, to: &SessionState) -> bool {
    use SessionState::*;
    matches!(
        (from, to),
        (Initializing, Active)
            | (Active, Paused)
            | (Active, Stopping)
            | (Paused, Active)
            | (Paused, Stopping)
            | (Stopping, Stopped)
            | (Initializing, Stopping)
            | (Error, Stopping)
            | (_, Error)
    )
}

#[cfg(test)]
mod tests {
    include!("session/tests.rs");
}

// ─── Session ──────────────────────────────────────────────────────────────────

/// All mutable runtime data for a session, protected by a read-write lock.
#[derive(Debug)]
struct SessionInner {
    info: SessionInfo,
    config: SessionConfig,
    transcript: TranscriptProcessor,
    /// Temporary audio buffer paths or identifiers to clean up on stop.
    temp_files: Vec<std::path::PathBuf>,
}

/// A reference-counted session handle shared across async tasks.
#[derive(Debug, Clone)]
pub struct Session(Arc<RwLock<SessionInner>>);

impl Session {
    /// Creates a new session in the `Initializing` state.
    pub fn new(config: SessionConfig) -> Self {
        let info = SessionInfo::new(&config);
        tracing::info!(
            session_id = %info.session_id,
            audio_source = ?config.audio_source,
            language = %config.language_hint,
            "Session initialised"
        );
        let transcript = TranscriptProcessor::new(info.session_id.clone());
        Session(Arc::new(RwLock::new(SessionInner {
            info,
            config,
            transcript,
            temp_files: Vec::new(),
        })))
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    pub fn id(&self) -> String {
        self.0.read().info.session_id.clone()
    }

    pub fn state(&self) -> SessionState {
        self.0.read().info.state.clone()
    }

    pub fn info(&self) -> SessionInfo {
        self.0.read().info.clone()
    }

    pub fn config(&self) -> SessionConfig {
        self.0.read().config.clone()
    }

    pub fn transcript(&self) -> Vec<TranscriptSegment> {
        self.canonical_transcript().segments
    }

    pub fn canonical_transcript(&self) -> crate::transcript_processing::SessionTranscript {
        self.0.read().transcript.session_transcript()
    }

    pub fn ingest_recognition(&self, chunk: RecognizedChunk) -> AppResult<TranscriptUpdate> {
        if chunk.end_ms < chunk.start_ms {
            return Err(AppError::InvalidTranscript(
                "Recognition ends before it starts".into(),
            ));
        }
        let mut inner = self.0.write();
        if matches!(
            inner.info.state,
            SessionState::Stopped | SessionState::Error
        ) {
            return Err(AppError::TranscriptOperationUnavailable(
                inner.info.state.to_string(),
            ));
        }
        let segments = inner.transcript.ingest_chunk(chunk);
        Ok(TranscriptUpdate {
            session_id: inner.info.session_id.clone(),
            segments,
        })
    }

    pub fn ingest_speakers(&self, intervals: Vec<SpeakerInterval>) -> AppResult<TranscriptUpdate> {
        if intervals
            .iter()
            .any(|interval| interval.end_ms < interval.start_ms)
        {
            return Err(AppError::InvalidTranscript(
                "Speaker interval ends before it starts".into(),
            ));
        }
        let mut inner = self.0.write();
        if matches!(
            inner.info.state,
            SessionState::Stopped | SessionState::Error
        ) {
            return Err(AppError::TranscriptOperationUnavailable(
                inner.info.state.to_string(),
            ));
        }
        let segments = inner.transcript.add_speaker_intervals(intervals);
        Ok(TranscriptUpdate {
            session_id: inner.info.session_id.clone(),
            segments,
        })
    }

    pub fn transcript_segment(&self, id: u64) -> AppResult<TranscriptSegment> {
        if id == 0 {
            return Err(AppError::InvalidTranscript(
                "Segment ID must be positive".into(),
            ));
        }
        self.0
            .read()
            .transcript
            .segments
            .iter()
            .find(|segment| segment.id == id)
            .cloned()
            .ok_or(AppError::TranscriptSegmentNotFound(id))
    }

    pub fn edit_transcript_segment(&self, id: u64, text: &str) -> AppResult<TranscriptSegment> {
        let mut inner = self.0.write();
        Self::require_review_state(&inner.info.state)?;
        TranscriptManager::new(&mut inner.transcript)
            .edit_segment(id, text)
            .map_err(Into::into)
    }

    pub fn delete_transcript_segment(&self, id: u64) -> AppResult<TranscriptDeletion> {
        let mut inner = self.0.write();
        Self::require_review_state(&inner.info.state)?;
        TranscriptManager::new(&mut inner.transcript).delete_segment(id)?;
        Ok(TranscriptDeletion {
            session_id: inner.info.session_id.clone(),
            segment_id: id,
        })
    }

    pub fn search_transcript(&self, keyword: &str) -> Vec<TranscriptSegment> {
        let inner = self.0.read();
        search_segments(&inner.transcript.segments, keyword)
            .into_iter()
            .cloned()
            .collect()
    }

    pub fn transcript_text(&self) -> AppResult<String> {
        Ok(crate::transcript_management::export::format_as_txt(
            &self.transcript(),
        )?)
    }

    pub fn export_transcript(&self, path: &std::path::Path) -> AppResult<()> {
        crate::transcript_management::export::export_to_txt(&self.transcript(), path)?;
        Ok(())
    }

    fn require_review_state(state: &SessionState) -> AppResult<()> {
        if matches!(
            state,
            SessionState::Active | SessionState::Paused | SessionState::Stopped
        ) {
            Ok(())
        } else {
            Err(AppError::TranscriptOperationUnavailable(state.to_string()))
        }
    }

    pub fn fail(&self) {
        let _ = self.transition(SessionState::Error);
    }

    // ── State transitions ──────────────────────────────────────────────────────

    fn transition(&self, to: SessionState) -> AppResult<SessionInfo> {
        let mut inner = self.0.write();
        let from = &inner.info.state;

        if !is_valid_transition(from, &to) {
            return Err(AppError::InvalidStateTransition {
                from: from.to_string(),
                to: to.to_string(),
            });
        }

        let now = Utc::now().timestamp();

        match &to {
            SessionState::Active if inner.info.started_at.is_none() => {
                inner.info.started_at = Some(now);
            }
            SessionState::Stopped => {
                inner.info.ended_at = Some(now);
            }
            _ => {}
        }

        tracing::info!(
            session_id = %inner.info.session_id,
            from = %inner.info.state,
            to = %to,
            "Session state transition"
        );

        inner.info.state = to;
        Ok(inner.info.clone())
    }

    /// Transition Initializing → Active.
    pub fn start(&self) -> AppResult<SessionInfo> {
        self.transition(SessionState::Active)
    }

    /// Transition Active → Paused.
    pub fn pause(&self) -> AppResult<SessionInfo> {
        self.transition(SessionState::Paused)
    }

    /// Transition Paused → Active.
    pub fn resume(&self) -> AppResult<SessionInfo> {
        self.transition(SessionState::Active)
    }

    /// Transition Active/Paused → Stopping.
    pub fn begin_stop(&self) -> AppResult<SessionInfo> {
        self.transition(SessionState::Stopping)
    }

    /// Transition Stopping → Stopped.
    pub fn finish_stop(&self) -> AppResult<SessionInfo> {
        self.transition(SessionState::Stopped)
    }

    /// Mark session as Error state.
    pub fn set_error(&self) -> AppResult<SessionInfo> {
        self.transition(SessionState::Error)
    }

    // ── Transcript management ─────────────────────────────────────────────────

    // ── Cleanup ───────────────────────────────────────────────────────────────

    /// Register a temp file path so it can be cleaned up on session end.
    pub fn register_temp_file(&self, path: std::path::PathBuf) {
        self.0.write().temp_files.push(path);
    }

    /// Delete all registered temporary files and clear the list.
    /// Called by `AppState::stop_session` after a successful stop transition.
    pub fn cleanup_temp_files(&self) {
        let mut inner = self.0.write();
        let mut removed = 0usize;

        for path in inner.temp_files.drain(..) {
            match std::fs::remove_file(&path) {
                Ok(_) => {
                    removed += 1;
                    tracing::debug!(?path, "Temp file removed");
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // Already gone — not an error.
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        path = %path.display(),
                        "Failed to remove temp file"
                    );
                }
            }
        }

        tracing::info!(
            session_id = %inner.info.session_id,
            removed,
            "Session temp files cleaned up"
        );
    }
}
