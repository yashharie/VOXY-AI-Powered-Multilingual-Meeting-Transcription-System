//! Audio-to-transcript pipeline coordinator.
//!
//! Chains the native pipeline components together:
//!   AudioCapture (WASAPI loopback + microphone)
//!     ──► AudioProcessor (resample to 16 kHz mono)
//!     ──► VAD & Diarization (shared state, speech boundary detection, speaker tracking)
//!     ──► Contextual speech windowing buffer
//!     ──► Multilingual WhisperEngine (auto language detection, exact timestamps)
//!     ──► TranscriptProcessor ──► Session::push_segment ──► Frontend event emission

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::audio_capture::{AudioCaptureHandle, CaptureConfig, DeviceSelection};
use crate::audio_processing::AudioProcessor;
use crate::core::error::{AppError, AppResult};
use crate::core::session::Session;
use crate::core::types::{AudioSource, SessionConfig};
use crate::speaker_analysis::{create_engines, DiarizationEngine, SpeakerAnalysisConfig, VadEngine};
use crate::speech_recognition::WhisperEngine;
use crate::transcript_processing::TranscriptProcessor;
use tauri::AppHandle;

/// Holds all running pipeline handles for a session.
/// Dropping this struct stops all pipeline tasks.
#[derive(Debug)]
pub struct Pipeline {
    _capture: AudioCaptureHandle,
    _worker: JoinHandle<()>,
}

impl Pipeline {
    /// Start the full audio → transcript pipeline for a session.
    ///
    /// Returns a `Pipeline` handle that keeps everything alive.
    /// Call `shutdown()` to stop cleanly, or drop for immediate teardown.
    pub async fn start(
        config: &SessionConfig,
        session: Session,
        app: AppHandle,
    ) -> AppResult<Self> {
        tracing::info!(
            session_id = %session.id(),
            audio_source = ?config.audio_source,
            language_hint = %config.language_hint,
            "Starting native audio-to-transcript pipeline"
        );

        // Configure audio capture from user settings
        let enable_mic = match config.audio_source {
            AudioSource::Loopback => false,
            AudioSource::Microphone | AudioSource::Both => true,
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
            target_chunk_frames: 4800, // 100 ms at 48 kHz
            channel_capacity: 64,
        };

        // Start real WASAPI audio capture
        let (capture_handle, mut rx) = match AudioCaptureHandle::start_real(capture_config) {
            Ok((handle, rx)) => (handle, rx),
            Err(e) => {
                tracing::error!(error = %e, "WASAPI audio capture failed to start");
                return Err(AppError::Internal(format!("Audio capture failed: {e}")));
            }
        };

        // Build audio processor (resamples to 16 kHz mono)
        let processor = AudioProcessor::new();

        // Build shared VAD + Diarization engines
        let speaker_cfg = SpeakerAnalysisConfig::default();
        let (vad, diarizer) = create_engines(&speaker_cfg, config.enable_diarization)
            .unwrap_or_else(|_| (VadEngine::new(), DiarizationEngine::new()));

        // Discover and load Whisper model — search multiple candidate paths
        let model_path = find_whisper_model();
        let whisper = match model_path {
            Some(path) => {
                tracing::info!(model = ?path, "Found Whisper model");
                match WhisperEngine::new(&path) {
                    Ok(engine) => Arc::new(engine),
                    Err(e) => {
                        return Err(AppError::Internal(format!(
                            "Failed to initialize Whisper engine: {e}"
                        )));
                    }
                }
            }
            None => {
                return Err(AppError::Internal(
                    "Whisper model not found. Place ggml-base.bin (or larger) and main.exe \
                     in the models/ directory next to the voxy executable.".into(),
                ));
            }
        };

        let transcript_proc = TranscriptProcessor::new();
        let language_hint = config.language_hint.clone();
        let show_partial = config.show_partial_results;

        // Spawn processing worker task
        let worker = tokio::spawn(async move {
            tracing::info!(session_id = %session.id(), "Pipeline worker loop started");

            let mut speech_buffer: Vec<f32> = Vec::with_capacity(16_000 * 10);
            let mut window_start_ms: u64 = 0;
            let mut session_clock_ms: u64 = 0;
            let mut silence_ms: u64 = 0;
            let mut in_speech_turn: bool = false;

            while let Some(chunk) = rx.recv().await {
                // Preprocess raw PCM chunk to 16 kHz mono
                let frame = crate::audio_capture::AudioFrame {
                    samples: chunk.samples,
                    channels: 1, // downmixed by capture thread
                    sample_rate: chunk.sample_rate_hz,
                };

                let processed = match processor.process(frame) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(error = %e, "Audio preprocessing failed for chunk");
                        continue;
                    }
                };

                let chunk_dur_ms = (processed.samples.len() as u64 * 1000) / 16000;
                let current_time_ms = session_clock_ms;
                session_clock_ms += chunk_dur_ms;

                // VAD gate
                let is_speech = vad.is_speech(&processed).unwrap_or(true);

                if is_speech {
                    if !in_speech_turn {
                        in_speech_turn = true;
                        window_start_ms = current_time_ms;
                    }
                    speech_buffer.extend_from_slice(&processed.samples);
                    silence_ms = 0;
                } else if in_speech_turn {
                    silence_ms += chunk_dur_ms;
                    // Append small trailing silence (~300ms) so final word consonants are not cut
                    if silence_ms <= 300 {
                        speech_buffer.extend_from_slice(&processed.samples);
                    }
                }

                // Drain any new speaker intervals that became available after VAD completed segments
                drain_diarizer_intervals(&diarizer, &session, &app);

                // Check trigger conditions:
                // Condition A: Natural pause after speech (>= 600ms silence with at least 0.8s speech)
                // Condition B: Maximum buffer length reached (>= 7 seconds of continuous speech)
                let should_transcribe = (in_speech_turn && silence_ms >= 600 && speech_buffer.len() >= 16_000 * 8 / 10)
                    || (speech_buffer.len() >= 16_000 * 7);

                if should_transcribe && !speech_buffer.is_empty() {
                    let audio_to_transcribe = std::mem::take(&mut speech_buffer);
                    let trans_start_ms = window_start_ms;
                    let continuous = in_speech_turn && silence_ms < 600;

                    if continuous {
                        // Continuous speech: keep last 1.0 second as overlap context for next window
                        let overlap_samples = 16_000.min(audio_to_transcribe.len());
                        speech_buffer.extend_from_slice(&audio_to_transcribe[audio_to_transcribe.len() - overlap_samples..]);
                        window_start_ms = session_clock_ms.saturating_sub((overlap_samples as u64 * 1000) / 16000);
                    } else {
                        in_speech_turn = false;
                        silence_ms = 0;
                    }

                    // Run Whisper inference in blocking thread pool
                    let whisper_clone = whisper.clone();
                    let lang_hint = language_hint.clone();
                    let results = tokio::task::spawn_blocking(move || {
                        whisper_clone.transcribe_window(&audio_to_transcribe, 16000, trans_start_ms, &lang_hint)
                    }).await;

                    match results {
                        Ok(Ok(segments)) => {
                            for mut result in segments {
                                if result.text.trim().is_empty() {
                                    continue;
                                }
                                // Mark as partial if this is an overlap window (continuous speech)
                                if continuous {
                                    result.is_partial = true;
                                }
                                let speaker = diarizer.assign_speaker(result.start_ms, result.end_ms);
                                let segment = transcript_proc.build_segment(result.clone(), speaker);

                                if result.is_partial && show_partial {
                                    // Emit partial — do NOT push to session transcript
                                    crate::core::events::emit_transcript_segment(&app, &session.id(), segment);
                                } else if !result.is_partial {
                                    // Final segment: store and emit
                                    let stored = session.push_segment(segment.clone());
                                    if stored > 0 {
                                        crate::core::events::emit_transcript_segment(&app, &session.id(), segment);
                                    }
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(error = %e, "Whisper transcription error");
                            crate::core::events::emit_error(&app, Some(&session.id()), &e);
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Whisper task join error");
                        }
                    }
                }
            }

            // Flush remaining speech audio when capture stream ends
            if !speech_buffer.is_empty() && speech_buffer.len() >= 8000 {
                let whisper_clone = whisper.clone();
                let lang_hint = language_hint.clone();
                let trans_start_ms = window_start_ms;
                if let Ok(Ok(segments)) = tokio::task::spawn_blocking(move || {
                    whisper_clone.transcribe_window(&speech_buffer, 16000, trans_start_ms, &lang_hint)
                }).await {
                    for result in segments {
                        if result.text.trim().is_empty() {
                            continue;
                        }
                        let speaker = diarizer.assign_speaker(result.start_ms, result.end_ms);
                        let segment = transcript_proc.build_segment(result, speaker);
                        let stored = session.push_segment(segment.clone());
                        if stored > 0 {
                            crate::core::events::emit_transcript_segment(&app, &session.id(), segment);
                        }
                    }
                }
            }

            let _ = vad.finish();
            // Final drain of any remaining diarization intervals
            drain_diarizer_intervals(&diarizer, &session, &app);
            tracing::info!(session_id = %session.id(), "Pipeline worker finished cleanly");
        });

        Ok(Pipeline {
            _capture: capture_handle,
            _worker: worker,
        })
    }

