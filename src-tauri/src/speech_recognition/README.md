# Speech Recognition Module — Member 5

Integrates [whisper.cpp](https://github.com/ggerganov/whisper.cpp) via
[whisper-rs 0.16](https://crates.io/crates/whisper-rs) for local, on-device
multilingual speech recognition in VOXY.

---

## Supported Languages

| Language | Code | Notes |
|----------|------|-------|
| Sinhala  | `si` | Requires a multilingual model |
| Tamil    | `ta` | Requires a multilingual model |
| English  | `en` | Works on both English-only and multilingual models |
| Auto     | `auto` | Whisper detects the language automatically |

---

## Model Files

VOXY uses `ggml`-format model files from whisper.cpp.  Place them in the
`models/` directory next to the binary.

### Recommended models for CPU-only Windows hardware

| File | Size | RAM (approx) | Notes |
|------|------|-------------|-------|
| `ggml-base.bin` | 148 MB | ~500 MB | Good starting point |
| `ggml-base-q5_0.bin` | 57 MB | ~200 MB | Recommended — fast + accurate |
| `ggml-small.bin` | 488 MB | ~1 GB | Better accuracy |
| `ggml-small-q5_0.bin` | 190 MB | ~400 MB | Best speed/accuracy trade-off |

### Large-v3 (accuracy reference only)

| File | Size | RAM (approx) | Notes |
|------|------|-------------|-------|
| `ggml-large-v3.bin` | 3.1 GB | ~4 GB | Highest accuracy, requires 8+ GB RAM |
| `ggml-large-v3-q5_0.bin` | 1.1 GB | ~2 GB | Quantized — feasible on 8 GB RAM |

### Download

```bash
# Using the official whisper.cpp download script
# (run from the whisper.cpp repo root)
bash models/download-ggml-model.sh base
bash models/download-ggml-model.sh small

# Or download directly from Hugging Face:
# https://huggingface.co/ggerganov/whisper.cpp/tree/main
```

Copy the downloaded `.bin` files into `src-tauri/models/`.

---

## whisper-rs API Reference (v0.16)

The correct method names used in `engine.rs`:

```rust
// Run full transcription pipeline
state.full(params, &audio.samples)?;

// Number of output segments
let n = state.full_n_segments();

// Get a segment (returns Option<WhisperSegment>)
let seg = state.get_segment(i).unwrap();

// Segment text (lossy UTF-8 — preserves Sinhala/Tamil)
let text = seg.to_str_lossy()?.trim().to_string();

// Timestamps in centiseconds (multiply by 10 for ms)
let start_ms = (seg.start_timestamp() * 10) as u64;
let end_ms   = (seg.end_timestamp()   * 10) as u64;

// No-speech probability (0.0–1.0, skip if > 0.8)
let no_speech = seg.no_speech_probability();

// Detected language ID (after full())
let lang_id = state.full_lang_id_from_state();
let lang_code = whisper_rs::get_lang_str(lang_id).unwrap_or("en");
```

> **Note:** `get_segment()`, `to_str_lossy()`, `no_speech_probability()`,
> `start_timestamp()`, `end_timestamp()`, and `full_lang_id_from_state()`
> are all correct names per whisper-rs 0.16.0 docs.
> There is no `full_get_segment_text()` or `full_lang_id()` in this version.

---

## Usage Example

```rust
use crate::speech_recognition::{WhisperEngine, WhisperLanguage};
use crate::audio_processing::ProcessedAudio;
use std::path::Path;

// Load model (falls back to stub mode if file not found)
let engine = WhisperEngine::new(Path::new("models/ggml-base-q5_0.bin"))?;

// Transcribe 16 kHz mono f32 PCM audio
let audio = ProcessedAudio {
    samples: vec![0.0f32; 16_000], // 1 second of silence
    sample_rate: 16_000,
};

let results = engine.transcribe(&audio, "auto")?;
for r in results {
    println!("[{:.1}s–{:.1}s] [{}] {}",
        r.start_ms as f64 / 1000.0,
        r.end_ms   as f64 / 1000.0,
        r.language,
        r.text,
    );
}
```

---

## Module Structure

```
speech_recognition/
├── mod.rs          — public API, re-exports
├── config.rs       — WhisperModelConfig, WhisperLanguage, ModelSize,
│                     QuantizationType, MultilingualConfig, ModelDescriptor
├── engine.rs       — WhisperEngine (loads model, runs inference)
├── evaluation.rs   — HardwareAssessment, AccuracyMetrics, ReferenceEvaluation
└── README.md       — this file
```

---

## Integration

The engine is consumed by `core/pipeline.rs`:

```
AudioCapture → AudioProcessing → VAD → WhisperEngine::transcribe()
    → TranscriptProcessing → Session → Frontend event
```

The pipeline calls `WhisperEngine::new(model_path)` at session start.
If the model file is missing, `WhisperEngine::stub()` is used automatically
and transcription returns empty results without crashing.
