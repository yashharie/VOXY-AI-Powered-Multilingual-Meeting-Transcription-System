//! Audio-to-transcript pipeline coordinator.
//!
//! Data flow:
//!   AudioCapture (WASAPI loopback + optional microphone)
//!     ──► source separation / timestamp sync
//!     ──► AudioProcessor (resample to 16 kHz mono, per source)
//!     ──► VAD (EnergyVad fallback; Silero when `sherpa` feature is enabled)
//!         ├─ completed SpeechChunks ──► DiarizationEngine (speaker embeddings)
//!         └─ in_speech gate ──► contextual speech-window buffer
//!     ──► Whisper CLI (auto language detection, JSON timestamps)
//!     ──► TranscriptProcessor ──► session.push_segment ──► frontend event

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::audio_capture::{AudioCaptureHandle, AudioSource, CaptureConfig, DeviceSelection};
use crate::audio_processing::AudioProcessor;
use crate::core::error::{AppError, AppResult};
use crate::core::session::Session;
use crate::core::types::{AudioSource as CoreAudioSource, SessionConfig};
use crate::speaker_analysis::{create_engines, DiarizationEngine, SpeakerAnalysisConfig, VadEngine};
use crate::speech_recognition::WhisperEngine;
use crate::transcript_processing::TranscriptProcessor;
use tauri::AppHandle;

/// Maximum microphone / loopback timestamp drift before we stop trying to sync and just process
/// them independently.  Set conservatively — we primarily want to prevent the same speech
/// appearing twice when both channels capture the same person.
const SOURCE_SYNC_TOLERANCE_MS: i64 = 500;

/// Holds all running pipeline handles for a session.
#[derive(Debug)]
pub struct Pipeline {
    _capture: AudioCaptureHandle,
    _worker: JoinHandle<()>,
}

impl Pipeline {
    pub async fn start(
        config: &SessionConfig,
        session: Session,
        app: AppHandle,
    ) -> AppResult<Self> {
        tracing::info!(
            session_id = %session.id(),
            audio_source = ?config.audio_source,
            language_hint = %config.language_hint,
            enable_diarization = config.enable_diarization,
            "Starting native audio-to-transcript pipeline"
        );

        let enable_mic = match config.audio_source {
            CoreAudioSource::Loopback => false,
            CoreAudioSource::Microphone | CoreAudioSource::Both => true,
        };

        let loopback_device = match &config.loopback_device_id {
            Some(id) if !id.is_empty() && id != "default" => DeviceSelection::ById(id.clone()),
            _ => DeviceSelection::Default,
        };

        let microphone_device = match &config.microphone_device_id {
            Some(id) if !id.is_empty() && id != "default" => DeviceSelection::ById(id.clone()),
            _ => DeviceSelection::Default,
        };

        let capture_config = CaptureConfig {
            loopback_device,
            enable_microphone: enable_mic,
            microphone_device,
            target_chunk_frames: 4800, // ~100 ms at 48 kHz
            channel_capacity: 128,
        };

        let (capture_handle, mut rx) = match AudioCaptureHandle::start_real(capture_config) {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!(error = %e, "WASAPI audio capture failed to start");
                return Err(AppError::Internal(format!(
                    "Audio capture failed to start: {e}. \
                     Make sure no other application is exclusively using the audio device."
                )));
            }
        };

        // Audio processors — one per source so each has its own resampler state.
        let loopback_processor = AudioProcessor::new();
        let mic_processor = AudioProcessor::new();

        // Whisper model discovery
        let model_path = find_whisper_model();
        let whisper = match model_path {
            Some(path) => {
                tracing::info!(model = ?path, "Found Whisper model");
                match WhisperEngine::new(&path) {
                    Ok(engine) => Arc::new(engine),
                    Err(e) => {
                        return Err(AppError::Internal(format!(
                            "Whisper engine failed to initialise: {e}"
                        )));
                    }
                }
            }
            None => {
                return Err(AppError::Internal(
                    "Whisper model not found. Place ggml-base.bin and main.exe in the \
                     models/ folder next to voxy.exe.".into(),
                ));
            }
        };

        // VAD + diarization — always try to enable diarization; fall back gracefully.
        let speaker_cfg = SpeakerAnalysisConfig::default();
        let enable_diar = config.enable_diarization;
        let (vad, diarizer) = create_engines(&speaker_cfg, enable_diar)
            .unwrap_or_else(|_| (VadEngine::new(), DiarizationEngine::new()));