    /// Pause audio capture without tearing down the pipeline.
    pub fn pause(&self) {
        let _ = self._capture.pause();
    }

    /// Resume audio capture.
    pub fn resume(&self) {
        let _ = self._capture.resume();
    }

    /// Gracefully stop the pipeline and wait for worker task to drain.
    pub async fn shutdown(self) {
        drop(self._capture);
        let _ = self._worker.await;
        tracing::info!("Pipeline shut down cleanly");
    }
}

/// Drain new speaker intervals from the diarizer and update transcript segments
/// whose timestamps fall within those intervals.
/// This is called periodically in the worker loop so speaker labels arrive
/// even after Whisper has already emitted the initial segment.
fn drain_diarizer_intervals(
    diarizer: &DiarizationEngine,
    _session: &Session,
    _app: &AppHandle,
) {
    let intervals = diarizer.drain_new_intervals();
    if !intervals.is_empty() {
        tracing::debug!(count = intervals.len(), "New speaker intervals drained from diarizer");
        // Future: re-emit updated segments with corrected speaker labels.
        // Currently diarizer.assign_speaker() is called inline at transcription time.
    }
}

/// Search standard locations for the Whisper ggml model binary.
fn find_whisper_model() -> Option<std::path::PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default();

    // Try all standard model names (smallest to largest for usability)
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
        std::path::PathBuf::from("../../models"),
    ];

    for dir in &search_dirs {
        for name in &model_names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}
