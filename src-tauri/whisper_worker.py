#!/usr/bin/env python3
"""VOXY Whisper Worker — faster-whisper with Tamil fine-tuned model.

Receives audio chunks as WAV file paths via JSON Lines on stdin,
transcribes them using faster-whisper, and returns results as JSON Lines
on stdout. Uses a Tamil-fine-tuned model for better Tamil accuracy.

JSON request:  {"wav_path": "C:/temp/voxy_chunk.wav", "language": "ta", "start_ms": 12345}
JSON response: {"text": "வணக்கம்", "language": "ta", "start_ms": 12345, "end_ms": 13000, "segments": [{"text": "...", "start": 0.0, "end": 1.5, "language": "ta"}]}
"""
import json
import os
import sys
import wave
import traceback
from pathlib import Path

# ── Model configuration ──────────────────────────────────────────────────────

# Tamil fine-tuned model (much better accuracy than vanilla Whisper)
TAMIL_MODEL = "vasista22/whisper-tamil-small"
# Fallback: vanilla whisper-small (HuggingFace)
FALLBACK_MODEL = "openai/whisper-small"

MODEL_CACHE_DIR = Path(os.environ.get("LOCALAPPDATA", str(Path.home()))) / "VOXY" / "models" / "faster-whisper"

def log(msg):
    print(msg, file=sys.stderr, flush=True)

def ensure_dependencies():
    try:
        from faster_whisper import WhisperModel  # noqa: F401
    except ImportError:
        log("Installing faster-whisper (first launch only)...")
        import subprocess
        subprocess.check_call(
            [sys.executable, "-m", "pip", "install", "--disable-pip-version-check",
             "--progress-bar", "off", "faster-whisper>=1.0,<2"],
            env={**os.environ, "PIP_DISABLE_PIP_VERSION_CHECK": "1", "PIP_PROGRESS_BAR": "off"}
        )

def load_model():
    """Load the Tamil fine-tuned Whisper model via faster-whisper."""
    from faster_whisper import WhisperModel

    MODEL_CACHE_DIR.mkdir(parents=True, exist_ok=True)

    # Try Tamil fine-tuned model first, fall back to vanilla whisper-small.
    for model_name in [TAMIL_MODEL, FALLBACK_MODEL]:
        try:
            log(f"Loading model: {model_name}")
            model = WhisperModel(
                model_name,
                device="cpu",
                compute_type="int8",  # Fastest on CPU, less RAM
                download_root=str(MODEL_CACHE_DIR),
            )
            log(f"Model loaded successfully: {model_name}")
            return model
        except Exception as e:
            log(f"Failed to load {model_name}: {e}")
            continue

    raise RuntimeError("Could not load any Whisper model")

def read_wav_samples(wav_path):
    """Read a WAV file and return (samples, sample_rate)."""
    with wave.open(wav_path, "rb") as wf:
        n_channels = wf.getnchannels()
        sampwidth = wf.getsampwidth()
        sample_rate = wf.getframerate()
        n_frames = wf.getnframes()
        raw = wf.readframes(n_frames)

    # Convert to float32 mono
    import struct
    if sampwidth == 2:
        samples = struct.unpack(f"<{len(raw)//2}h", raw)
        samples = [s / 32768.0 for s in samples]
    elif sampwidth == 4:
        samples = struct.unpack(f"<{len(raw)//4}i", raw)
        samples = [s / 2147483648.0 for s in samples]
    else:
        raise ValueError(f"Unsupported sample width: {sampwidth}")

    # If stereo, average channels
    if n_channels == 2:
        samples = [samples[i] for i in range(0, len(samples), 2)]

    return samples, sample_rate

