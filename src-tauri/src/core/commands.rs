//! Tauri IPC command handlers.
//!
//! All public commands are registered in lib.rs via `tauri::generate_handler!`.
//! Commands follow the naming contract defined in frontend/src/lib/ipc.ts.
//!
//! Return type convention:
//!   Commands return an externally tagged `IpcResult<T>`: `{ Ok: value }`
//!   or `{ Err: { code, message } }`, handled by frontend ipc.ts.

use tauri::{AppHandle, Manager, State};

use super::error::{AppError, AppResult, IpcResult};
use super::events;
use super::state::AppState;
use super::types::{AppVersion, SessionConfig, SessionInfo};

#[derive(serde::Serialize)]
pub struct AudioDeviceInfo {
    id: String,
    name: String,
    is_default: bool,
    source: super::types::AudioSource,
}

#[tauri::command]
pub async fn list_audio_devices() -> IpcResult<Vec<AudioDeviceInfo>> {
    tauri::async_runtime::spawn_blocking(|| {
        use crate::audio_capture::DeviceEnumerator;
        let mut devices = Vec::new();
        for (result, source) in [
            (
                DeviceEnumerator::list_output_devices(),
                super::types::AudioSource::Loopback,
            ),
            (
                DeviceEnumerator::list_input_devices(),
                super::types::AudioSource::Microphone,
            ),
        ] {
            match result {
                Ok(found) => devices.extend(found.into_iter().map(|device| AudioDeviceInfo {
                    id: device.id,
                    name: format!(
                        "{} ({})",
                        device.name,
                        if source == super::types::AudioSource::Loopback {
                            "system audio"
                        } else {
                            "microphone"
                        }
                    ),
                    is_default: device.is_default,
                    source: source.clone(),
                })),
                Err(error) => tracing::warn!(%error, "Audio device enumeration failed"),
            }
        }
        Ok(devices)
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))
    .and_then(|result: AppResult<Vec<AudioDeviceInfo>>| result)
    .into()
}

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

#[derive(Clone, Copy)]
enum RuntimeOperation {
    Start,
    Pause,
    Resume,
    Stop,
}

async fn runtime_command(
    app: AppHandle,
    id: String,
    operation: RuntimeOperation,
) -> IpcResult<SessionInfo> {
    let worker_app = app.clone();
    let worker_id = id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<AppState>();
        let previous = state.get_session(&worker_id)?.state();
        let info = match operation {
            RuntimeOperation::Start => {
                let models = state.config.read().models.clone();
                state.start_runtime(&worker_id, |session| {
                    super::pipeline::start(session.config(), session, worker_app.clone(), models)
                })?
            }
            RuntimeOperation::Pause => state.pause_runtime(&worker_id)?,
            RuntimeOperation::Resume => state.resume_runtime(&worker_id)?,
            RuntimeOperation::Stop => {
                let stopped = state.stop_session(&worker_id);
                let info = state.get_session(&worker_id)?.info();
                if info.state == super::types::SessionState::Stopped {
                    events::emit_session_cleanup_done(&worker_app, &worker_id);
                    if stopped.is_err() {
                        events::emit_session_state_changed(
                            &worker_app,
                            &worker_id,
                            previous.clone(),
                            info.state.clone(),
                            None,
                        );
                    }
                }
                stopped?;
                info
            }
        };
        events::emit_session_state_changed(
            &worker_app,
            &worker_id,
            previous,
            info.state.clone(),
            None,
        );
        Ok(info)
    })
    .await
    .map_err(|error| AppError::Internal(format!("Runtime command task failed: {error}")))
    .and_then(|result: AppResult<SessionInfo>| result);
    if let Err(ref error) = result {
        events::emit_error(&app, Some(&id), error);
    }
    result.into()
}

