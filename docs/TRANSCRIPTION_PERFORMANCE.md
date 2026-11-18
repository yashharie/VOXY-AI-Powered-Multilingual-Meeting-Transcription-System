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

- Inference is handled by a separate bounded queue. Queue wait, inference duration, audio duration, and real-time factor are logged for each job.
- The current whisper.cpp CLI integration still starts a new CLI process and loads the model for every inference window. A persistent in-process runtime or long-lived server remains the main performance optimization; current timings must not be described as warm-start performance.
- Continuous-speech windows are now committed to the session instead of being discarded as provisional previews. The overlap deduplicator suppresses repeated text from adjacent windows. This fixes the previous behaviour where Stop removed earlier partial transcript rows.
- Tamil, Sinhala, and English can be selected together. Multiple selected languages use Whisper auto-detection; each returned segment is then labelled from its Unicode script and filtered against the selected languages. A mixed-script segment is labelled `mul`.
- Segment-level script detection is a best-effort label/filter, not a multilingual acoustic language detector. Whisper still detects one dominant language for each audio window, so actual code-switching accuracy must be measured with the test corpus above.
- Desktop device selection now uses native WASAPI endpoint IDs and keeps microphone input separate from system playback output. Browser MediaDeviceInfo IDs are not valid WASAPI endpoint IDs.
- The audio preprocessor now buffers arbitrary capture blocks and feeds Rubato's fixed 2,048-frame input chunks only when complete. This avoids passing a typical 4,800-frame WASAPI block to `process_partial`, which is only for the final shorter chunk. The final remainder is flushed when a source stops.
- WASAPI sample conversion now reads the endpoint's actual subformat, container bit depth, and valid bit depth. It handles 32-bit float, unsigned 8-bit PCM, signed 16-bit PCM, packed 24-bit PCM, 24 valid bits in a 32-bit container, and signed 32-bit PCM. Unsupported combinations are logged instead of being interpreted as float samples.
- The application still needs real Windows measurements. Passing unit tests or compiling successfully is not evidence of transcription accuracy.

## Preliminary smoke-test observation

The earlier Windows smoke test processed five-second audio windows in approximately 9.11, 12.92, 13.61, and 39.36 seconds. Those observations correspond to rough RTF values of 1.82, 2.58, 2.72, and 7.87. They are not a controlled benchmark and do not establish WER/CER. They show that the current per-window CLI process is not real-time on the tested machine and that a persistent model runtime or faster model configuration is still required.

## Required report

Attach a table with one row per test recording and include the model, thread count, language group, duration, WER, CER, first-result latency, final latency, inference duration, RTF, peak CPU, peak RAM, and any dropped/late audio. Include hardware and software versions so results can be reproduced.