def transcribe_chunk(model, wav_path, language, start_ms):
    """Transcribe one audio chunk and return results."""
    samples, sample_rate = read_wav_samples(wav_path)

    if len(samples) < sample_rate // 5:  # < 200ms, skip
        return {"text": "", "language": language, "start_ms": start_ms, "end_ms": start_ms, "segments": []}

    # Determine language hint — None means auto-detect
    lang_hint = None if not language or language == "auto" else language

    segments_iter, info = model.transcribe(
        samples,
        language=lang_hint,
        beam_size=1,          # Greedy — fastest on CPU
        best_of=1,
        vad_filter=True,      # Built-in VAD
        vad_parameters=dict(
            min_silence_duration_ms=400,
            speech_pad_ms=200,
        ),
        temperature=0.0,      # Deterministic, no hallucination retries
        condition_on_previous_text=False,  # Prevent hallucination chains
        no_speech_threshold=0.6,
        compression_ratio_threshold=2.8,
        log_prob_threshold=-2.0,
    )

    segments = []
    for seg in segments_iter:
        text = seg.text.strip()
        if not text:
            continue
        segments.append({
            "text": text,
            "start": round(seg.start, 3),
            "end": round(seg.end, 3),
            "language": info.language if info.language else language,
        })

    # Filter repetitive hallucinations
    filtered = [s for s in segments if not is_hallucination(s["text"])]

    return {
        "text": " ".join(s["text"] for s in filtered),
        "language": info.language if info.language else language,
        "start_ms": start_ms,
        "end_ms": start_ms + int(len(samples) * 1000 / sample_rate),
        "segments": filtered,
    }

def is_hallucination(text):
    """Check for common Whisper hallucination patterns."""
    if not text.strip():
        return True

    lower = text.lower().strip()

    # Common silence hallucinations
    hallucinations = [
        "thank you", "thanks", "thank you.", "you", ".", "..", "...",
        "the", "a", "i", "oh", "ah", "um", "uh", "bye", "bye.",
        "goodbye", "okay", "ok", "yes", "no", "please", "sorry",
    ]
    if lower in hallucinations:
        return True

    # Word-level repetition (80%+ same word in 7+ words)
    words = lower.split()
    if len(words) >= 7:
        from collections import Counter
        counts = Counter(words)
        if counts.most_common(1)[0][1] * 100 // len(words) >= 80:
            return True

    # Phrase-level repetition (2-4 word phrase repeating 3+ times)
    for plen in range(2, 5):
        if len(words) < plen * 3:
            continue
        for i in range(len(words) - plen * 2):
            phrase = tuple(words[i:i+plen])
            count = 1
            j = i + plen
            while j + plen <= len(words) and tuple(words[j:j+plen]) == phrase:
                count += 1
                j += plen
            if count >= 3:
                return True

    # Character-level repetition (same char 10+ times in one "word")
    for word in words:
        chars = [c for c in word if c.isalpha()]
        if len(chars) > 20:
            max_rep = 1
            cur = 1
            for k in range(1, len(chars)):
                if chars[k] == chars[k-1]:
                    cur += 1
                    max_rep = max(max_rep, cur)
                else:
                    cur = 1
            if max_rep > 10:
                return True

    return False

def main():
    log("VOXY Whisper Worker starting (faster-whisper + Tamil fine-tuned)")

    # Set UTF-8 for stdin/stdout/stderr
    for stream in (sys.stdin, sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8")

    ensure_dependencies()
    model = load_model()

    log("Worker ready — waiting for audio chunks on stdin")

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        try:
            request = json.loads(line)
            wav_path = request.get("wav_path", "")
            language = request.get("language", "auto")
            start_ms = request.get("start_ms", 0)

            if not wav_path or not os.path.isfile(wav_path):
                print(json.dumps({"error": f"WAV file not found: {wav_path}"}), flush=True)
                continue

            result = transcribe_chunk(model, wav_path, language, start_ms)
            print(json.dumps(result, ensure_ascii=False), flush=True)

            # Clean up temp WAV
            try:
                os.remove(wav_path)
            except:
                pass

        except Exception as e:
            log(f"Transcription error: {e}")
            log(traceback.format_exc())
            print(json.dumps({"error": str(e)}), ensure_ascii=False, flush=True)

if __name__ == "__main__":
    main()
