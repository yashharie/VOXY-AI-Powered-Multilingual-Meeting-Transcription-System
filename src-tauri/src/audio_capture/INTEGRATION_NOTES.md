# Audio Capture Module — Integration Notes for Member 2

**Owner:** Member 3 — Windows Audio Capture  
**Branch:** `feature/wasapi-audio-capture`  
**Module path:** `src-tauri/src/audio_capture/`

---

## 1. What this module delivers

| Item | Description |
|------|-------------|
| `AudioCaptureManager::start(config)` | Opens WASAPI loopback + optional mic; returns `(CaptureHandle, Receiver<AudioChunk>)` |
| `AudioChunk` | PCM delivery type consumed by the preprocessing module (Member 4) |
| `AudioDevice` / `DeviceEnumerator` | Enumerate and select audio endpoints |
| `CaptureConfig` | Session-level settings (device selection, mic toggle, chunk size) |
| `CaptureError` | Exhaustive error enum for all failure modes |
| `MockAudioSource` | Synthetic sine-wave source for tests that need no hardware |

---

## 2. Dependencies to add to the main `Cargo.toml`

```toml
wasapi          = "0.14.0"
tokio           = { version = "1.40.0", features = ["full"] }
tracing         = "0.1.40"
tracing-subscriber = { version = "0.3.18", features = ["env-filter"] }
serde           = { version = "1.0.210", features = ["derive"] }
serde_json      = "1.0.128"
```

`wasapi` only compiles on Windows — the crate enforces this.

---

## 3. How to wire it in `src-tauri/src/lib.rs` (or `main.rs`)

```rust
mod audio_capture;
use audio_capture::{AudioCaptureManager, CaptureConfig};
```

---

## 4. Minimal session startup / shutdown

```rust
// --- start ---
let config = CaptureConfig {
    enable_microphone: false,       // set true to also capture mic
    ..CaptureConfig::default()
};
let (capture_handle, mut audio_rx) = AudioCaptureManager::start(config)?;

// --- consume on a Tokio task (hand to Member 4's preprocessing module) ---
tokio::spawn(async move {
    while let Some(chunk) = audio_rx.recv().await {
        preprocessing::process(chunk).await;
    }
});

// --- stop (on session end) ---
capture_handle.stop();   // or just drop it
```

---

## 5. Agreed PCM format

| Parameter | Value |
|-----------|-------|
| Encoding  | 32-bit IEEE float (f32), little-endian |
| Channels  | 1 (mono — down-mixed by capture layer) |
| Sample rate | Native device rate surfaced in `AudioChunk::sample_rate_hz`; resampling to 16 kHz is Member 4's responsibility |
| Chunk size | Configurable via `CaptureConfig::target_chunk_frames` (default 4800 frames ≈ 100 ms @ 48 kHz) |

---

## 6. Tauri IPC command surface (suggested)

Register these in the Tauri command layer (Member 2):

| Command | Description |
|---------|-------------|
| `list_audio_devices` | Returns `Vec<AudioDevice>` serialised to JSON |
| `start_capture` | Accepts `CaptureConfig` and starts the session |
| `stop_capture` | Stops the current session |

`AudioDevice` and `CaptureConfig` need `#[derive(serde::Serialize, serde::Deserialize)]` for IPC use — this can be added in a shared types file or by wrapping them in IPC-specific DTOs.

---

## 7. Running tests

```sh
# All unit + mock integration tests (no hardware required)
cargo test --lib

# Live WASAPI hardware tests (Windows machine with audio device)
cargo test --features wasapi_hw_test -- --nocapture
```

---

## 8. Device change / disconnection handling

When a device is unplugged during capture, the capture thread returns
`CaptureError::DeviceDisconnected`. The calling session manager (Member 2)
should:
1. Log the event.
2. Surface a user notification via a Tauri event.
3. Optionally re-enumerate devices and restart capture on the new default.
