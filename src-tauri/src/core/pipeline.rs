//! Audio-to-transcript pipeline coordinator.
//!
//! This module is owned by Member 2 and is the integration point that
//! chains the downstream modules together:
//!
//!   AudioCapture → AudioProcessing → VAD → SpeechRecognition
//!       → TranscriptProcessing → Session::push_segment → Event emit
//!
//! Each downstream module exposes the trait interface defined here.
//! When a module is implemented, its concrete type is plugged in below.
//! Until then, the stub implementations in each mod.rs are used.

use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::audio_capture::{AudioCaptureHandle, AudioCaptureManager, CaptureConfig, AudioChunk};
use crate::audio_processing::AudioProcessor;
use crate::core::error::{AppResult, AppError};
use crate::core::session::Session;
use crate::core::types::SessionConfig;
use crate::speaker_analysis::{DiarizationEngine, VadEngine};
use crate::speech_recognition::WhisperEngine;
use crate::transcript_processing::TranscriptProcessor;
use tauri::AppHandle;

/// Maximum number of raw audio frames buffered between capture and processing.
#[allow(dead_code)]
const AUDIO_CHANNEL_CAPACITY: usize = 64;

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
            "Starting audio-to-transcript pipeline"
        );

        // Start real WASAPI audio capture and get the audio chunk receiver.
        let capture_config = CaptureConfig::default();
        let (capture_handle, mut rx) = match AudioCaptureHandle::start_real(capture_config) {
            Ok((handle, rx)) => (handle, rx),
            Err(e) => {
                tracing::warn!(error = %e, "WASAPI capture failed, pipeline cannot start");
                return Err(AppError::Internal(format!("Audio capture failed: {e}")));
            }
        };

        // Build processing components.
        let processor = AudioProcessor::new();
        let vad = VadEngine::new();
        let diarizer = DiarizationEngine::new();

        // Load Whisper model — look next to the executable first, then ./models/.
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_default();

        let model_path = [
            exe_dir.join("models").join("ggml-base.bin"),
            std::path::PathBuf::from("models/ggml-base.bin"),
            std::path::PathBuf::from("../models/ggml-base.bin"),
        ]
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| std::path::PathBuf::from("models/ggml-base.bin"));

        tracing::info!(path = ?model_path, "Loading Whisper model");

        let whisper = Arc::new(WhisperEngine::new(&model_path).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Whisper model not found, using stub engine");
            WhisperEngine::stub()
        }));

        let transcript_proc = TranscriptProcessor::new();
        let language_hint = config.language_hint.clone();

        // Spawn the processing worker task.
        let worker = tokio::spawn(async move {
            tracing::debug!(session_id = %session.id(), "Pipeline worker started");

            while let Some(chunk) = rx.recv().await {
                // Convert AudioChunk to AudioFrame for the preprocessor.
                let frame = crate::audio_processing::AudioFrame::new(
                    chunk.sample_rate_hz,
                    1,
                    chunk.samples,
                    None,
                    None,
                    false,
                );

                // 1. Preprocess raw PCM.
                let processed = match processor.process(crate::audio_capture::AudioFrame {
                    samples: frame.samples.clone(),
                    channels: frame.channels,
                    sample_rate: frame.sample_rate,
                }) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(error = %e, "Audio preprocessing failed, skipping frame");
                        continue;
                    }
                };

                // 2. VAD gate — skip silent frames.
                match vad.is_speech(&processed) {
                    Ok(false) => continue,
                    Err(e) => tracing::warn!(error = %e, "VAD error, passing frame through"),
                    Ok(true) => {}
                }

                // 3. Speech recognition.
                let results = match whisper.transcribe(&processed, &language_hint) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(error = %e, "Whisper transcription failed");
                        continue;
                    }
                };

                // 4. Assemble segments, push to session, emit events.
                for result in results {
                    let start_ms = result.start_ms;
                    let end_ms = result.end_ms;
                    let speaker = diarizer.assign_speaker(start_ms, end_ms);
                    let segment = transcript_proc.build_segment(result, speaker);

                    let _idx = session.push_segment(segment.clone());

                    crate::core::events::emit_transcript_segment(
                        &app,
                        &session.id(),
                        segment,
                    );
                }
            }

            tracing::debug!(session_id = %session.id(), "Pipeline worker finished");
        });

        Ok(Pipeline {
            _capture: capture_handle,
            _worker: worker,
        })
    }

    /// Gracefully stop the pipeline.
    pub async fn shutdown(self) {
        // Dropping _capture signals the capture side to stop sending frames.
        // The worker will drain remaining frames and exit.
        drop(self._capture);
        // Wait for the worker to finish.
        let _ = self._worker.await;
        tracing::info!("Pipeline shut down");
    }
}
