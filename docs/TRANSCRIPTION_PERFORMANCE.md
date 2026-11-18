# VOXY transcription accuracy and latency validation

This document defines a repeatable validation procedure for Issue #23. It intentionally does not claim a target WER, CER, or real-time factor until the tests have been run on the target Windows machine.

## Configuration

PowerShell examples:

```powershell
# Select a model explicitly for a controlled comparison.
$env:VOXY_WHISPER_MODEL = "C:\\VOXY\\models\\ggml-base.bin"

# Keep one CPU core available to Windows; tune 1-4 for the reference 8 GB CPU-only machine.
$env:VOXY_WHISPER_THREADS = "3"

npm run tauri dev
```

Unset both variables to use the application defaults. Use the same model, thread count, audio device, and power mode for each comparison run.

## Test corpus

Prepare consented, locally stored reference recordings and human-verified transcripts. Keep the audio and reference text outside the application repository unless contributors have explicit permission to publish them.

Run separate sets for:

- Sinhala-only speech.
- Tamil-only speech.
- English-only speech.
- Sinhala-English code-switching.
- Tamil-English code-switching.
- Sinhala-Tamil-English mixed speech.
- Two or more speakers taking turns.
- Overlapping speech and noisy meeting audio.

For each recording, record its duration, sample rate, channel count, speaker count, and language composition.

## Metrics

Record the following per recording and per language group:

| Metric | Measurement |
|---|---|
| Word Error Rate (WER) | `(substitutions + deletions + insertions) / reference words` |
| Character Error Rate (CER) | Character-level edit distance / reference characters |
| Capture-to-first-result latency | Time from audio capture timestamp to first visible transcript segment |
| Capture-to-final latency | Time from the end of a spoken segment to its finalized transcript |
| Inference duration | Whisper processing wall-clock duration |
| Real-time factor (RTF) | Inference duration / audio duration; below 1.0 is faster than real time |
| Queue wait | Time an inference job waits before processing |
| CPU and RAM | Windows Performance Monitor during idle, single-speaker, and multi-speaker runs |
| Dropped/late audio | Any capture or inference queue overflow, with timestamp and reason |

Do not combine all languages into one WER/CER number; report each language and code-switching set separately. Preserve the normalization rules used for punctuation, whitespace, and Unicode in the report.

## Procedure

1. Use the same Windows 10/11 CPU-only machine and power profile for all runs.
2. Close unrelated high-CPU applications and record the hardware/OS details.
3. Run each sample at least three times after one warm-up run.
4. Compare Base and Small multilingual models only when each model file is available. Do not compare different hardware or thread settings as if they were model-only differences.
5. Capture application logs containing queue wait and inference duration.
6. Compare generated transcript text with the human-verified reference transcript.
7. Record the median and worst observed latency, WER, CER, RTF, CPU, and RAM.
8. Repeat with normal meeting audio and with two concurrent sources enabled.
9. Document every failure, missing model, dropped chunk, and unsupported language result.

## Interpretation

- Lower WER/CER means closer text to the reference transcript.
- RTF below 1.0 means the inference stage processed audio faster than its duration; it does not by itself prove low end-to-end latency.
- Queue wait and capture-to-final latency reveal backlog that inference duration alone cannot show.
- A language tag inferred for an entire window must not be presented as verified per-segment language for code-switched speech.
- Speaker diarization quality must be evaluated separately from speech recognition accuracy.

## Current implementation notes

- Inference is now handled by a separate bounded queue so the capture receive loop does not directly await each Whisper invocation.
- Queue back-pressure is logged when the queue is saturated; the queue is bounded to prevent unbounded memory growth.
- The current whisper.cpp CLI integration still starts a CLI process for each inference window. A persistent in-process Whisper runtime or long-lived server is a separate required optimization before claiming model warm-start behaviour.
- The application still needs real Windows measurements. Passing unit tests or compiling successfully is not evidence of transcription accuracy.

## Required report

Attach a table with one row per test recording and include the model, thread count, language group, duration, WER, CER, first-result latency, final latency, inference duration, RTF, peak CPU, peak RAM, and any dropped/late audio. Include hardware and software versions so results can be reproduced.
