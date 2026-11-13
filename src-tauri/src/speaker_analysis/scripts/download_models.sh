#!/usr/bin/env sh
# Same as download_models.ps1 for Linux/macOS (run from src-tauri/).
set -e
mkdir -p models && cd models
B=https://github.com/k2-fsa/sherpa-onnx/releases/download
curl -L -O $B/asr-models/silero_vad.onnx
curl -L -O $B/speaker-recongition-models/3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx
curl -L $B/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2 | tar xjf -
