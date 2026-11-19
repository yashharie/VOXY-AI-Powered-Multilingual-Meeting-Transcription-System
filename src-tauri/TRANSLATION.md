# VOXY local live translation

VOXY keeps the Whisper transcript as the source of truth and adds a separate optional
translation to each final segment. The original text is emitted immediately; translation
runs on a separate blocking worker so it does not hold the WASAPI capture loop or the
Whisper inference queue.

## Behaviour

- The session UI asks for one target language: English, Tamil, or Sinhala.
- Whisper is configured for automatic source-language detection and the three supported
  source languages remain allowed throughout the meeting.
- The M2M100 worker detects the source script from the segment language supplied by the
  speech pipeline and translates into the selected target.
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

## Limitations

Translation cannot repair an incorrectly recognised source transcript. Accuracy depends
on Whisper's recognition and M2M100's translation quality. Mixed-script segments use
the dominant script to choose the source language; Latin transliteration of Tamil or
Sinhala may be treated as English. CPU translation is sequential because one persistent
worker protects its tokenizer/model state, and may add delay to the translated line
without delaying the original transcript.

Model: `VoragoStudios/gct-m2m100-418m-ct2`, an INT8 CTranslate2 conversion of
`facebook/m2m100_418M`. The model card identifies MIT licensing and includes tokenizer
assets. The original M2M100 model card lists English, Sinhala, and Tamil among its
supported languages.
