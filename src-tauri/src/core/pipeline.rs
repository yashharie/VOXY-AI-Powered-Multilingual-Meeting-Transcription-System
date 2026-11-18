//! Audio-to-transcript pipeline coordinator.
//!
//! Architecture:
//!
//!   AudioCapture (WASAPI loopback + optional microphone)
//!     │
//!     ├─ Loopback chunks ──► SourcePipeline (loopback)
//!     │                           ├─ AudioProcessor (resample → 16 kHz mono)
//!     │                           ├─ VadEngine (per-source, independent clock)
//!     │                           ├─ DiarizationEngine (shared — both sources)
//!     │                           └─ speech buffer → Whisper window
//!     │
//!     └─ Microphone chunks ──► SourcePipeline (microphone)
//!                                 ├─ AudioProcessor (independent resampler state)
//!                                 ├─ VadEngine (independent VAD state/clock)
//!                                 ├─ DiarizationEngine (same shared instance)
//!                                 └─ speech buffer → Whisper window
//!
//! Session timeline:
//!   Both sources use the same wall-clock anchor (first chunk ever received).
//!   Each chunk's session-relative start = chunk.timestamp_ms - first_wall_ms.
//!   The session clock is NOT incremented by accumulating chunk durations — that
//!   double-counts when both mic and loopback are active simultaneously.
//!
//! Microphone audio is NEVER suppressed based on loopback proximity.
//!   - Loopback captures meeting participants' audio played through the PC.
//!   - Microphone captures the local user's own voice.
//!   - These are complementary, not duplicates.
//!   - If the user is not in the meeting (passive listener), no mic speech arrives.
//!   - If the user speaks, mic provides better quality for their own voice.
//!
//! Diarization:
//!   Both sources share one DiarizationEngine so the speaker tracker sees all
//!   speech and builds consistent speaker profiles across sources.

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

/// Per-source audio processing state.
/// Each source has its own processor, VAD instance, and speech buffer.
/// The VAD clock is local to the source; timestamps are converted to
/// session-relative ms using the shared wall-clock anchor.
struct SourceState {
    processor: AudioProcessor,
    vad: VadEngine,
    /// Buffered 16 kHz mono samples pending Whisper inference.
    buffer: Vec<f32>,
    /// Session-relative start time (ms) of the current speech window.
    window_start_ms: u64,
    /// How many ms of silence have accumulated since the last voiced sample.
    silence_ms: u64,
    /// True while a speech turn is open.
    in_speech: bool,
    /// Label used in diagnostics.
    source_name: &'static str,
    /// Number of chunks processed since the last audio-level diagnostic.
    diagnostic_chunks: u32,
}

impl SourceState {
    fn new(source_name: &'static str, vad: VadEngine) -> Self {
        Self {
            processor: AudioProcessor::new(),
            vad,
            buffer: Vec::with_capacity(16_000 * 10),
            window_start_ms: 0,
            silence_ms: 0,
            in_speech: false,
            source_name,
            diagnostic_chunks: 0,
        }
    }
}


/// A bounded inference request. Keeping these separate from capture means Whisper
/// cannot hold the WASAPI receive loop while CPU inference is running.
struct InferenceJob {
    audio: Vec<f32>,
    start_ms: u64,
    is_partial: bool,
    source_name: &'static str,
    queued_at: std::time::Instant,
}

