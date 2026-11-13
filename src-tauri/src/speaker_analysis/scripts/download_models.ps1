# Downloads the sherpa-onnx models used by speaker_analysis into .\models (run from src-tauri\).
$ErrorActionPreference = "Stop"
$models = Join-Path (Get-Location) "models"
New-Item -ItemType Directory -Force $models | Out-Null
$base = "https://github.com/k2-fsa/sherpa-onnx/releases/download"

Invoke-WebRequest "$base/asr-models/silero_vad.onnx" -OutFile "$models\silero_vad.onnx"
Invoke-WebRequest "$base/speaker-recongition-models/3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx" `
  -OutFile "$models\3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx"

# Offline (post-meeting) diarization only; Windows 10+ ships tar.exe
$tar = "$models\sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"
Invoke-WebRequest "$base/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2" -OutFile $tar
tar -xjf $tar -C $models
Remove-Item $tar
Get-ChildItem $models -Recurse | Select-Object FullName, Length