#[tauri::command]
pub async fn start_session(session_id: String, app: AppHandle) -> IpcResult<SessionInfo> {
    runtime_command(app, session_id, RuntimeOperation::Start).await
}
#[tauri::command]
pub async fn pause_session(session_id: String, app: AppHandle) -> IpcResult<SessionInfo> {
    runtime_command(app, session_id, RuntimeOperation::Pause).await
}
#[tauri::command]
pub async fn resume_session(session_id: String, app: AppHandle) -> IpcResult<SessionInfo> {
    runtime_command(app, session_id, RuntimeOperation::Resume).await
}
#[tauri::command]
pub async fn stop_session(session_id: String, app: AppHandle) -> IpcResult<SessionInfo> {
    runtime_command(app, session_id, RuntimeOperation::Stop).await
}
/// Returns the current state snapshot of a session.
#[tauri::command]
pub fn get_session_info(session_id: String, state: State<AppState>) -> IpcResult<SessionInfo> {
    state.get_session(&session_id).map(|s| s.info()).into()
}

// ─── Transcript commands ──────────────────────────────────────────────────────

/// Returns current canonical segments, preserving partial/final status and IDs.
#[tauri::command]
pub fn get_transcript(
    session_id: String,
    state: State<AppState>,
) -> IpcResult<Vec<crate::transcript_processing::TranscriptSegment>> {
    state
        .get_session(&session_id)
        .map(|s| s.canonical_transcript().segments)
        .into()
}

/// Retrieve one canonical segment by its stable numeric ID.
#[tauri::command]
pub fn get_transcript_segment(
    session_id: String,
    segment_id: u64,
    state: State<AppState>,
) -> IpcResult<super::types::TranscriptSegment> {
    state
        .get_session(&session_id)
        .and_then(|session| session.transcript_segment(segment_id))
        .into()
}

#[tauri::command]
pub fn edit_transcript_segment(
    session_id: String,
    segment_id: u64,
    text: String,
    app: AppHandle,
    state: State<AppState>,
) -> IpcResult<super::types::TranscriptSegment> {
    let result = state
        .get_session(&session_id)
        .and_then(|session| session.edit_transcript_segment(segment_id, &text));
    match &result {
        Ok(segment) => events::emit_transcript_update(
            &app,
            crate::transcript_processing::TranscriptUpdate {
                session_id: session_id.clone(),
                segments: vec![segment.clone()],
            },
        ),
        Err(error) => events::emit_error(&app, Some(&session_id), error),
    }
    result.into()
}

#[tauri::command]
pub fn delete_transcript_segment(
    session_id: String,
    segment_id: u64,
    app: AppHandle,
    state: State<AppState>,
) -> IpcResult<crate::transcript_management::models::TranscriptDeletion> {
    let result = state
        .get_session(&session_id)
        .and_then(|session| session.delete_transcript_segment(segment_id));
    match &result {
        Ok(deletion) => events::emit_transcript_deleted(&app, deletion.clone()),
        Err(error) => events::emit_error(&app, Some(&session_id), error),
    }
    result.into()
}

#[tauri::command]
pub fn search_transcript(
    session_id: String,
    query: String,
    state: State<AppState>,
) -> IpcResult<Vec<super::types::TranscriptSegment>> {
    state
        .get_session(&session_id)
        .map(|session| session.search_transcript(&query))
        .into()
}

/// Return the same UTF-8 TXT content used by file export and clipboard copy.
#[tauri::command]
pub fn get_transcript_text(session_id: String, state: State<AppState>) -> IpcResult<String> {
    state
        .get_session(&session_id)
        .and_then(|session| session.transcript_text())
        .into()
}

/// Write to the explicit TXT destination selected by the frontend's native dialog.
#[tauri::command]
pub fn save_transcript(
    session_id: String,
    format: String,
    file_path: String,
    state: State<AppState>,
) -> IpcResult<String> {
    save_transcript_file(&state, &session_id, &format, &file_path).into()
}

pub(crate) fn save_transcript_file(
    state: &AppState,
    session_id: &str,
    format: &str,
    file_path: &str,
) -> AppResult<String> {
    let session = state.get_session(session_id)?;
    if !format.eq_ignore_ascii_case("txt") {
        return Err(AppError::InvalidTranscript(
            "Only TXT export is supported".into(),
        ));
    }
    let path = std::path::Path::new(file_path);
    if file_path.is_empty() || !path.is_absolute() {
        return Err(AppError::InvalidTranscript(
            "Export destination must be an absolute file path".into(),
        ));
    }
    session.export_transcript(path)?;
    Ok(file_path.to_owned())
}

#[cfg(test)]
mod tests {
    include!("commands/tests.rs");
}
