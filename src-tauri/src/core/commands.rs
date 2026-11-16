//! Tauri IPC command handlers.
//!
//! All public commands are registered in lib.rs via `tauri::generate_handler!`.
//! Commands follow the naming contract defined in frontend/src/lib/ipc.ts.
//!
//! Return type convention:
//!   Commands return `Result<T, String>` where the error string is a JSON
//!   serialised `IpcError { code, message }`.  The frontend's `unwrap()`
//!   helper in ipc.ts parses this envelope.

use tauri::{AppHandle, State};

use super::error::{AppError, AppResult, IpcResult};
use super::events;
use super::state::AppState;
use super::types::{AppVersion, SessionConfig, SessionInfo, TranscriptSegment};

// ─── Application commands ─────────────────────────────────────────────────────

/// Health-check: returns "pong". Used by the frontend to verify IPC works.
#[tauri::command]
pub fn ping() -> &'static str {
    tracing::debug!("ping received");
    "pong"
}

/// Returns application version and build metadata.
#[tauri::command]
pub fn get_app_version() -> AppVersion {
    AppVersion {
        version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: "2".to_string(),
        build_target: std::env::consts::OS.to_string(),
    }
}

// ─── Session lifecycle commands ───────────────────────────────────────────────

/// Create a new transcription session.
///
/// Returns `SessionInfo` with state = `Initializing`.
/// The frontend should then call `start_session` to begin recording.
#[tauri::command]
pub fn init_session(
    config: SessionConfig,
    app: AppHandle,
    state: State<AppState>,
) -> IpcResult<SessionInfo> {
    tracing::info!("IPC: init_session called");
    let result: AppResult<SessionInfo> = (|| {
        let session = state.create_session(config)?;
        let info = session.info();

        events::emit_session_state_changed(
            &app,
            &info.session_id,
            super::types::SessionState::Idle,
            super::types::SessionState::Initializing,
            None,
        );

        Ok(info)
    })();

    if let Err(ref e) = result {
        events::emit_error(&app, None, e);
    }

    result.into()
}

/// Start (or resume from Initializing) a session — begins audio capture.
#[tauri::command]
pub fn start_session(
    session_id: String,
    app: AppHandle,
    state: State<AppState>,
) -> IpcResult<SessionInfo> {
    tracing::info!(%session_id, "IPC: start_session called");
    let result: AppResult<SessionInfo> = (|| {
        let session = state.get_session(&session_id)?;
        let prev = session.state();
        let info = session.start()?;
        let config = info.clone();

        events::emit_session_state_changed(
            &app,
            &session_id,
            prev,
            info.state.clone(),
            None,
        );

        // Start the real audio-to-transcript pipeline.
        let session_clone = session.clone();
        let app_clone = app.clone();
        let session_config = super::types::SessionConfig::default();
        let app_handle = app.clone();

        // Use a channel to get the pipeline result back without lifetime issues
        let (tx, rx) = tokio::sync::oneshot::channel::<super::pipeline::Pipeline>();

        tokio::spawn(async move {
            let cfg = session_config;
            match super::pipeline::Pipeline::start(&cfg, session_clone, app_clone).await {
                Ok(pipeline) => {
                    let _ = tx.send(pipeline);
                    tracing::info!("Pipeline started successfully");
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to start pipeline");
                }
            }
        });

        // Store pipeline once it's ready (non-blocking — we don't await here)
        let state_arc = std::sync::Arc::new(std::sync::Mutex::new(Option::<super::pipeline::Pipeline>::None));
        tokio::spawn(async move {
            if let Ok(pipeline) = rx.await {
                // Pipeline is ready — log it (AppState storage requires refactor)
                tracing::info!("Pipeline ready and running");
                // Drop pipeline here — it will keep running until dropped
                std::mem::forget(pipeline); // keep it alive
            }
        });

        Ok(info)
    })();

    if let Err(ref e) = result {
        events::emit_error(&app, Some(&session_id), e);
    }

    result.into()
}

/// Pause an active session — halts audio capture.
#[tauri::command]
pub fn pause_session(
    session_id: String,
    app: AppHandle,
    state: State<AppState>,
) -> IpcResult<SessionInfo> {
    tracing::info!(%session_id, "IPC: pause_session called");
    let result: AppResult<SessionInfo> = (|| {
        let session = state.get_session(&session_id)?;
        let prev = session.state();
        let info = session.pause()?;

        events::emit_session_state_changed(
            &app,
            &session_id,
            prev,
            info.state.clone(),
            None,
        );

        // TODO (Task 11): signal audio_capture to pause.

        Ok(info)
    })();

    if let Err(ref e) = result {
        events::emit_error(&app, Some(&session_id), e);
    }

    result.into()
}

