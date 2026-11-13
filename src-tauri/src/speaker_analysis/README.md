# speaker_analysis — VAD + speaker diarization (Member 6, issue #6)

Branch `feature/vad-diarization`. Only this directory is changed. Shared files (`Cargo.toml`, `core/pipeline.rs`)
need the small edits below, done by Member 2.

## What is implemented

| Issue task | Where |
|---|---|
| Silero VAD through sherpa-onnx / ONNX Runtime | `sherpa.rs::SherpaSileroVad` (feature `sherpa`) |
| Speech / non-speech detection, skip non-speech | `vad.rs`, `VadEngine::is_speech` (pipeline already drops non-speech blocks) |
| Fallback when model is missing | `vad.rs::EnergyVad` (pure Rust) |
| Speaker changes + generic labels ("Speaker 1") | `diarization.rs` (embedding per ~2 s window) + `speaker_tracker.rs` |
| Labels stable across consecutive chunks | `speaker_tracker.rs` (centroid per speaker, cosine match, stickiness) |
| Timestamps / segment info | `SpeakerInterval {start_ms,end_ms,speaker}`, `SpeechSegment`, `AnalysisStats` |
| Pass speaker info to transcript module | `DiarizationEngine::drain_new_intervals()` -> `TranscriptInput::Speakers` |
| Offline sherpa-onnx diarization (overlap-aware segmentation) | `sherpa.rs::OfflineDiarizer` |
| Init / inference error handling | `SpeakerAnalysisConfig::fallback_on_error`, auto-disable after 5 embedding failures |
| CPU / RAM measurement | `AnalysisStats::real_time_factor()`, `eval.rs` (+ Windows Performance Monitor) |

sherpa-onnx's diarization is **offline only** (whole recording). Live labelling therefore uses sherpa-onnx's
speaker-embedding model + our own online tracker; `OfflineDiarizer` is provided for a post-meeting pass and to
measure how good the live labels are.

## Changes needed in shared files (Member 2)

`src-tauri/Cargo.toml`
```toml
[dependencies]
sherpa-rs = { version = "0.6", optional = true, features = ["download-binaries"] }

[features]
sherpa = ["dep:sherpa-rs"]
```
Without `--features sherpa` everything still builds and runs with the energy VAD and no speaker labels.

`core/pipeline.rs` — build the two engines together (they share state; diarization needs the audio that only the
VAD call sees) and forward new speaker intervals:
```rust
let (vad, diarizer) = crate::speaker_analysis::create_engines(
    &crate::speaker_analysis::SpeakerAnalysisConfig::default(),
    config.enable_diarization,
)?;
// ... inside the worker loop, after the VAD gate:
for iv in diarizer.drain_new_intervals() {
    // forward to transcript_processing as TranscriptInput::Speakers(vec![iv, ..])
}
// when the session stops: vad.finish()?;  then drain once more.
```
`diarizer.assign_speaker(start_ms, end_ms)` keeps working as before but returns `None` while the label is not known yet.

## Things other members must know

* **Labels arrive late.** A speech region is closed after `min_silence_ms` (500 ms) of silence and then embedded, so
  its speaker is known roughly 0.5 s + embedding time after the speech ends — usually after Whisper produced the
  text. That is why `drain_new_intervals` exists; Member 7 merges by time range.
* **Time base.** All timestamps are session-relative ms counted from the first block given to `is_speech`.
  Whisper's `start_ms/end_ms` must use the same base, and every block must be passed in order.
* **Member 4 (preprocessing):** this module needs 16 kHz mono f32 and only reads `ProcessedAudio::samples`.
  Per-chunk peak normalisation would amplify silence/noise to full scale and defeat the VAD; use a slowly varying gain.
* `ProcessedAudio` in Member 2's stub and in Member 4's branch have different fields; this module is unaffected.

## Models (download once)

`scripts/download_models.ps1` (Windows) or `.sh`, run from `src-tauri/`; override the folder with `VOXY_MODELS_DIR`.

| File | Purpose |
|---|---|
| `silero_vad.onnx` | VAD |
| `3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx` | speaker embeddings (live + offline) |
| `sherpa-onnx-pyannote-segmentation-3-0/model.onnx` | segmentation (offline only) |

The embedding model is trained on Mandarin data. Whether it separates Tamil / Sinhala speakers well is **not yet
tested**; `wespeaker_en_voxceleb_resnet34.onnx` is the obvious alternative to compare (set
`ModelPaths::embedding_model`).

## Tests

```text
cargo test speaker_analysis                       # synthetic-audio unit/integration tests, no models needed
cargo test --features sherpa --release eval_wav -- --ignored --nocapture   # real audio, see eval.rs
```
The synthetic tests verify plumbing, timestamps, label stability and error handling. They say nothing about real
diarization accuracy — see `FEASIBILITY_REPORT.md` for what still has to be measured.
