//! Audio device enumeration via WASAPI.
//!
//! Wraps the `wasapi` crate's device-discovery API to expose a plain-Rust
//! list of available endpoints, each carrying the information needed to
//! display a device selection UI and to open a capture session.

use tracing::{debug, warn};
use wasapi::{DeviceCollection, Direction};

use crate::audio_capture::error::CaptureError;

/// Whether a device is a playback (output) or recording (input) endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDeviceKind {
    /// Playback endpoint — used for loopback capture.
    Output,
    /// Recording endpoint — used for microphone capture.
    Input,
}

/// A discovered audio endpoint.
#[derive(Debug, Clone)]
pub struct AudioDevice {
    /// Opaque Windows endpoint ID string. Pass this to
    /// [`DeviceSelection::ById`](crate::audio_capture::config::DeviceSelection::ById)
    /// to open a specific device.
    pub id: String,

    /// Human-readable device name (e.g. "Speakers (Realtek Audio)").
    pub name: String,

    /// Whether this is a playback or recording endpoint.
    pub kind: AudioDeviceKind,

    /// Whether this endpoint is the current system default.
    pub is_default: bool,

    /// Native sample rate reported by the device mix format, if readable.
    pub native_sample_rate_hz: Option<u32>,

    /// Native channel count, if readable.
    pub native_channels: Option<u16>,
}

/// Enumerates audio endpoints using WASAPI.
pub struct DeviceEnumerator;

impl DeviceEnumerator {
    /// Return all active output (playback) endpoints.
    pub fn list_output_devices() -> Result<Vec<AudioDevice>, CaptureError> {
        Self::enumerate(Direction::Render)
    }

    /// Return all active input (capture/microphone) endpoints.
    pub fn list_input_devices() -> Result<Vec<AudioDevice>, CaptureError> {
        Self::enumerate(Direction::Capture)
    }

    /// Return the default output endpoint, or an error if none exists.
    pub fn default_output_device() -> Result<AudioDevice, CaptureError> {
        // Initialize COM before any WASAPI call.
        wasapi::initialize_mta()
            .map_err(|e| CaptureError::Wasapi(format!("COM init: {e}")))?;

        let device = wasapi::get_default_device(&Direction::Render)
            .map_err(|e| CaptureError::Wasapi(format!("get_default_device(Render): {e}")))?;

        let id = device
            .get_id()
            .map_err(|e| CaptureError::Wasapi(format!("get_id: {e}")))?;
        let name = device.get_friendlyname().unwrap_or_else(|_| "Default Output".to_string());
        let (native_sample_rate_hz, native_channels) = Self::read_mix_format(&device);

        Ok(AudioDevice {
            id,
            name,
            kind: AudioDeviceKind::Output,
            is_default: true,
            native_sample_rate_hz,
            native_channels,
        })
    }

    /// Return the default input (microphone) endpoint, or an error if none.
    pub fn default_input_device() -> Result<AudioDevice, CaptureError> {
        wasapi::initialize_mta()
            .map_err(|e| CaptureError::Wasapi(format!("COM init: {e}")))?;

        let device = wasapi::get_default_device(&Direction::Capture)
            .map_err(|e| CaptureError::Wasapi(format!("get_default_device(Capture): {e}")))?;

        let id = device
            .get_id()
            .map_err(|e| CaptureError::Wasapi(format!("get_id: {e}")))?;
        let name = device.get_friendlyname().unwrap_or_else(|_| "Default Input".to_string());
        let (native_sample_rate_hz, native_channels) = Self::read_mix_format(&device);

        Ok(AudioDevice {
            id,
            name,
            kind: AudioDeviceKind::Input,
            is_default: true,
            native_sample_rate_hz,
            native_channels,
        })
    }

    /// Find an output device by its endpoint ID.
    pub fn find_output_device_by_id(id: &str) -> Result<AudioDevice, CaptureError> {
        Self::list_output_devices()?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| CaptureError::DeviceNotFound(id.to_string()))
    }

    /// Find an input device by its endpoint ID.
    pub fn find_input_device_by_id(id: &str) -> Result<AudioDevice, CaptureError> {
        Self::list_input_devices()?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| CaptureError::DeviceNotFound(id.to_string()))
    }

    // ── internal ─────────────────────────────────────────────────────────────

    fn enumerate(direction: Direction) -> Result<Vec<AudioDevice>, CaptureError> {
        // Initialise COM on this thread (MTA). The `wasapi` crate requires COM
        // to be initialised before any WASAPI calls.
        wasapi::initialize_mta()
            .map_err(|e| CaptureError::Wasapi(format!("COM init failed: {e}")))?;

        let collection = DeviceCollection::new(&direction).map_err(|e| {
            CaptureError::Wasapi(format!("DeviceCollection::new failed: {e}"))
        })?;

        let count = collection.get_nbr_devices().map_err(|e| {
            CaptureError::Wasapi(format!("get_nbr_devices failed: {e}"))
        })?;

        // Obtain the default device ID so we can flag it in the list.
        let default_id: Option<String> = wasapi::get_default_device(&direction)
            .ok()
            .and_then(|d| d.get_id().ok());

        let kind = match direction {
            Direction::Render => AudioDeviceKind::Output,
            Direction::Capture => AudioDeviceKind::Input,
        };

        let mut devices = Vec::with_capacity(count as usize);

        for i in 0..count {
            match collection.get_device_at_index(i) {
                Ok(device) => {
                    let id = device.get_id().unwrap_or_default();
                    let name = device.get_friendlyname().unwrap_or_else(|_| {
                        format!("Unknown Device {i}")
                    });

                    // Try reading the mix format to surface sample rate / channels.
                    let (native_sample_rate_hz, native_channels) =
                        Self::read_mix_format(&device);

                    let is_default =
                        default_id.as_deref().map_or(false, |did| did == id);

                    debug!(
                        id = %id,
                        name = %name,
                        is_default = is_default,
                        "Enumerated audio device"
                    );

                    devices.push(AudioDevice {
                        id,
                        name,
                        kind,
                        is_default,
                        native_sample_rate_hz,
                        native_channels,
                    });
                }
                Err(e) => {
                    warn!("Failed to query device at index {i}: {e}");
                }
            }
        }

        Ok(devices)
    }

    /// Try to read the mix format from a device without failing enumeration.
    fn read_mix_format(device: &wasapi::Device) -> (Option<u32>, Option<u16>) {
        let client = match device.get_iaudioclient() {
            Ok(c) => c,
            Err(_) => return (None, None),
        };

        match client.get_mixformat() {
            Ok(fmt) => {
                let rate = fmt.get_samplespersec();
                let channels = fmt.get_nchannels();
                (Some(rate), Some(channels))
            }
            Err(_) => (None, None),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod device_tests {
    use super::*;

    /// Smoke test: calling list_output_devices does not panic.
    /// On a machine with no audio hardware this should return an empty list,
    /// not an error (WASAPI returns an empty collection).
    #[test]
    #[cfg(windows)]
    fn list_output_devices_does_not_panic() {
        // This may return an error on headless CI; that's acceptable.
        let result = DeviceEnumerator::list_output_devices();
        match result {
            Ok(devices) => {
                println!("Found {} output device(s):", devices.len());
                for d in &devices {
                    println!("  {} (id={})", d.name, d.id);
                }
            }
            Err(e) => {
                println!("list_output_devices returned error (expected on headless): {e}");
            }
        }
    }
}
