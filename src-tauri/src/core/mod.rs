//! Core application module — owned by Member 2.
//!
//! Sub-modules:
//!   types    — shared data structures (SessionInfo, TranscriptSegment, …)
//!   error    — centralised AppError type
//!   events   — Tauri event channel names and emission helpers
//!   config   — application and session configuration (Serde/JSON)
//!   session  — session lifecycle state machine
//!   state    — Tauri-managed shared application state
//!   commands — all #[tauri::command] handlers
//!   logging  — tracing subscriber initialisation

pub mod commands;
pub mod config;
pub mod error;
pub mod events;
pub mod logging;
pub mod session;
pub mod state;
pub mod types;
