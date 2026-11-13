//! Native wiring for the dedicated audio runtime. Algorithms remain in their modules.
pub mod adapters;
pub mod engine;
pub mod runtime;

use crate::audio_capture::{AudioCaptureManager, CaptureConfig, CaptureHandle};
use crate::audio_processing::ProcessedAudio;
use crate::core::{
    error::{AppError, AppResult},
    session::Session,
    types::SessionConfig,
};
use crate::speaker_analysis::{create_engines, SpeakerAnalysisConfig};
use crate::speech_recognition::{RecognitionResult, WhisperEngine};
use crate::transcript_processing::TranscriptUpdate;
use engine::{Engine, EventSink, Recognizer};
pub use runtime::Runtime as Pipeline;
use runtime::{CaptureControl, CaptureStream};
use std::sync::Arc;
use tauri::AppHandle;

impl CaptureControl for CaptureHandle {
    fn request_stop(&self) {
        CaptureHandle::request_stop(self);
    }
}
impl Recognizer for WhisperEngine {
    fn transcribe(
        &mut self,
        audio: &ProcessedAudio,
        language: &str,
    ) -> AppResult<Vec<RecognitionResult>> {
        WhisperEngine::transcribe(self, audio, language)
    }
}
struct TauriSink {
    app: AppHandle,
    session_id: String,
    show_partial_results: bool,
}
impl EventSink for TauriSink {
    fn update(&self, mut update: TranscriptUpdate) {
        if !self.show_partial_results {
            update.segments.retain(|segment| {
                segment.status == crate::transcript_processing::SegmentStatus::Final
            });
        }
        if !update.segments.is_empty() {
            crate::core::events::emit_transcript_update(&self.app, update);
        }
    }
    fn failed(&self, error: &AppError, previous: crate::core::types::SessionState) {
        crate::core::events::emit_error(&self.app, Some(&self.session_id), error);
        crate::core::events::emit_session_state_changed(
            &self.app,
            &self.session_id,
            previous,
            crate::core::types::SessionState::Error,
            Some(error.to_string()),
        );
    }
}

/// Called from a blocking command task. The factory and all inference run on
/// Runtime's dedicated thread; neither model loading nor WASAPI joins block Tokio.
pub fn start(
    config: SessionConfig,
    session: Session,
    app: AppHandle,
    models: crate::core::config::ModelConfig,
) -> AppResult<Pipeline> {
    let source = adapters::single_capture_source(&config.audio_source)?;
    Pipeline::start(move || {
        let model_path = models.whisper_model_path.unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_default()
                .join("models/ggml-base.bin")
        });
        let whisper = WhisperEngine::new(&model_path)?;
        if !whisper.is_loaded() {
            return Err(AppError::ModelNotFound(model_path.display().to_string()));
        }
        let mut analysis = SpeakerAnalysisConfig::default();
        if let Some(path) = models.silero_model_path {
            // Joining an absolute model path preserves the embedding model directory.
            analysis.models.vad_model = path.to_string_lossy().into_owned();
        }
        let (vad, diarizer) = create_engines(&analysis, config.enable_diarization)?;
        let sink = Arc::new(TauriSink {
            app,
            session_id: session.id(),
            show_partial_results: config.show_partial_results,
        });
        let capture_factory = Box::new(move || {
            let device = config
                .device_id
                .clone()
                .map(crate::audio_capture::config::DeviceSelection::ById)
                .unwrap_or_default();
            let capture_config = CaptureConfig {
                enable_loopback: source == crate::audio_capture::types::AudioSource::Loopback,
                enable_microphone: source == crate::audio_capture::types::AudioSource::Microphone,
                loopback_device: device.clone(),
                microphone_device: device,
                ..CaptureConfig::default()
            };
            let (handle, receiver) = AudioCaptureManager::start(capture_config)
                .map_err(|error| AppError::AudioCaptureFailed(error.to_string()))?;
            Ok(CaptureStream {
                receiver,
                control: Box::new(handle),
            })
        });
        Ok(Box::new(Engine::new(
            session,
            sink,
            capture_factory,
            Box::new(whisper),
            vad,
            diarizer,
        )))
    })
}
