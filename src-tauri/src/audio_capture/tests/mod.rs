//! Integration tests for the audio capture module.
//!
//! These tests use [`MockAudioSource`] so they run on any machine without
//! requiring real audio hardware. Live WASAPI tests are in the separate
//! `integration_wasapi` module and are gated behind the `wasapi_hw_test`
//! feature flag to avoid CI failures on machines without audio devices.

mod mock_integration;

#[cfg(feature = "wasapi_hw_test")]
mod live_wasapi;
