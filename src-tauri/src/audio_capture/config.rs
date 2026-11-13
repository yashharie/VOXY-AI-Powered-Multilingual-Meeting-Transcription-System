//! Session-level configuration for audio capture.

/// Selects which physical device to use for loopback or microphone capture.
#[derive(Debug, Clone)]
pub enum DeviceSelection {
    /// Use the system default audio output / input device.
    Default,
    /// Use the device with this exact endpoint ID string.
    ById(String),
}

impl Default for DeviceSelection {
    fn default() -> Self {
        DeviceSelection::Default
    }
}

/// Configuration passed to [`AudioCaptureManager::start`].
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Whether loopback capture is enabled. Disable for microphone-only sessions.
    pub enable_loopback: bool,
    /// Which output (loopback) device to record from.
    pub loopback_device: DeviceSelection,

    /// Whether to also capture the microphone.
    pub enable_microphone: bool,

    /// Which microphone to use when `enable_microphone` is true.
    pub microphone_device: DeviceSelection,

    /// Target number of PCM frames delivered per chunk to the downstream
    /// channel. The actual size may vary slightly based on the WASAPI
    /// buffer period. A value of 4800 at 48 kHz ≈ 100 ms per chunk.
    pub target_chunk_frames: usize,

    /// Capacity of the Tokio channel (number of `AudioChunk` items).
    /// Back-pressure is applied to the capture thread if the consumer
    /// (preprocessing module) is too slow.
    pub channel_capacity: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            enable_loopback: true,
            loopback_device: DeviceSelection::Default,
            enable_microphone: false,
            microphone_device: DeviceSelection::Default,
            // 100 ms of audio at 48 000 Hz.
            target_chunk_frames: 4800,
            channel_capacity: 64,
        }
    }
}
