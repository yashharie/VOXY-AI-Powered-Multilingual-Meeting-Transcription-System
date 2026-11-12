//! # VOXY Audio Capture Module
//!
//! Provides Windows system audio capture via WASAPI loopback and optional
//! microphone capture. Captured audio is delivered as PCM chunks to the
//! preprocessing module through a Tokio async channel.
//!
//! ## Owner
//! Member 3 — Windows Audio Capture  
//! Branch: `feature/wasapi-audio-capture`  
//! Owned directory: `src-tauri/src/audio_capture/`
//!
//! ## Public surface
//! - [`AudioCaptureManager`] — primary handle; create once, hold for the
//!   lifetime of a session.
//! - [`AudioChunk`] — the agreed PCM delivery type consumed by the
//!   preprocessing module.
//! - [`AudioDevice`] — describes a discoverable audio endpoint.
//! - [`CaptureConfig`] — session-level capture settings.
//! - [`CaptureError`] — exhaustive error type for all capture failures.

pub mod capture;
pub mod config;
pub mod device;
pub mod error;
pub mod mock;
pub mod types;

#[cfg(test)]
mod tests;

// Re-export the public API so callers only need `use audio_capture::…`.
pub use capture::AudioCaptureManager;
pub use config::CaptureConfig;
pub use device::{AudioDevice, AudioDeviceKind, DeviceEnumerator};
pub use error::CaptureError;
pub use types::AudioChunk;
