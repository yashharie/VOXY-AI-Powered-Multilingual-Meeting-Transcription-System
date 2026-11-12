//! Tauri-managed shared application state.
//!
//! `AppState` is registered with `app.manage()` in lib.rs and injected into
//! every Tauri command via `tauri::State<AppState>`.
//! It is `Send + Sync` because all interior mutation goes through parking_lot.

use parking_lot::RwLock;
use std::collections::HashMap;

use super::config::AppConfig;
use super::error::{AppError, AppResult};
use super::session::Session;
use super::types::SessionConfig;

/// Application-wide shared state injected into Tauri commands.
#[derive(Debug)]
pub struct AppState {
    /// All sessions keyed by their UUID string.
    sessions: RwLock<HashMap<String, Session>>,
    /// The session that is currently Active or Paused (at most one).
    active_session_id: RwLock<Option<String>>,
    /// Loaded application configuration.
    pub config: RwLock<AppConfig>,
}

impl AppState {
    /// Create a fresh `AppState`, loading config from disk.
    pub fn new() -> Self {
        let config = super::config::load_config().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Could not load config, using defaults");
            AppConfig::default()
        });

        Self {
            sessions: RwLock::new(HashMap::new()),
            active_session_id: RwLock::new(None),
            config: RwLock::new(config),
        }
    }

    // ── Session management ────────────────────────────────────────────────────

    /// Returns the ID of the currently active/paused session, if any.
    pub fn active_session_id(&self) -> Option<String> {
        self.active_session_id.read().clone()
    }

    /// Creates and stores a new session. Returns an error if one is already active.
    pub fn create_session(&self, config: SessionConfig) -> AppResult<Session> {
        // Guard: only one active session at a time.
        if let Some(id) = self.active_session_id.read().as_deref() {
            return Err(AppError::SessionAlreadyActive(id.to_string()));
        }

        let session = Session::new(config);
        let id = session.id();
        self.sessions.write().insert(id.clone(), session.clone());
        *self.active_session_id.write() = Some(id.clone());

        tracing::info!(session_id = %id, "Session created and registered");
        Ok(session)
    }

    /// Looks up an existing session by ID.
    pub fn get_session(&self, id: &str) -> AppResult<Session> {
        self.sessions
            .read()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::SessionNotFound(id.to_string()))
    }

    /// Stops a session: runs the full stop → cleanup flow.
    pub fn stop_session(&self, id: &str) -> AppResult<()> {
        let session = self.get_session(id)?;

        // Begin stop transition (Active/Paused → Stopping).
        session.begin_stop()?;

        // --- Module teardown would be coordinated here ---
        // audio_capture::stop(), speech_recognition::stop(), etc.
        // For now this is a synchronous stub; async teardown hooks will be
        // added when the respective modules are integrated (Task 11).

        // Finish stop (Stopping → Stopped).
        session.finish_stop()?;

        // Delete temporary files.
        session.cleanup_temp_files();

        // Release the active session slot.
        let mut active = self.active_session_id.write();
        if active.as_deref() == Some(id) {
            *active = None;
        }

        tracing::info!(session_id = %id, "Session stopped and cleaned up");
        Ok(())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