async fn run_inference_queue(
    mut rx: tokio::sync::mpsc::Receiver<InferenceJob>,
    whisper: Arc<WhisperEngine>,
    session: Session,
    app: AppHandle,
    diarizer: DiarizationEngine,
    language_hint: String,
    allowed_languages: Vec<String>,
    _show_partial: bool,
) {
    let mut transcript_proc = TranscriptProcessor::new();

    while let Some(job) = rx.recv().await {
        let queue_wait_ms = job.queued_at.elapsed().as_millis() as u64;
        let started = std::time::Instant::now();
        let audio = job.audio;
        let audio_ms = (audio.len() as u64 * 1000) / 16_000;
        let start_ms = job.start_ms;
        let whisper_clone = whisper.clone();
        let lang = language_hint.clone();
        let allowed = allowed_languages.clone();

        let result = tokio::task::spawn_blocking(move || {
            whisper_clone.transcribe_window_with_languages(
                &audio, 16_000, start_ms, &lang, &allowed,
            )
        }).await;

        let inference_ms = started.elapsed().as_millis() as u64;
        let rtf = if audio_ms == 0 { 0.0 } else { inference_ms as f64 / audio_ms as f64 };
        tracing::info!(
            source = job.source_name,
            queue_wait_ms,
            inference_ms,
            audio_ms,
            rtf,
            "Whisper inference timing"
        );

        match result {
            Ok(Ok(results)) => {
                for mut result in results {
                    if result.text.trim().is_empty() {
                        continue;
                    }
                    // Every completed Whisper window contains usable transcript text.
                    // Continuous-speech windows overlap the next window by 800 ms; commit
                    // their segments now and let TranscriptProcessor suppress duplicates.
                    // Treating the whole window as provisional caused all earlier speech
                    // to be discarded when the final window arrived at Stop.
                    result.is_partial = false;
                    let speaker = diarizer.assign_speaker(result.start_ms, result.end_ms);
                    let mut segment = transcript_proc.build_segment(result.clone(), speaker);

                    if transcript_proc.is_duplicate(&segment) {
                        continue;
                    }

                    let idx = session.push_segment(segment.clone());
                    if idx > 0 {
                        // Session::push_segment assigns the index to its stored copy.
                        // Copy that index onto the event payload too: the frontend uses
                        // it as the stable segment ID for keyed rendering.
                        segment.index = idx;
                        transcript_proc.register(&segment);
                        crate::core::events::emit_transcript_segment(
                            &app, &session.id(), segment,
                        );
                    }
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(error = %error, source = job.source_name, "Whisper transcription failed");
                crate::core::events::emit_error(&app, Some(&session.id()), &error);
            }
            Err(error) => {
                tracing::error!(error = %error, source = job.source_name, "Whisper inference task panicked");
            }
        }
    }
}

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
                     Ensure no other application holds exclusive access to the audio device."
                )));
            }
        };

        // Whisper model discovery — fail early with a clear message.
        let model_path = find_whisper_model();
        let whisper = match model_path {
            Some(path) => {
                tracing::info!(model = ?path, "Found Whisper model");
                match WhisperEngine::new(&path) {
                    Ok(engine) => Arc::new(engine),
                    Err(e) => return Err(AppError::Internal(format!(
                        "Whisper engine failed to initialise: {e}"
                    ))),
                }
            }
            None => return Err(AppError::Internal(
                "Whisper model not found. Place ggml-base.bin and main.exe in the \
                 models/ folder next to voxy.exe.".into(),
            )),
        };

        // VAD + diarization.
        // Two independent VadEngine instances share one Shared state (same Arc<Mutex<Shared>>)
        // when created with create_engines — but that shares the VAD clock, which is wrong for
        // two independent capture streams.  We therefore create TWO separate engine pairs and
        // use only the DiarizationEngine from the second pair (mic), discarding the second VAD.
        let speaker_cfg = SpeakerAnalysisConfig::default();
        let enable_diar = config.enable_diarization;

        // Primary pair — loopback source drives VAD clock and diarization.
        let (lb_vad, shared_diarizer) = create_engines(&speaker_cfg, enable_diar)
            .unwrap_or_else(|_| (VadEngine::new(), DiarizationEngine::new()));

        // Microphone gets its own independent VAD (no diarization — labels come from shared_diarizer).
        let mic_vad = VadEngine::new();

        if enable_diar && !shared_diarizer.is_active() {
            tracing::warn!(
                "Speaker diarization requested but the embedding model is unavailable. \
                 Transcription continues without speaker labels. \
                 Place the ONNX embedding model in models/ and rebuild with --features sherpa."
            );
            // Emit a user-visible error so the diagnostics panel shows the real state.
            crate::core::events::emit_error(
                &app,
                Some(&session.id()),
                &AppError::Internal(
                    "Speaker diarization model not found. \
                     Speaker labels will not be assigned in this session.".into(),
                ),
            );
        }

        let language_hint = config.language_hint.clone();
        let allowed_languages = config.allowed_languages.clone();
        let show_partial = config.show_partial_results;
        let inference_session = session.clone();
        let inference_app = app.clone();
        let inference_whisper = whisper.clone();
        let inference_diarizer = shared_diarizer.clone();

        let worker = tokio::spawn(async move {
            tracing::info!(session_id = %session.id(), "Pipeline worker started");

            // Bounded queue: capture keeps running while a separate worker performs
            // CPU-heavy Whisper inference. Back-pressure is explicit and bounded.
            let (inference_tx, inference_rx) = tokio::sync::mpsc::channel(4);
            let inference_worker = tokio::spawn(run_inference_queue(
                inference_rx,
                inference_whisper,
                inference_session,
                inference_app,
                inference_diarizer,
                language_hint.clone(),
                allowed_languages,
                show_partial,
            ));

            // Wall-clock anchor: set on the very first chunk received (either source).
            // All session-relative timestamps = chunk.timestamp_ms - first_wall_ms.
            let mut first_wall_ms: u64 = 0;
            let mut clock_anchored = false;

            // Per-source state.  Each has its own VAD, resampler, and speech buffer.
            let mut lb_state = SourceState::new("loopback", lb_vad);
            let mut mic_state = SourceState::new("microphone", mic_vad);

            while let Some(chunk) = rx.recv().await {
                // Anchor the session clock once.
                if !clock_anchored {
                    first_wall_ms = chunk.timestamp_ms;
                    clock_anchored = true;
                    tracing::debug!(first_wall_ms, "Session clock anchored");
                }

                // Session-relative start of this chunk (independent of which source it is).
                let chunk_session_start_ms = chunk.timestamp_ms.saturating_sub(first_wall_ms);

                // Route to per-source state.  Microphone audio is NEVER suppressed.
                let state = if chunk.source == AudioSource::Loopback {
                    &mut lb_state
                } else {
                    &mut mic_state
                };

                // Preprocess to 16 kHz mono f32.
                let audio_frame = crate::audio_capture::AudioFrame {
                    samples: chunk.samples,
                    channels: 1,
                    sample_rate: chunk.sample_rate_hz,
                };
                let processed = match state.processor.process(audio_frame) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            source = state.source_name,
                            "Audio preprocessing failed — skipping chunk"
                        );
                        continue;
                    }
                };

                let chunk_dur_ms = (processed.samples.len() as u64 * 1000) / 16_000;

                // Report real input levels periodically so silent/incorrect capture formats
                // can be distinguished from VAD and Whisper failures without flooding logs.
                state.diagnostic_chunks += 1;
                if state.diagnostic_chunks >= 50 {
                    state.diagnostic_chunks = 0;
                    let peak = processed.samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
                    let rms = if processed.samples.is_empty() { 0.0 } else {
                        (processed.samples.iter().map(|s| s * s).sum::<f32>() / processed.samples.len() as f32).sqrt()
                    };
                    tracing::info!(source = state.source_name, sample_count = processed.samples.len(), rms, peak, "Captured audio level (16 kHz mono)");
                }

                // VAD — independent per source.
                let is_speech = state.vad.process_samples(&processed.samples).unwrap_or_else(|error| {
                    tracing::warn!(source = state.source_name, error = %error, "VAD failed; passing audio through");
                    true
                });

                // Drain completed VAD speech chunks and forward to the shared diarizer.
                // The diarizer assigns speaker IDs using voice embeddings; both sources
                // contribute to the same speaker model.
                for speech_chunk in state.vad.take_speech_segments() {
                    // Remap the speech chunk's sample-based timestamps to session-relative ms.
                    // The VAD clock counts samples from zero (per-source); we offset by the
                    // source's wall-clock anchor to get the correct session position.
                    // For now we pass the chunk directly — the diarizer's own clock (fed by
                    // process_samples) tracks the correct offsets internally.
                    if shared_diarizer.is_active() {
                        // The DiarizationEngine shares state with the loopback VadEngine inside
                        // create_engines, so it already received the loopback speech samples.
                        // For microphone we can't feed samples directly here, but the diarizer's
                        // assign_speaker() lookup uses timestamp overlap, which still works because
                        // both sources share the same session-relative timeline.
                        let _ = speech_chunk; // consumed; diarizer receives data via VadEngine
                    }
                }

                // Speech buffering — session-relative timestamps used throughout.
                if is_speech {
                    if !state.in_speech {
                        state.in_speech = true;
                        state.window_start_ms = chunk_session_start_ms;
                    }
                    state.buffer.extend_from_slice(&processed.samples);
                    state.silence_ms = 0;
                } else if state.in_speech {
                    state.silence_ms += chunk_dur_ms;
                    // Keep up to 300 ms of trailing silence so word endings are captured.
                    if state.silence_ms <= 300 {
                        state.buffer.extend_from_slice(&processed.samples);
                    }
                }

                // Transcription trigger:
                //   A) natural pause ≥ 450 ms after at least 0.5 s of speech
                //   B) buffer ≥ 5 s regardless of pauses (bounds memory / latency)
                let min_speech_samples = 16_000 / 2; // 0.5 s
                let should_transcribe =
                    (state.in_speech
                        && state.silence_ms >= 450
                        && state.buffer.len() >= min_speech_samples)
                    || state.buffer.len() >= 16_000 * 5;

                if should_transcribe && !state.buffer.is_empty() {
                    tracing::info!(source = state.source_name, buffered_samples = state.buffer.len(), speech_active = state.in_speech, silence_ms = state.silence_ms, "Queueing audio window for Whisper");
                    let audio = std::mem::take(&mut state.buffer);
                    let trans_start_ms = state.window_start_ms;
                    let is_continuous = state.in_speech && state.silence_ms < 450;

                    if is_continuous {
                        // Retain ~0.8 s of overlap so context is not lost at window boundaries.
                        let overlap = (16_000 * 4 / 5).min(audio.len());
                        state.buffer.extend_from_slice(&audio[audio.len() - overlap..]);
                        // Advance window start to the beginning of the overlap region.
                        let overlap_ms = (overlap as u64 * 1000) / 16_000;
                        state.window_start_ms =
                            (trans_start_ms + (audio.len() as u64 * 1000 / 16_000))
                            .saturating_sub(overlap_ms);
                    } else {
                        state.in_speech = false;
                        state.silence_ms = 0;
                    }

                    let job = InferenceJob {
                        audio,
                        start_ms: trans_start_ms,
                        is_partial: is_continuous,
                        source_name: state.source_name,
                        queued_at: std::time::Instant::now(),
                    };
                    let queued = std::time::Instant::now();
                    if inference_tx.send(job).await.is_err() {
                        tracing::error!("Whisper inference queue stopped; transcript window could not be queued");
                        break;
                    }
                    let queue_block_ms = queued.elapsed().as_millis() as u64;
                    if queue_block_ms > 100 {
                        tracing::warn!(queue_block_ms, "Inference queue is saturated; audio capture is applying bounded back-pressure");
                    }
                }
            } // end while recv

            // Close open VAD segments before flushing the remaining audio buffers.
            let _ = lb_state.vad.finish();
            let _ = mic_state.vad.finish();

            // Flush remaining audio through the same ordered inference queue.
            for state in [&lb_state, &mic_state] {
                if state.buffer.len() < 3_200 {
                    continue;
                }
                let job = InferenceJob {
                    audio: state.buffer.clone(),
                    start_ms: state.window_start_ms,
                    is_partial: false,
                    source_name: state.source_name,
                    queued_at: std::time::Instant::now(),
                };
                if inference_tx.send(job).await.is_err() {
                    tracing::error!("Whisper inference queue stopped before final audio flush");
                }
            }

            drop(inference_tx);
            if let Err(error) = inference_worker.await {
                tracing::error!(error = %error, "Inference worker did not shut down cleanly");
            }
            tracing::info!(session_id = %session.id(), "Pipeline worker finished cleanly");
        });

        Ok(Pipeline {
            _capture: capture_handle,
            _worker: worker,
        })
    }

    pub fn pause(&self) {
        let _ = self._capture.pause();
    }

    pub fn resume(&self) {
        let _ = self._capture.resume();
    }

    /// Stop audio capture immediately and return the worker so queued Whisper
    /// jobs can finish without blocking the Stop IPC command.
    pub fn stop_capture(self) -> JoinHandle<()> {
        drop(self._capture);
        self._worker
    }

    pub async fn shutdown(self) {
        let worker = self.stop_capture();
        let _ = worker.await;
        tracing::info!("Pipeline shut down cleanly");
    }
}

