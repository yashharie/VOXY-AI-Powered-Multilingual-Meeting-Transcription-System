//! VOXY — Rust application library root.
//!
//! Module layout:
//!   core/          — session lifecycle, IPC commands, events, config, error types
//!   audio_capture/ — WASAPI loopback / microphone capture (Member 3)
//!   audio_processing/ — PCM preprocessing, Rubato resampling (Member 4)
//!   speech_recognition/ — Whisper integration (Member 5)
//!   speaker_analysis/ — VAD + diarization (Member 6)
//!   transcript_processing/ — segment assembly, timestamps (Member 7)
//!   transcript_management/ — save/export/cleanup (Member 8)

pub mod core;

// Stub modules owned by other members.
// Each member will replace the stub with their implementation.
pub mod audio_capture;
pub mod audio_processing;
pub mod speech_recognition;
pub mod speaker_analysis;
pub mod transcript_processing;
pub mod transcript_management;

use tauri::Manager;

/// Application entry point called from main.rs.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialise tracing before anything else.
    core::logging::init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            tracing::info!(
                version = env!("CARGO_PKG_VERSION"),
                "VOXY application starting"
            );

            // Attach the shared application state to the Tauri app handle.
            let state = core::state::AppState::new();
            app.manage(state);

            tracing::info!("Application state initialised");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // ── Application ──────────────────────────────────────────────────
            core::commands::ping,
            core::commands::get_app_version,
            // ── Session lifecycle ─────────────────────────────────────────────
            core::commands::init_session,
            core::commands::start_session,
            core::commands::pause_session,
            core::commands::resume_session,
            core::commands::stop_session,
            core::commands::get_session_info,
            // ── Transcript ────────────────────────────────────────────────────
            core::commands::get_transcript,
            core::commands::save_transcript,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                tracing::info!("Main window destroyed — triggering cleanup");
                let state = window.state::<core::state::AppState>();
                // Best-effort cleanup on window close.
                if let Some(id) = state.active_session_id() {
                    if let Err(e) = state.stop_session(&id) {
                        tracing::warn!(error = %e, "Cleanup stop_session failed on exit");
                    }
                }
                // Safety net: purge any remaining VOXY temp data.
                core::cleanup::purge_all_voxy_temp();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running VOXY application");
}
