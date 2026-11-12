//! Whisper inference engine backed by whisper-rs.
//!
//! [`WhisperEngine`] wraps a loaded [`whisper_rs::WhisperContext`] and exposes
//! a simple `transcribe` API that the pipeline calls for each audio chunk.
//!
//! When no model file is available the engine degrades gracefully to a stub
//! that returns empty results — this lets the rest of the application compile
//! and run even before a model has been downloaded.

use std::path::Path;
use std::sync::Mutex;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

use crate::audio_processing::ProcessedAudio;
use crate::core::error::{AppError, AppResult};

use super::config::WhisperModelConfig;
use super::RecognitionResult;

/// Handle to the Whisper inference engine.
///
/// Holds the loaded model context and the configuration used to create it.
/// The `WhisperContext` is `Send + Sync`, so the engine can be shared across
/// threads via `Arc<WhisperEngine>`.
pub struct WhisperEngine {
    /// Loaded model context. `None` when running in stub mode (no model file).
    context: Option<WhisperContext>,
    /// Configuration used to load the model.
    config: WhisperModelConfig,
}

impl WhisperEngine {
    /// Load a whisper.cpp model from the given path.
    ///
    /// If the path is empty or the file does not exist, the engine enters
    /// **stub mode**: `transcribe` will return an empty vector.  This allows
    /// the pipeline to start without crashing when no model has been
    /// downloaded yet.
    pub fn new(model_path: &Path) -> AppResult<Self> {
        // Stub mode: empty or nonexistent path.
        if model_path.as_os_str().is_empty() || !model_path.is_file() {
            tracing::warn!(
                path = ?model_path,
                "Whisper model not found — engine running in stub mode"
            );
            return Ok(Self {
                context: None,
                config: WhisperModelConfig::new(model_path),
            });
        }

        tracing::info!(path = ?model_path, "Loading Whisper model");

        let params = WhisperContextParameters::default();
        let context = WhisperContext::new_with_params(model_path, params)
            .map_err(|e| AppError::SpeechRecognitionError(format!(
                "Failed to load Whisper model: {e}"
            )))?;

        let multilingual = context.is_multilingual();
        tracing::info!(
            multilingual,
            "Whisper model loaded successfully"
        );

        Ok(Self {
            context: Some(context),
            config: WhisperModelConfig::new(model_path),
        })
    }

    /// Create a stub engine with no model loaded.
    ///
    /// `transcribe` on a stub engine always returns an empty vector.
    pub fn stub() -> Self {
        Self {
            context: None,
            config: WhisperModelConfig::default(),
        }
    }

    /// Returns `true` when a real model is loaded.
    pub fn is_loaded(&self) -> bool {
        self.context.is_some()
    }

    /// Transcribe processed audio into recognition results.
    ///
    /// The `language_hint` parameter accepts `"auto"`, `"si"`, `"ta"`, or `"en"`.
    /// When the engine is in stub mode, an empty vector is returned.
    pub fn transcribe(
        &self,
        audio: &ProcessedAudio,
        language_hint: &str,
    ) -> AppResult<Vec<RecognitionResult>> {
        // Stub mode: nothing to transcribe.
        let context = match &self.context {
            Some(ctx) => ctx,
            None => return Ok(Vec::new()),
        };

        // Whisper requires 16 kHz mono f32 PCM.
        if audio.sample_rate != 16_000 {
            return Err(AppError::SpeechRecognitionError(format!(
                "Expected 16 kHz audio, got {} Hz",
                audio.sample_rate
            )));
        }

        if audio.samples.is_empty() {
            return Ok(Vec::new());
        }

        // Create a fresh state for this inference call.
        let mut state = context
            .create_state()
            .map_err(|e| AppError::SpeechRecognitionError(format!(
                "Failed to create Whisper state: {e}"
            )))?;

        // Build inference parameters.
        let strategy = SamplingStrategy::Greedy { best_of: 1 };
        let mut params = FullParams::new(strategy);

        // Language configuration.
        let lang = language_hint.trim();
        if lang.is_empty() || lang.eq_ignore_ascii_case("auto") {
            params.set_language(None);
            params.set_detect_language(true);
        } else {
            params.set_language(Some(lang));
        }

        // Always transcribe (never translate) to preserve original scripts.
        params.set_translate(false);

        // Thread count.
        let n_threads = if self.config.n_threads > 0 {
            self.config.n_threads as i32
        } else {
            std::thread::available_parallelism()
                .map(|n| n.get() as i32)
                .unwrap_or(4)
        };
        params.set_n_threads(n_threads);

        // Suppress Whisper's own stderr output.
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);

        // Run the full transcription pipeline.
        tracing::debug!(
            samples = audio.samples.len(),
            language_hint = lang,
            "Running Whisper inference"
        );

        state
            .full(params, &audio.samples)
            .map_err(|e| AppError::SpeechRecognitionError(format!(
                "Whisper inference failed: {e}"
            )))?;

        // Extract recognized language from the state.
        let detected_lang_id = state.full_lang_id_from_state();
        let detected_lang = whisper_rs::get_lang_str(detected_lang_id)
            .unwrap_or("en")
            .to_string();

        // Collect segments.
        let n_segments = state.full_n_segments();
        let mut results = Vec::with_capacity(n_segments as usize);

        for i in 0..n_segments {
            let segment = match state.get_segment(i) {
                Some(seg) => seg,
                None => continue,
            };

            // Skip segments with high no-speech probability.
            let no_speech_prob = segment.no_speech_probability();
            if no_speech_prob > 0.8 {
                tracing::debug!(
                    segment = i,
                    no_speech_prob,
                    "Skipping segment — likely non-speech"
                );
                continue;
            }

            let text = segment
                .to_str_lossy()
                .map(|cow| cow.trim().to_string())
                .unwrap_or_default();

            if text.is_empty() {
                continue;
            }

            // Timestamps are in centiseconds (10 ms units).
            let start_ms = (segment.start_timestamp() * 10) as u64;
            let end_ms = (segment.end_timestamp() * 10) as u64;

            results.push(RecognitionResult {
                text,
                language: detected_lang.clone(),
                start_ms,
                end_ms,
                is_partial: false,
            });
        }

        tracing::debug!(
            segments = results.len(),
            language = %detected_lang,
            "Whisper transcription complete"
        );

        Ok(results)
    }
}

// SAFETY: `WhisperContext` is `Send + Sync` (confirmed in whisper-rs docs).
// We wrap it in a struct that is therefore also `Send + Sync`.
// The `Mutex` is not needed because `WhisperContext` itself is sync,
// but we keep the engine behind `Arc` in the pipeline for shared access.
unsafe impl Send for WhisperEngine {}
unsafe impl Sync for WhisperEngine {}
