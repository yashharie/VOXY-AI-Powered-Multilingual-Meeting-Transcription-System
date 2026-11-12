//! Temporary session data cleanup.
//!
//! All temporary files and buffers created during a session are tracked
//! in `Session::temp_files` (via `register_temp_file`) and deleted here
//! when the session ends or the application exits.
//!
//! Raw audio is never permanently archived per the project specification.

use std::path::{Path, PathBuf};

use super::error::AppResult;

/// Returns the per-session temp directory under the OS temp folder.
/// Creates the directory if it does not exist.
pub fn session_temp_dir(session_id: &str) -> AppResult<PathBuf> {
    let dir = std::env::temp_dir()
        .join("voxy")
        .join(session_id);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Delete the entire per-session temp directory and all its contents.
///
/// Called by `AppState::stop_session` after the pipeline has shut down.
/// Safe to call multiple times — no-op if the directory is already gone.
pub fn purge_session_temp_dir(session_id: &str) {
    let dir = std::env::temp_dir().join("voxy").join(session_id);
    if dir.exists() {
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {
                tracing::info!(
                    session_id = %session_id,
                    path = %dir.display(),
                    "Session temp directory purged"
                );
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %dir.display(),
                    "Failed to purge session temp directory"
                );
            }
        }
    }
}

/// Delete an individual temp file.
///
/// Errors are logged as warnings — a failed cleanup should not propagate
/// as a hard error to the frontend.
pub fn remove_temp_file(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => tracing::debug!(path = %path.display(), "Temp file removed"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => { /* already gone */ }
        Err(e) => tracing::warn!(
            error = %e,
            path = %path.display(),
            "Failed to remove temp file"
        ),
    }
}

/// Purge all VOXY temp data (called on application exit as a safety net).
pub fn purge_all_voxy_temp() {
    let voxy_temp = std::env::temp_dir().join("voxy");
    if voxy_temp.exists() {
        match std::fs::remove_dir_all(&voxy_temp) {
            Ok(()) => tracing::info!("All VOXY temp data purged on exit"),
            Err(e) => tracing::warn!(error = %e, "Could not fully purge VOXY temp data on exit"),
        }
    }
}
