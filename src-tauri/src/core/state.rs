//! Tauri-managed shared application state.
//!
//! `AppState` is registered with `app.manage()` in lib.rs and injected into
//! every Tauri command via `tauri::State<AppState>`.
//! It is `Send + Sync` because all interior mutation goes through parking_lot.

use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;

use super::config::AppConfig;
use super::error::{AppError, AppResult};
use super::pipeline::Pipeline;
use super::session::Session;
use super::types::{SessionConfig, SessionInfo, SessionState};

/// Application-wide shared state injected into Tauri commands.
#[derive(Debug)]
pub struct AppState {
    lifecycle: Mutex<()>,
    runtimes: Mutex<HashMap<String, Pipeline>>,
    /// All sessions keyed by their UUID string.
    sessions: RwLock<HashMap<String, Session>>,
    /// The session that is currently Active or Paused (at most one).
    active_session_id: RwLock<Option<String>>,
    review_session_id: RwLock<Option<String>>,
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
            lifecycle: Mutex::new(()),
            runtimes: Mutex::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            active_session_id: RwLock::new(None),
            review_session_id: RwLock::new(None),
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
        let _lifecycle = self.lifecycle.lock();
        super::pipeline::adapters::single_capture_source(&config.audio_source)?;
        // Guard: only one active session at a time.
        if let Some(id) = self.active_session_id.read().as_deref() {
            return Err(AppError::SessionAlreadyActive(id.to_string()));
        }

        let session = Session::new(config);
        let id = session.id();
        // Keep the last successful session until this candidate actually starts.
        let mut sessions = self.sessions.write();
        let review = self.review_session_id.read().clone();
        sessions.retain(|id, _| Some(id) == review.as_ref());
        sessions.insert(id.clone(), session.clone());
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
        let _lifecycle = self.lifecycle.lock();
        let session = self.get_session(id)?;

        // Begin stop transition (Active/Paused → Stopping).
        session.begin_stop()?;

        let runtime = self.runtimes.lock().remove(id);
        let stopped = match runtime {
            Some(runtime) => runtime.shutdown(),
            None => Ok(()),
        };

        // A failed final inference can mark Error while shutdown is draining.
        // Still complete teardown and release the active slot before returning
        // that error to the caller.
        if session.state() == SessionState::Error {
            session.begin_stop()?;
        }

        // Finish stop (Stopping → Stopped).
        session.finish_stop()?;

        // Delete registered temp files and the whole session temp directory.
        session.cleanup_temp_files();
        super::cleanup::purge_session_temp_dir(id);

        // Release the active session slot.
        let mut active = self.active_session_id.write();
        if active.as_deref() == Some(id) {
            *active = None;
        }

        tracing::info!(session_id = %id, "Session stopped and cleaned up");
        stopped
    }

    /// Serialize lifecycle operations without holding transcript locks while waiting.
    pub fn start_runtime(
        &self,
        id: &str,
        factory: impl FnOnce(Session) -> AppResult<Pipeline>,
    ) -> AppResult<SessionInfo> {
        let _lifecycle = self.lifecycle.lock();
        let session = self.get_session(id)?;
        if session.state() != SessionState::Initializing {
            return Err(AppError::InvalidStateTransition {
                from: session.state().to_string(),
                to: "Active".into(),
            });
        }
        let runtime = factory(session.clone())?;
        match session.start() {
            Ok(info) => {
                self.runtimes.lock().insert(id.into(), runtime);
                self.sessions
                    .write()
                    .retain(|session_id, _| session_id == id);
                *self.review_session_id.write() = Some(id.into());
                Ok(info)
            }
            Err(error) => {
                let _ = runtime.shutdown();
                Err(error)
            }
        }
    }

    pub fn pause_runtime(&self, id: &str) -> AppResult<SessionInfo> {
        let _lifecycle = self.lifecycle.lock();
        let session = self.get_session(id)?;
        if session.state() != SessionState::Active {
            return Err(AppError::InvalidStateTransition {
                from: session.state().to_string(),
                to: "Paused".into(),
            });
        }
        self.runtimes
            .lock()
            .get(id)
            .ok_or_else(|| AppError::Internal("Session runtime is missing".into()))?
            .pause()?;
        session.pause()
    }

    pub fn resume_runtime(&self, id: &str) -> AppResult<SessionInfo> {
        let _lifecycle = self.lifecycle.lock();
        let session = self.get_session(id)?;
        if session.state() != SessionState::Paused {
            return Err(AppError::InvalidStateTransition {
                from: session.state().to_string(),
                to: "Active".into(),
            });
        }
        self.runtimes
            .lock()
            .get(id)
            .ok_or_else(|| AppError::Internal("Session runtime is missing".into()))?
            .resume()?;
        session.resume()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    include!("state/tests.rs");
}