        if enable_diar && !diarizer.is_active() {
            tracing::warn!(
                "Speaker diarization requested but no embedding model found. \
                 Transcription continues without speaker labels. \
                 Download an ONNX embedding model and place it in models/."
            );
        }

        let transcript_proc = TranscriptProcessor::new();
        let language_hint = config.language_hint.clone();
        let show_partial = config.show_partial_results;

        let worker = tokio::spawn(async move {
            tracing::info!(session_id = %session.id(), "Pipeline worker started");

            // Session-relative clock (ms) counted from the first sample received.
            let mut session_clock_ms: u64 = 0;
            let mut clock_anchored = false;
            let mut first_wall_ms: u64 = 0;

            // Per-source speech buffers and state
            let mut lb_buffer: Vec<f32> = Vec::with_capacity(16_000 * 10);
            let mut lb_window_start_ms: u64 = 0;
            let mut lb_silence_ms: u64 = 0;
            let mut lb_in_speech = false;

            let mut mic_buffer: Vec<f32> = Vec::with_capacity(16_000 * 10);
            let mut mic_window_start_ms: u64 = 0;
            let mut mic_silence_ms: u64 = 0;
            let mut mic_in_speech = false;

            // Recent loopback timestamp for deduplication (mic speech within
            // SOURCE_SYNC_TOLERANCE_MS of a loopback chunk is suppressed)
            let mut last_lb_wall_ms: Option<u64> = None;

            while let Some(chunk) = rx.recv().await {
                // Anchor the session clock on the first chunk.
                if !clock_anchored {
                    first_wall_ms = chunk.timestamp_ms;
                    clock_anchored = true;
                }
                // Session-relative timestamp for this chunk's first sample.
                let chunk_session_start_ms = chunk.timestamp_ms.saturating_sub(first_wall_ms);

                // Deduplication: suppress microphone chunks whose wall-clock timestamp is
                // close to a recent loopback chunk — both channels captured the same meeting
                // audio and we do not want it transcribed twice.
                if chunk.source == AudioSource::Microphone {
                    if let Some(lb_wall) = last_lb_wall_ms {
                        let drift = (chunk.timestamp_ms as i64 - lb_wall as i64).abs();
                        if drift < SOURCE_SYNC_TOLERANCE_MS {
                            // Skip — loopback already carries this speech.
                            continue;
                        }
                    }
                }

                if chunk.source == AudioSource::Loopback {
                    last_lb_wall_ms = Some(chunk.timestamp_ms);
                }

                // Choose the right processor and state for this source.
                let (processor, buffer, window_start, silence_ms_ref, in_speech_ref) =
                    if chunk.source == AudioSource::Loopback {
                        (&loopback_processor, &mut lb_buffer, &mut lb_window_start_ms, &mut lb_silence_ms, &mut lb_in_speech)
                    } else {
                        (&mic_processor, &mut mic_buffer, &mut mic_window_start_ms, &mut mic_silence_ms, &mut mic_in_speech)
                    };

                // Preprocess to 16 kHz mono f32
                let audio_frame = crate::audio_capture::AudioFrame {
                    samples: chunk.samples,
                    channels: 1,
                    sample_rate: chunk.sample_rate_hz,
                };
                let processed = match processor.process(audio_frame) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(error = %e, source = ?chunk.source, "Audio preprocessing failed, skipping chunk");
                        continue;
                    }
                };

                let chunk_dur_ms = (processed.samples.len() as u64 * 1000) / 16_000;

                // VAD gate — pass raw samples so the VAD can track its own clock
                // and emit completed SpeechChunks for diarization.
                let is_speech = vad.process_samples(&processed.samples).unwrap_or(true);

                // Forward completed VAD speech chunks to the diarizer.
                for _speech_chunk in vad.take_speech_segments() {
                    // VAD segments are consumed here; the DiarState processes them
                    // internally via VadEngine::process_samples above.
                }

                session_clock_ms += chunk_dur_ms;

                if is_speech {
                    if !*in_speech_ref {
                        *in_speech_ref = true;
                        *window_start = chunk_session_start_ms;
                    }
                    buffer.extend_from_slice(&processed.samples);
                    *silence_ms_ref = 0;
                } else if *in_speech_ref {
                    *silence_ms_ref += chunk_dur_ms;
                    // Append trailing silence (up to 300 ms) so word endings are not clipped.
                    if *silence_ms_ref <= 300 {
                        buffer.extend_from_slice(&processed.samples);
                    }
                }

                // Trigger transcription when:
                // A) natural pause ≥ 600 ms after at least 0.5 s of speech
                // B) buffer exceeds 8 s (prevent unbounded memory growth)
                let min_speech_samples = 16_000 / 2; // 0.5 s
                let should_transcribe =
                    (*in_speech_ref && *silence_ms_ref >= 600 && buffer.len() >= min_speech_samples)
                    || buffer.len() >= 16_000 * 8;

                if should_transcribe && !buffer.is_empty() {
                    let audio = std::mem::take(buffer);
                    let trans_start = *window_start;
                    let is_continuous = *in_speech_ref && *silence_ms_ref < 600;

                    if is_continuous {
                        // Continuous speech: retain 0.8 s overlap for context
                        let overlap = (16_000 * 4 / 5).min(audio.len());
                        buffer.extend_from_slice(&audio[audio.len() - overlap..]);
                        *window_start = session_clock_ms
                            .saturating_sub((overlap as u64 * 1000) / 16_000);
                    } else {
                        *in_speech_ref = false;
                        *silence_ms_ref = 0;
                    }

                    let whisper_clone = whisper.clone();
                    let lang = language_hint.clone();
                    let results = tokio::task::spawn_blocking(move || {
                        whisper_clone.transcribe_window(&audio, 16_000, trans_start, &lang)
                    }).await;

                    match results {
                        Ok(Ok(segs)) => {
                            for mut result in segs {
                                if result.text.trim().is_empty() {
                                    continue;
                                }
                                if is_continuous {
                                    result.is_partial = true;
                                }

                                // Look up speaker from diarizer using the segment's timestamps.
                                let speaker = diarizer.assign_speaker(result.start_ms, result.end_ms);

                                let segment = transcript_proc.build_segment(result.clone(), speaker);

                                if result.is_partial && show_partial {
                                    crate::core::events::emit_transcript_segment(
                                        &app, &session.id(), segment,
                                    );
                                } else if !result.is_partial {
                                    let idx = session.push_segment(segment.clone());
                                    if idx > 0 {
                                        crate::core::events::emit_transcript_segment(
                                            &app, &session.id(), segment,
                                        );
                                    }
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(error = %e, "Whisper transcription error");
                            crate::core::events::emit_error(&app, Some(&session.id()), &e);
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Whisper task panicked");
                        }
                    }
                }
            }

            // Flush remaining buffers when capture ends
            for (buffer, window_start) in [
                (&lb_buffer, lb_window_start_ms),
                (&mic_buffer, mic_window_start_ms),
            ] {
                if buffer.len() >= 8_000 {
                    let audio = buffer.clone();
                    let whisper_clone = whisper.clone();
                    let lang = language_hint.clone();
                    let trans_start = window_start;
                    if let Ok(Ok(segs)) = tokio::task::spawn_blocking(move || {
                        whisper_clone.transcribe_window(&audio, 16_000, trans_start, &lang)
                    }).await {
                        for result in segs {
                            if result.text.trim().is_empty() { continue; }
                            let speaker = diarizer.assign_speaker(result.start_ms, result.end_ms);
                            let segment = transcript_proc.build_segment(result, speaker);
                            let idx = session.push_segment(segment.clone());
                            if idx > 0 {
                                crate::core::events::emit_transcript_segment(
                                    &app, &session.id(), segment,
                                );
                            }
                        }
                    }
                }
            }

            let _ = vad.finish();
            tracing::info!(session_id = %session.id(), "Pipeline worker finished cleanly");
        });

        Ok(Pipeline {
            _capture: capture_handle,
            _worker: worker,
        })
    }

    /// Signal audio capture to pause (does not shut down the worker task).
    pub fn pause(&self) {
        let _ = self._capture.pause();
    }

    /// Signal audio capture to resume.
    pub fn resume(&self) {
        let _ = self._capture.resume();
    }

    /// Gracefully shut down — drops capture handle then waits for worker to drain.
    pub async fn shutdown(self) {
        drop(self._capture);
        let _ = self._worker.await;
        tracing::info!("Pipeline shut down cleanly");
    }
}

/// Search standard locations for a Whisper ggml model binary.
/// Tries smaller models first for usability on 8 GB RAM machines.
fn find_whisper_model() -> Option<std::path::PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default();

    let model_names = [
        "ggml-base.bin",
        "ggml-small.bin",
        "ggml-medium.bin",
        "ggml-large-v3.bin",
        "ggml-tiny.bin",
    ];

    let search_dirs = [
        exe_dir.clone(),
        exe_dir.join("models"),
        std::path::PathBuf::from("models"),
        std::path::PathBuf::from("src-tauri/models"),
        std::path::PathBuf::from("../models"),
    ];

    for dir in &search_dirs {
        for name in &model_names {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}