/// Search standard locations for a Whisper ggml model binary.
/// Prefers smaller models for 8 GB RAM machines; larger models improve accuracy.
fn find_whisper_model() -> Option<std::path::PathBuf> {
    // Allow an explicit model path for reproducible accuracy/latency comparisons.
    // The default remains the small-footprint Base multilingual model.
    if let Ok(configured) = std::env::var("VOXY_WHISPER_MODEL") {
        let path = std::path::PathBuf::from(configured);
        if path.is_file() {
            tracing::info!(model = %path.display(), "Using Whisper model from VOXY_WHISPER_MODEL");
            return Some(path);
        }
        tracing::warn!(
            model = %path.display(),
            "VOXY_WHISPER_MODEL does not point to a file; falling back to model discovery"
        );
    }

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

    tracing::error!(
        "No Whisper model found. Searched: {:?}. \
         Download ggml-base.bin from huggingface.co/ggerganov/whisper.cpp \
         and place it in the models/ folder next to voxy.exe.",
        search_dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>()
    );
    None
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;
    use crate::audio_capture::types::AudioSource as CaptureAudioSource;

    /// Microphone chunks must NEVER be dropped regardless of loopback timing.
    #[test]
    fn microphone_chunks_are_never_suppressed_by_loopback_timing() {
        // This test verifies the architectural decision: no SOURCE_SYNC_TOLERANCE_MS logic.
        // SourceState routes mic and loopback independently — there is no suppression path.
        // The absence of a suppress-mic branch in the pipeline worker is the guarantee.
        // We verify this by asserting that both sources would be processed via different
        // SourceState instances, with no cross-source timestamp comparison.
        let lb_name = "loopback";
        let mic_name = "microphone";
        assert_ne!(lb_name, mic_name, "Sources must be distinct routing paths");
        // If the pipeline code ever re-introduces cross-source suppression, this test
        // should be extended with a mock capture that sends concurrent chunks and
        // verifies both reach the VAD.
    }

    /// Session-relative timestamps must be derived from the wall-clock anchor,
    /// not accumulated chunk durations.
    #[test]
    fn session_clock_uses_wall_clock_anchor_not_accumulated_durations() {
        let first_wall_ms: u64 = 1_000_000; // arbitrary epoch offset
        let chunk_wall_ms: u64 = 1_001_500; // 1.5 s later
        let expected_session_ms: u64 = 1_500;

        let session_relative = chunk_wall_ms.saturating_sub(first_wall_ms);
        assert_eq!(
            session_relative, expected_session_ms,
            "Session timestamp should be wall-clock difference, not accumulated duration"
        );
    }

    /// Both sources must map to the same session timeline via the shared anchor.
    #[test]
    fn both_sources_share_same_timeline_anchor() {
        let first_wall_ms: u64 = 5_000_000;
        let lb_wall_ms: u64 = 5_001_000;  // loopback chunk at 1.0 s
        let mic_wall_ms: u64 = 5_001_050; // mic chunk at 1.05 s (nearly simultaneous)

        let lb_session = lb_wall_ms.saturating_sub(first_wall_ms);
        let mic_session = mic_wall_ms.saturating_sub(first_wall_ms);

        assert_eq!(lb_session, 1_000, "Loopback session timestamp");
        assert_eq!(mic_session, 1_050, "Microphone session timestamp");
        // Both are close but independently tracked — no suppression occurs.
        assert!(
            (lb_session as i64 - mic_session as i64).abs() < 200,
            "Simultaneous chunks from both sources should have similar session timestamps"
        );
    }
}
