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

use super::error::{AppResult, IpcResult};
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

/// Start (or resume from Initializing) a session — begins native audio capture and transcription.
#[tauri::command]
pub async fn start_session(
    session_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IpcResult<SessionInfo>, String> {
    tracing::info!(%session_id, "IPC: start_session called");
    let result: AppResult<SessionInfo> = (async {
        let session = state.get_session(&session_id)?;
        let prev = session.state();
        let info = session.start()?;

        events::emit_session_state_changed(
            &app,
            &session_id,
            prev,
            info.state.clone(),
            None,
        );

        // Shut down any stale running pipeline if present
        if let Some(stale) = state.take_pipeline() {
            stale.shutdown().await;
        }

        // Start real native audio-to-transcript pipeline
        let session_config = session.config();
        match super::pipeline::Pipeline::start(&session_config, session.clone(), app.clone()).await {
            Ok(pipeline) => {
                state.store_pipeline(pipeline);
                tracing::info!(%session_id, "Native pipeline started and active");
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to start native pipeline");
                let _ = session.set_error();
                events::emit_session_state_changed(
                    &app,
                    &session_id,
                    info.state.clone(),
                    super::types::SessionState::Error,
                    Some(e.to_string()),
                );
                return Err(e);
            }
        }

        Ok(info)
    }).await;

    if let Err(ref e) = result {
        events::emit_error(&app, Some(&session_id), e);
    }

    Ok(result.into())
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

        // Signal audio capture to pause
        if let Ok(guard) = state.pipeline.lock() {
            if let Some(ref pipeline) = *guard {
                pipeline.pause();
            }
        }

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

        // Signal audio capture to resume
        if let Ok(guard) = state.pipeline.lock() {
            if let Some(ref pipeline) = *guard {
                pipeline.resume();
            }
        }

        Ok(info)
    })();

    if let Err(ref e) = result {
        events::emit_error(&app, Some(&session_id), e);
    }

    result.into()
}

/// Stop recording immediately, then finalize queued transcription in the background.
#[tauri::command]
pub async fn stop_session(
    session_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IpcResult<SessionInfo>, String> {
    tracing::info!(%session_id, "IPC: stop_session called");
    let result: AppResult<SessionInfo> = (|| {
        let session = state.get_session(&session_id)?;
        let previous = session.state();
        let stopping_info = session.begin_stop()?;

        events::emit_session_state_changed(
            &app,
            &session_id,
            previous,
            super::types::SessionState::Stopping,
            None,
        );

        if let Some(pipeline) = state.take_pipeline() {
            // Dropping the capture handle stops and joins the WASAPI capture
            // threads. Queued Whisper jobs continue in order on this background
            // task, so the Stop IPC command does not wait for CPU inference.
            let worker = pipeline.stop_capture();
            let app_for_finalize = app.clone();
            let id_for_finalize = session_id.clone();

            tauri::async_runtime::spawn(async move {
                if let Err(error) = worker.await {
                    tracing::error!(session_id = %id_for_finalize, error = %error, "Pipeline worker failed during stop finalization");
                }

                let managed_state = app_for_finalize.state::<AppState>();
                match managed_state.stop_session(&id_for_finalize) {
                    Ok(()) => {
                        if let Ok(session) = managed_state.get_session(&id_for_finalize) {
                            let info = session.info();
                            events::emit_session_state_changed(
                                &app_for_finalize,
                                &id_for_finalize,
                                super::types::SessionState::Stopping,
                                info.state,
                                None,
                            );
                            events::emit_session_cleanup_done(&app_for_finalize, &id_for_finalize);
                            tracing::info!(session_id = %id_for_finalize, "Background session finalization completed");
                        }
                    }
                    Err(error) => {
                        tracing::error!(session_id = %id_for_finalize, error = %error, "Session finalization failed");
                        events::emit_error(&app_for_finalize, Some(&id_for_finalize), &error);
                    }
                }
            });

            return Ok(stopping_info);
        }

        // No pipeline exists (for example, startup failed before capture).
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

    if let Err(ref error) = result {
        events::emit_error(&app, Some(&session_id), error);
    }

    Ok(result.into())
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
