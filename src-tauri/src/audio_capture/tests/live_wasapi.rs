//! Live WASAPI hardware integration tests.
//!
//! These tests require a real Windows audio device.  They are gated behind the
//! `wasapi_hw_test` Cargo feature so they never run on CI machines that have no
//! audio hardware.
//!
//! Run locally with:
//! ```sh
//! cargo test --features wasapi_hw_test -- --nocapture
//! ```

use std::time::Duration;
use tokio::time::timeout;

use crate::audio_capture::{
    capture::AudioCaptureManager,
    config::CaptureConfig,
    device::DeviceEnumerator,
    types::AudioSource,
};

/// Confirm that at least one output device is discoverable.
#[test]
fn can_enumerate_output_devices() {
    let devices = DeviceEnumerator::list_output_devices()
        .expect("Should enumerate output devices");
    assert!(!devices.is_empty(), "Expected at least one output device");
    for d in &devices {
        println!("  Output: {} (id={})", d.name, d.id);
    }
}

/// Confirm that the system default output device is reported.
#[test]
fn default_output_device_is_present() {
    let device = DeviceEnumerator::default_output_device()
        .expect("Should have a default output device");
    println!("Default output: {} ({}Hz)", device.name, device.native_sample_rate_hz.unwrap_or(0));
    assert!(!device.name.is_empty());
}

/// Start loopback capture, receive at least 5 chunks, then stop.
///
/// Play audio on the system before running this test to produce non-silent
/// chunks (e.g. a tone or background music). Silent loopback data is still
/// valid PCM — the test only checks structural correctness.
#[tokio::test]
async fn loopback_capture_delivers_chunks() {
    let config = CaptureConfig {
        enable_microphone: false,
        ..CaptureConfig::default()
    };

    let (handle, mut rx) =
        AudioCaptureManager::start(config).expect("Capture should start");

    let result = timeout(Duration::from_secs(5), async {
        let mut count = 0;
        while let Some(chunk) = rx.recv().await {
            assert_eq!(chunk.source, AudioSource::Loopback);
            assert!(!chunk.samples.is_empty());
            count += 1;
            if count >= 5 {
                break;
            }
        }
        count
    })
    .await
    .expect("Should receive 5 chunks within 5 seconds");

    assert_eq!(result, 5);
    handle.stop();
}

/// Start loopback + microphone capture and receive chunks from both sources.
#[tokio::test]
async fn loopback_and_mic_capture_deliver_chunks() {
    // Skip silently if no microphone is present.
    if DeviceEnumerator::default_input_device().is_err() {
        eprintln!("No microphone available; skipping combined capture test.");
        return;
    }

    let config = CaptureConfig {
        enable_microphone: true,
        ..CaptureConfig::default()
    };

    let (handle, mut rx) =
        AudioCaptureManager::start(config).expect("Capture should start");

    let result = timeout(Duration::from_secs(10), async {
        let mut loopback_count = 0usize;
        let mut mic_count = 0usize;

        while let Some(chunk) = rx.recv().await {
            match chunk.source {
                AudioSource::Loopback => loopback_count += 1,
                AudioSource::Microphone => mic_count += 1,
            }
            if loopback_count >= 3 && mic_count >= 3 {
                break;
            }
        }
        (loopback_count, mic_count)
    })
    .await
    .expect("Should receive chunks within 10 seconds");

    assert!(result.0 >= 3, "Expected loopback chunks");
    assert!(result.1 >= 3, "Expected microphone chunks");
    handle.stop();
}
