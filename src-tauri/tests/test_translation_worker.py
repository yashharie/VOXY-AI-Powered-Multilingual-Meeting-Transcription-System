import importlib.util
import unittest
from pathlib import Path
from types import SimpleNamespace

WORKER_PATH = Path(__file__).resolve().parents[1] / "translation_worker.py"
SPEC = importlib.util.spec_from_file_location("voxy_translation_worker", WORKER_PATH)
worker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(worker)


class FakeTokenizer:
    def __init__(self):
        self.src_lang = None
        self.lang_code_to_token = {"en": "__en__", "ta": "__ta__", "si": "__si__"}

    def __call__(self, text, return_tensors=None):
        return {"input_ids": list(text)}

    def convert_ids_to_tokens(self, ids):
        return ids

    def convert_tokens_to_ids(self, tokens):
        return tokens

    def decode(self, tokens, skip_special_tokens=True):
        return "".join(tokens)


class FakeTranslator:
    def __init__(self):
        self.calls = []

    def translate_batch(
        self,
        source,
        target_prefix,
        beam_size,
        repetition_penalty,
        no_repeat_ngram_size,
        max_decoding_length,
    ):
        text = "".join(source[0])
        self.calls.append((text, target_prefix[0][0]))
        return [SimpleNamespace(hypotheses=[[
            target_prefix[0][0],
            f"<{text}>",
        ]])]


class TranslationWorkerTests(unittest.TestCase):
    def test_split_by_script_preserves_text_and_punctuation(self):
        text = "Hello, வணக்கம்! ආයුබෝවන්."
        self.assertEqual(
            worker.split_by_script(text),
            [
                ("en", "Hello, "),
                ("ta", "வணக்கம்! "),
                ("si", "ආයුබෝවන්."),
            ],
        )

    def test_split_handles_leading_punctuation_and_whitespace(self):
        self.assertEqual(
            worker.split_by_script("  (வணக்கம்)"),
            [("ta", "  (வணக்கம்)")],
        )

    def test_mixed_language_translation_uses_each_source_language(self):
        tokenizer = FakeTokenizer()
        translator = FakeTranslator()
        result = worker.translate_text(
            "Good morning, வணக்கம்! ආයුබෝවන්.",
            "mul",
            "en",
            tokenizer,
            translator,
        )
        self.assertEqual(
            [source for source, _ in translator.calls],
            ["வணக்கம்!", "ආයුබෝවන්."],
        )
        self.assertEqual(
            [target for _, target in translator.calls],
            ["__en__", "__en__"],
        )
        self.assertEqual(
            result,
            "Good morning, <வணக்கம்!> <ආයුබෝවන්.>",
        )

    def test_translated_span_preserves_boundary_whitespace(self):
        result = worker.translate_span(
            "  வணக்கம்  ",
            "ta",
            "en",
            FakeTokenizer(),
            FakeTranslator(),
        )
        self.assertEqual(result, "  <வணக்கம்>  ")

    def test_same_language_returns_original_without_inference(self):
        translator = FakeTranslator()
        text = "வணக்கம்"
        self.assertEqual(
            worker.translate_text(text, "ta", "ta", FakeTokenizer(), translator),
            text,
        )
        self.assertEqual(translator.calls, [])


if __name__ == "__main__":
    unittest.main()
