#!/usr/bin/env python3
"""Persistent offline VOXY translation worker (JSON Lines over stdin/stdout)."""
import json
import os
import subprocess
import sys
from pathlib import Path

MODEL_REPO = "VoragoStudios/gct-m2m100-418m-ct2"
MODEL_DIR = Path(os.environ.get("LOCALAPPDATA", str(Path.home()))) / "VOXY" / "models" / "m2m100-418m-ct2"
REQUIRED_MODEL_FILES = (
    "model.bin",
    "config.json",
    "vocab.json",
    "sentencepiece.bpe.model",
)


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
        # stdout is reserved exclusively for JSON Lines responses consumed by Rust.
        pip_env = os.environ.copy()
        pip_env["PIP_DISABLE_PIP_VERSION_CHECK"] = "1"
        pip_env["PIP_PROGRESS_BAR"] = "off"
        subprocess.check_call(
            [
                sys.executable, "-m", "pip", "install", "--upgrade",
                "--disable-pip-version-check", "--progress-bar", "off",
                "ctranslate2>=4.4,<5",
                "transformers>=4.40,<5",
                "sentencepiece>=0.2,<1",
                "huggingface_hub>=0.23,<1",
            ],
            stdout=sys.stderr,
            stderr=sys.stderr,
            env=pip_env,
        )


def load_engine():
    import ctranslate2
    from huggingface_hub import snapshot_download
    from transformers import M2M100Tokenizer

    MODEL_DIR.mkdir(parents=True, exist_ok=True)
    model_is_complete = all(
        (MODEL_DIR / name).is_file() and (MODEL_DIR / name).stat().st_size > 0
        for name in REQUIRED_MODEL_FILES
    )
    if not model_is_complete:
        log("Downloading VOXY translation model and tokenizer assets (first use only)...")
        snapshot_download(
            repo_id=MODEL_REPO,
            local_dir=str(MODEL_DIR),
            allow_patterns=[
                "model.bin", "config.json", "shared_vocabulary.json",
                "sentencepiece.bpe.model", "tokenizer.json",
                "tokenizer_config.json", "special_tokens_map.json",
                "vocab.json", "generation_config.json",
            ],
        )

    missing = [name for name in REQUIRED_MODEL_FILES if not (MODEL_DIR / name).is_file()]
    if missing:
        raise RuntimeError(
            "Translation model download is incomplete; missing: " + ", ".join(missing)
        )

    # This repository is a CTranslate2 conversion. Its tokenizer_config.json
    # deliberately has no Transformers tokenizer_class and its config.json is
    # a CTranslate2 config, not an AutoConfig model config. Construct the
    # tokenizer from the two actual assets instead of invoking AutoTokenizer
    # or from_pretrained's config discovery.
    tokenizer = M2M100Tokenizer(
        vocab_file=str(MODEL_DIR / "vocab.json"),
        spm_file=str(MODEL_DIR / "sentencepiece.bpe.model"),
        src_lang="en",
    )
    translator = ctranslate2.Translator(
        str(MODEL_DIR),
        device="cpu",
        compute_type="int8",
        inter_threads=1,
        intra_threads=2,
    )
    return tokenizer, translator


def script_language(character):
    code = ord(character)
    if 0x0B80 <= code <= 0x0BFF:
        return "ta"
    if 0x0D80 <= code <= 0x0DFF:
        return "si"
    if character.isascii() and character.isalpha():
        return "en"
    if 0x00C0 <= code <= 0x024F and character.isalpha():
        return "en"
    return None


def split_by_script(text, fallback="en"):
    """Split code-switched text into script-homogeneous spans, preserving punctuation."""
    spans = []
    pending = ""
    current_language = None
    current_text = ""

    for character in text:
        language = script_language(character)
        if language is None:
            if current_language is None:
                pending += character
            else:
                current_text += character
            continue

        if current_language is None:
            current_language = language
            current_text = pending + character
            pending = ""
        elif language == current_language:
            current_text += character
        else:
            spans.append((current_language, current_text))
            current_language = language
            current_text = character

    if current_language is not None:
        spans.append((current_language, current_text + pending))
    elif pending.strip():
        spans.append((fallback if fallback in ("en", "ta", "si") else "en", pending))

    return spans


def translate_span(text, source, target, tokenizer, translator):
    if not text.strip() or source == target:
        return text

    # Tokenizers may trim boundary whitespace. Keep it outside the model call
    # so adjacent translated script spans remain separated in the final line.
    leading_length = len(text) - len(text.lstrip())
    trailing_length = len(text) - len(text.rstrip())
    leading = text[:leading_length]
    trailing = text[len(text) - trailing_length:] if trailing_length else ""
    content_end = len(text) - trailing_length if trailing_length else len(text)
    content = text[leading_length:content_end]
    if not content:
        return text

    tokenizer.src_lang = source
    encoded = tokenizer(content, return_tensors=None)
    source_tokens = tokenizer.convert_ids_to_tokens(encoded["input_ids"])
    target_token = tokenizer.lang_code_to_token[target]
    result = translator.translate_batch(
        [source_tokens],
        target_prefix=[[target_token]],
        beam_size=2,
        max_decoding_length=256,
    )[0]
    target_tokens = result.hypotheses[0][1:]
    target_ids = tokenizer.convert_tokens_to_ids(target_tokens)
    translated = tokenizer.decode(target_ids, skip_special_tokens=True).strip()
    return leading + (translated if translated else content) + trailing


def translate_text(text, source, target, tokenizer, translator):
    if not text.strip() or source == target:
        return text
    fallback = source if source in ("en", "ta", "si") else "en"
    spans = split_by_script(text, fallback)
    if not spans:
        return text
    # A mixed-script segment cannot be translated reliably with one language
    # prefix. Translate each Tamil/Sinhala/Latin span with its own source code.
    return "".join(
        translate_span(span, span_language, target, tokenizer, translator)
        for span_language, span in spans
    )


def configure_stdio_utf8():
    # Windows console defaults may be cp1252, which cannot write Tamil/Sinhala.
    # The Rust side consumes JSON Lines as UTF-8, so force all worker streams
    # to UTF-8 before reading requests or writing multilingual responses.
    for stream in (sys.stdin, sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(
                encoding="utf-8",
                errors="replace" if stream is sys.stderr else "strict",
            )


def main():
    configure_stdio_utf8()
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
                tokenizer,
                translator,
            )
            print(json.dumps({"translation": translated}, ensure_ascii=False), flush=True)
        except Exception as exc:
            log("Translation request failed: " + str(exc))
            print(json.dumps({"error": str(exc)}, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
