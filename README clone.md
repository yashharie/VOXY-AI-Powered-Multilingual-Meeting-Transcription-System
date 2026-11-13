# transcript_processing (Member 7)

Combines Whisper text, timestamps, speaker labels and language info into ordered
transcript segments, and delivers updates to the frontend.

## Inputs / outputs
| Direction | Type | From / to |
|---|---|---|
| in | `RecognizedChunk` | Member 5 (speech recognition) |
| in | `SpeakerInterval` | Member 6 (diarization) |
| out | `TranscriptUpdate` via event `transcript:update` | Member 1 (frontend) |
| out | `SessionTranscript` | Member 8 (review / export) |

Both inputs are sent into one `tokio::sync::mpsc::Sender<TranscriptInput>`.

## Event payload (camelCase JSON)
```json
{
  "sessionId": "s1",
  "segments": [
    { "id": 1, "startMs": 5200, "endMs": 8800, "speaker": "Speaker 2",
      "text": "வணக்கம் எல்லோருக்கும்", "language": "ta", "status": "final" }
  ]
}
```
`segments` holds only changed segments. The frontend replaces the segment with the
same `id`, or adds it, then sorts by `startMs`. `status` is `partial` or `final`.

## Rules implemented
- Text is only trimmed and whitespace-collapsed. No translation, no case or script changes.
- A result overlapping a `partial` segment updates it in place (same `id`).
- A result overlapping a `final` segment is ignored (repeat or stale).
- Late speaker info relabels earlier segments and re-emits them.

## For Member 2 (shared files)
Dependencies needed in `Cargo.toml`:
```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["sync", "rt", "macros"] }
tracing = "0.1"
```
Register the module with `mod transcript_processing;` and implement the emitter:
```rust
use tauri::{AppHandle, Emitter};
use crate::transcript_processing::*;

struct TauriEmitter(AppHandle);
impl TranscriptEmitter for TauriEmitter {
    fn emit_update(&self, update: &TranscriptUpdate) -> Result<(), String> {
        self.0.emit(TRANSCRIPT_UPDATE_EVENT, update).map_err(|e| e.to_string())
    }
}
// tokio::spawn(run_transcript_pipeline(session_id, rx, TauriEmitter(app_handle)));
```

## Tests
`cargo test transcript_processing`
