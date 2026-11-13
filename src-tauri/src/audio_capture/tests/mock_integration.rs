//! Integration tests using the mock audio source.
//!
//! These confirm the full data-flow from capture → channel → consumer without
//! requiring Windows audio hardware.

use crate::audio_capture::{
    mock::{MockAudioSource, MockConfig},
    types::AudioSource,
};

/// Helper: receive exactly `n` chunks from the mock and return them.
async fn collect_chunks(
    rx: &mut tokio::sync::mpsc::Receiver<crate::audio_capture::types::AudioChunk>,
    n: usize,
) -> Vec<crate::audio_capture::types::AudioChunk> {
    let mut chunks = Vec::with_capacity(n);
    for _ in 0..n {
        if let Some(c) = rx.recv().await {
            chunks.push(c);
        }
    }
    chunks
}

#[tokio::test]
async fn mock_chunks_have_correct_sample_rate() {
    let config = MockConfig {
        sample_rate_hz: 44_100,
        chunk_frames: 4410,
        ..MockConfig::default()
    };
    let (handle, mut rx) = MockAudioSource::start(config).unwrap();
    let chunks = collect_chunks(&mut rx, 3).await;
    for chunk in &chunks {
        assert_eq!(chunk.sample_rate_hz, 44_100);
    }
    handle.stop();
}

#[tokio::test]
async fn mock_chunks_contain_non_zero_samples() {
    let config = MockConfig {
        amplitude: 0.5,
        ..MockConfig::default()
    };
    let (handle, mut rx) = MockAudioSource::start(config).unwrap();
    let chunk = rx.recv().await.unwrap();
    // A sine wave at non-zero amplitude must have at least some non-zero samples.
    let has_nonzero = chunk.samples.iter().any(|&s| s.abs() > 1e-6);
    assert!(has_nonzero, "Expected non-zero audio samples");
    handle.stop();
}

#[tokio::test]
async fn mock_chunks_are_monotonically_timestamped() {
    let (handle, mut rx) = MockAudioSource::start(MockConfig::default()).unwrap();
    let chunks = collect_chunks(&mut rx, 5).await;
    for w in chunks.windows(2) {
        assert!(
            w[1].timestamp_ms >= w[0].timestamp_ms,
            "Timestamps must be non-decreasing"
        );
    }
    handle.stop();
}

#[tokio::test]
async fn mock_source_tag_is_loopback_by_default() {
    let (handle, mut rx) = MockAudioSource::start(MockConfig::default()).unwrap();
    let chunk = rx.recv().await.unwrap();
    assert_eq!(chunk.source, AudioSource::Loopback);
    handle.stop();
}

#[tokio::test]
async fn mock_microphone_source_tag() {
    let config = MockConfig {
        source: AudioSource::Microphone,
        ..MockConfig::default()
    };
    let (handle, mut rx) = MockAudioSource::start(config).unwrap();
    let chunk = rx.recv().await.unwrap();
    assert_eq!(chunk.source, AudioSource::Microphone);
    handle.stop();
}

#[tokio::test]
async fn channel_back_pressure_does_not_panic() {
    // A channel capacity of 1 means the mock thread blocks after the first
    // chunk. Stopping it cleanly must not panic.
    let config = MockConfig {
        channel_capacity: 1,
        ..MockConfig::default()
    };
    let (handle, mut rx) = MockAudioSource::start(config).unwrap();
    // Receive one chunk to unblock.
    rx.recv().await.unwrap();
    handle.stop();
}
