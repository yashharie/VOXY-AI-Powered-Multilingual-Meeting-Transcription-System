//! Exhaustive error type for the audio capture module.

use std::fmt;

/// All errors that can occur inside the audio capture module.
#[derive(Debug)]
pub enum CaptureError {
    /// A WASAPI COM operation returned a non-success HRESULT.
    Wasapi(String),

    /// No audio output device is available on this machine.
    NoOutputDevice,

    /// No microphone (input) device is available on this machine.
    NoInputDevice,

    /// The requested device ID was not found.
    DeviceNotFound(String),

    /// The audio device was disconnected while capture was running.
    DeviceDisconnected(String),

    /// The audio format reported by the device is not supported.
    UnsupportedFormat(String),

    /// The capture thread or Tokio task encountered an internal error.
    TaskError(String),

    /// A channel send/receive operation failed (channel closed).
    ChannelClosed,

    /// Capture was requested but it has already been started.
    AlreadyRunning,

    /// An operation requires capture to be running, but it is stopped.
    NotRunning,

    /// An unclassified IO or OS error.
    Io(std::io::Error),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Wasapi(msg) => write!(f, "WASAPI error: {msg}"),
            CaptureError::NoOutputDevice => {
                write!(f, "No audio output device available")
            }
            CaptureError::NoInputDevice => {
                write!(f, "No microphone (input) device available")
            }
            CaptureError::DeviceNotFound(id) => {
                write!(f, "Audio device not found: {id}")
            }
            CaptureError::DeviceDisconnected(name) => {
                write!(f, "Audio device disconnected: {name}")
            }
            CaptureError::UnsupportedFormat(detail) => {
                write!(f, "Unsupported audio format: {detail}")
            }
            CaptureError::TaskError(msg) => {
                write!(f, "Capture task error: {msg}")
            }
            CaptureError::ChannelClosed => {
                write!(f, "Audio channel closed unexpectedly")
            }
            CaptureError::AlreadyRunning => {
                write!(f, "Audio capture is already running")
            }
            CaptureError::NotRunning => {
                write!(f, "Audio capture is not running")
            }
            CaptureError::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for CaptureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CaptureError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CaptureError {
    fn from(e: std::io::Error) -> Self {
        CaptureError::Io(e)
    }
}
