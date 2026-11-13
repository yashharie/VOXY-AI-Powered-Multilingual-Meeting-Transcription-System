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

// Modules implemented by the respective team members.
pub mod audio_capture;
pub mod audio_processing;
pub mod speaker_analysis;
pub mod speech_recognition;
pub mod transcript_management;
pub mod transcript_processing;

use tauri::Manager;

/// Application entry point called from main.rs.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialise tracing before anything else.
    core::logging::init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
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
            core::commands::list_audio_devices,
            // ── Session lifecycle ─────────────────────────────────────────────
            core::commands::init_session,
            core::commands::start_session,
            core::commands::pause_session,
            core::commands::resume_session,
            core::commands::stop_session,
            core::commands::get_session_info,
            // ── Transcript ────────────────────────────────────────────────────
            core::commands::get_transcript,
            core::commands::get_transcript_segment,
            core::commands::edit_transcript_segment,
            core::commands::delete_transcript_segment,
            core::commands::search_transcript,
            core::commands::get_transcript_text,
            core::commands::save_transcript,
        ])
        .build(tauri::generate_context!())
        .expect("error while building VOXY application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested {
                api, code: None, ..
            } = event
            {
                api.prevent_exit();
                let app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let state = app.state::<core::state::AppState>();
                    if let Some(id) = state.active_session_id() {
                        if let Err(error) = state.stop_session(&id) {
                            tracing::warn!(%error, "Runtime shutdown failed on exit");
                        }
                    }
                    core::cleanup::purge_all_voxy_temp();
                    app.exit(0);
                });
            }
        });
}
