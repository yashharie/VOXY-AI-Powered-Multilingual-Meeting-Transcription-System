# VOXY local live translation

VOXY keeps the Whisper transcript as the source of truth and adds a separate optional
translation to each completed segment. The original text is emitted immediately;
translation runs in a separate worker so it does not hold the WASAPI capture loop or
the Whisper inference queue.

## Behaviour

- The session UI asks for one target language: English, Tamil, or Sinhala.
- Whisper automatically recognises English, Tamil, and Sinhala throughout the meeting.
- The translation worker identifies Tamil, Sinhala, and Latin-script spans independently.
  Code-switched segments are translated span by span rather than assigning one source
  language to the entire mixed-script sentence.
- Original text is never overwritten. A later transcript event updates the same segment
  with its `translation` field.
- Transcript and translation remain in session memory; no translation database is added.

## First use

The first segment that needs a language conversion starts the persistent Python worker.
It installs the local runtime packages and downloads the INT8 M2M100 model (about 500 MB)
to `%LOCALAPPDATA%\VOXY\models\m2m100-418m-ct2`. Later segments reuse the loaded model.
After setup, translation runs offline.

Python 3.10 or newer is required. Set `VOXY_PYTHON` to the Python executable path if
the Windows `python` / `py -3` launchers do not find the intended interpreter.

## Model loading details

The selected CTranslate2 model repository contains a CTranslate2 `config.json` and a
tokenizer configuration without a Transformers `tokenizer_class`. The worker therefore
constructs `M2M100Tokenizer` directly from the downloaded `vocab.json` and
`sentencepiece.bpe.model` files. It does not ask `AutoTokenizer` or `AutoConfig` to
infer a model architecture from the CTranslate2 configuration.

## Validation

The Python unit tests verify punctuation-preserving script segmentation without
downloading model files. The Windows validation workflow also downloads the real model
and runs English-to-Tamil, Tamil-to-English, Sinhala-to-English, and mixed English/Tamil
translation smoke tests. Those tests verify that the model loads and returns text in
the requested target script; they are not a substitute for a human translation-quality
evaluation.

## Limitations

Translation cannot repair an incorrectly recognised source transcript. Accuracy depends
on Whisper's recognition and M2M100's translation quality. Romanized Tamil or Sinhala
written with Latin letters may be treated as English. Code-switched text is translated
as separate script spans, so word-level language switching within the same script cannot
be detected. CPU translation is sequential because one persistent worker protects its
tokenizer/model state, and may add delay to the translated line without delaying the
original transcript.

Model: `VoragoStudios/gct-m2m100-418m-ct2`, an INT8 CTranslate2 conversion of
`facebook/m2m100_418M`. The model card identifies MIT licensing and includes tokenizer
assets. The original M2M100 model card lists English, Sinhala, and Tamil among its
supported languages.
