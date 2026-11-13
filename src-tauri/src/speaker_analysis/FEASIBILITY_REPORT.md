# VAD / diarization feasibility report (issue #6)

Status: **design + synthetic tests done; real-audio measurements pending.** Sections marked `TODO(measure)` must be
filled from actual runs before the issue is closed (acceptance criteria require limitations based on real testing).

## 1. Approach
* VAD: Silero (ONNX, sherpa-onnx) with pure-Rust energy fallback.
* Live diarization: per speech region, ~2 s windows -> speaker embedding (sherpa-onnx) -> online centroid tracker.
* Post-meeting option: sherpa-onnx offline diarization (pyannote segmentation 3.0 + embedding + clustering).

## 2. Known design limits (by construction, not measured)
* One label per ~2 s window: no overlap detection in the live path; overlapped speech gets one (dominant/mixed) label.
* Diarization != speech separation; overlapping voices are not separated.
* Speech shorter than `min_window_ms` (0.8 s), e.g. "yes", "okay", gets no label of its own (nearest-neighbour lookup).
* Label latency ≈ `min_silence_ms` + embedding time.
* Speaker count is unbounded in theory, capped at `max_speakers` (8); a new speaker is created when similarity < threshold,
  so a noisy channel can split one person into two labels.
* System-audio loopback mixes all remote participants into one stream; speakers sharing a room microphone sound alike.

## 3. Measurements to take (Windows target machine)
| Item | How | Result |
|---|---|---|
| VAD speech/non-speech correctness | `eval_wav` on labelled clips | TODO(measure) |
| Turn-taking accuracy, 2/3/4 speakers | `eval_wav` RTTM vs reference, DER (pyannote.metrics) | TODO(measure) |
| Tamil / Sinhala / English / code-switch | same, per language | TODO(measure) |
| Overlapping speech | clips with interruptions | TODO(measure) |
| Embedding model comparison (3D-Speaker vs WeSpeaker) | swap `embedding_model` | TODO(measure) |
| Live vs offline pass agreement | live RTTM vs offline RTTM | TODO(measure) |
| Real-time factor, model load time | `AnalysisStats::real_time_factor`, `eval_wav` output | TODO(measure) |
| CPU % and RAM (alone and with Whisper) | Windows Performance Monitor on the test process / app | TODO(measure) |
| Threshold tuning (`similarity_threshold`, `min_silence_ms`, `threshold`) | sweep | TODO(measure) |
