#!/usr/bin/env python3
"""Persistent offline VOXY translation worker (JSON Lines over stdin/stdout)."""
import json
import os
import subprocess
import sys
from pathlib import Path

MODEL_REPO = "VoragoStudios/gct-m2m100-418m-ct2"
MODEL_DIR = Path(os.environ.get("LOCALAPPDATA", str(Path.home()))) / "VOXY" / "models" / "m2m100-418m-ct2"

def log(message):
    print(message, file=sys.stderr, flush=True)

def ensure_dependencies():
    try:
        import ctranslate2  # noqa: F401
        import sentencepiece  # noqa: F401
        import transformers  # noqa: F401
        import huggingface_hub  # noqa: F401
    except ImportError:
        log("Installing local translation runtime (first launch only)...")
        subprocess.check_call([
            sys.executable, "-m", "pip", "install", "--upgrade",
            "ctranslate2", "transformers", "sentencepiece", "huggingface_hub"
        ])

def load_engine():
    import ctranslate2
    from huggingface_hub import snapshot_download
    from transformers import AutoTokenizer

    MODEL_DIR.mkdir(parents=True, exist_ok=True)
    if not (MODEL_DIR / "model.bin").is_file():
        log("Downloading VOXY translation model (~500 MB, first use only)...")
        snapshot_download(
            repo_id=MODEL_REPO,
            local_dir=str(MODEL_DIR),
            allow_patterns=[
                "model.bin", "config.json", "shared_vocabulary.json",
                "sentencepiece.bpe.model", "tokenizer.json",
                "tokenizer_config.json", "special_tokens_map.json",
                "vocab.json", "generation_config.json"
            ],
        )
    log("Loading M2M100 INT8 translation model...")
    tokenizer = AutoTokenizer.from_pretrained(str(MODEL_DIR), local_files_only=True)
    translator = ctranslate2.Translator(
        str(MODEL_DIR), device="cpu", compute_type="int8",
        inter_threads=1, intra_threads=2
    )
    return tokenizer, translator

def detect_script_language(text, fallback="en"):
    counts = {"ta": 0, "si": 0, "en": 0}
    for ch in text:
        code = ord(ch)
        if 0x0B80 <= code <= 0x0BFF:
            counts["ta"] += 1
        elif 0x0D80 <= code <= 0x0DFF:
            counts["si"] += 1
        elif ("A" <= ch <= "Z") or ("a" <= ch <= "z"):
            counts["en"] += 1
    best = max(counts, key=counts.get)
    return best if counts[best] else (fallback if fallback in counts else "en")

def translate_text(text, source, target, tokenizer, translator):
    if not text.strip() or source == target:
        return text
    if source not in ("en", "ta", "si"):
        source = detect_script_language(text, source)
    tokenizer.src_lang = source
    encoded = tokenizer(text, return_tensors=None)
    source_tokens = tokenizer.convert_ids_to_tokens(encoded["input_ids"])
    target_token = tokenizer.lang_code_to_token[target]
    result = translator.translate_batch(
        [source_tokens], target_prefix=[[target_token]],
        beam_size=2, max_decoding_length=256
    )[0]
    target_tokens = result.hypotheses[0][1:]
    target_ids = tokenizer.convert_tokens_to_ids(target_tokens)
    return tokenizer.decode(target_ids, skip_special_tokens=True).strip()

def main():
    try:
        ensure_dependencies()
        tokenizer, translator = load_engine()
    except Exception as exc:
        log("Translation engine could not initialise: " + str(exc))
        for line in sys.stdin:
            try:
                json.loads(line)
                print(json.dumps({"error": str(exc)}, ensure_ascii=False), flush=True)
            except Exception:
                print(json.dumps({"error": "Invalid request"}), flush=True)
        return

    for line in sys.stdin:
        try:
            request = json.loads(line)
            translated = translate_text(
                str(request.get("text", "")),
                str(request.get("source", "en")),
                str(request.get("target", "en")),
                tokenizer, translator
            )
            print(json.dumps({"translation": translated}, ensure_ascii=False), flush=True)
        except Exception as exc:
            log("Translation request failed: " + str(exc))
            print(json.dumps({"error": str(exc)}, ensure_ascii=False), flush=True)

if __name__ == "__main__":
    main()