/// Resume a paused session.
#[tauri::command]
pub fn resume_session(
    session_id: String,
    app: AppHandle,
    state: State<AppState>,
) -> IpcResult<SessionInfo> {
    tracing::info!(%session_id, "IPC: resume_session called");
    let result: AppResult<SessionInfo> = (|| {
        let session = state.get_session(&session_id)?;
        let prev = session.state();
        let info = session.resume()?;

        events::emit_session_state_changed(
            &app,
            &session_id,
            prev,
            info.state.clone(),
            None,
        );

        // TODO (Task 11): signal audio_capture to resume.

        Ok(info)
    })();

    if let Err(ref e) = result {
        events::emit_error(&app, Some(&session_id), e);
    }

    result.into()
}

/// Stop a session: triggers cleanup and emits cleanup-done event.
#[tauri::command]
pub fn stop_session(
    session_id: String,
    app: AppHandle,
    state: State<AppState>,
) -> IpcResult<SessionInfo> {
    tracing::info!(%session_id, "IPC: stop_session called");
    let result: AppResult<SessionInfo> = (|| {
        let session = state.get_session(&session_id)?;
        let prev = session.state();

        // Emit stopping state immediately so the UI can update.
        events::emit_session_state_changed(
            &app,
            &session_id,
            prev,
            super::types::SessionState::Stopping,
            None,
        );

        // Shut down the pipeline before stopping the session.
        if let Ok(mut guard) = state.pipeline.lock() {
            if let Some(pipeline) = guard.take() {
                tokio::spawn(async move {
                    pipeline.shutdown().await;
                });
            }
        }

        // Run the full stop + cleanup flow.
        state.stop_session(&session_id)?;

        let info = state.get_session(&session_id)?.info();

        events::emit_session_state_changed(
            &app,
            &session_id,
            super::types::SessionState::Stopping,
            info.state.clone(),
            None,
        );

        events::emit_session_cleanup_done(&app, &session_id);

        Ok(info)
    })();

    if let Err(ref e) = result {
        events::emit_error(&app, Some(&session_id), e);
    }

    result.into()
}

/// Returns the current state snapshot of a session.
#[tauri::command]
pub fn get_session_info(
    session_id: String,
    state: State<AppState>,
) -> IpcResult<SessionInfo> {
    state
        .get_session(&session_id)
        .map(|s| s.info())
        .into()
}

// ─── Transcript commands ──────────────────────────────────────────────────────

/// Returns all finalized transcript segments for a session.
#[tauri::command]
pub fn get_transcript(
    session_id: String,
    state: State<AppState>,
) -> IpcResult<Vec<TranscriptSegment>> {
    state
        .get_session(&session_id)
        .map(|s| s.transcript())
        .into()
}

/// Saves the transcript to a local file.
/// Format: "txt" or "json".
/// Returns the absolute path of the saved file.
///
/// Note: full implementation is owned by Member 8 (transcript_management).
/// This command stub delegates to the management module once integrated.
#[tauri::command]
pub fn save_transcript(
    session_id: String,
    format: String,
    state: State<AppState>,
) -> IpcResult<String> {
    tracing::info!(%session_id, %format, "IPC: save_transcript called");
    let result: AppResult<String> = (|| {
        let session = state.get_session(&session_id)?;
        let segments = session.transcript();

        // Stub — full file-dialog + write logic belongs to Member 8.
        // For now we write to %TEMP%\voxy_transcript_<id>.<ext>.
        let ext = if format == "json" { "json" } else { "txt" };
        let filename = format!("voxy_transcript_{}.{}", &session_id[..8], ext);
        let path = std::env::temp_dir().join(&filename);

        let content = if format == "json" {
            serde_json::to_string_pretty(&segments)?
        } else {
            segments
                .iter()
                .map(|s| {
                    let mins = (s.start_time / 60.0) as u32;
                    let secs = (s.start_time % 60.0) as u32;
                    format!("[{:02}:{:02}] {}", mins, secs, s.text)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        std::fs::write(&path, content)?;
        tracing::info!(?path, "Transcript saved");
        Ok(path.to_string_lossy().to_string())
    })();

    result.into()
}
