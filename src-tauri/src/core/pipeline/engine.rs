//! Existing algorithms composed on one blocking worker; no transcript storage.
use super::adapters::{recognition_to_chunk, speaker_to_interval, RecognitionWindow};
use super::runtime::{Backend, CaptureStream};
use crate::audio_capture::AudioChunk;
use crate::audio_processing::capture_adapter::samples_to_ms;
use crate::audio_processing::{AudioProcessor, ProcessedAudio};
use crate::core::error::{AppError, AppResult};
use crate::core::session::Session;
use crate::speaker_analysis::{DiarizationEngine, VadEngine};
use crate::speech_recognition::RecognitionResult;
use crate::transcript_processing::TranscriptUpdate;
use std::sync::Arc;

pub trait Recognizer: Send {
    fn transcribe(
        &mut self,
        audio: &ProcessedAudio,
        language: &str,
    ) -> AppResult<Vec<RecognitionResult>>;
}
pub trait EventSink: Send + Sync {
    fn update(&self, update: TranscriptUpdate);
    fn failed(&self, error: &AppError, previous: crate::core::types::SessionState);
}

pub struct Engine {
    pub session: Session,
    pub sink: Arc<dyn EventSink>,
    pub capture_factory: Box<dyn FnMut() -> AppResult<CaptureStream> + Send>,
    pub recognizer: Box<dyn Recognizer>,
    pub vad: VadEngine,
    pub diarizer: DiarizationEngine,
    processor: AudioProcessor,
    pending: Vec<f32>,
    window_start: u64,
    active_samples: u64,
    speech_in_window: bool,
}

impl Engine {
    pub fn new(
        session: Session,
        sink: Arc<dyn EventSink>,
        capture_factory: Box<dyn FnMut() -> AppResult<CaptureStream> + Send>,
        recognizer: Box<dyn Recognizer>,
        vad: VadEngine,
        diarizer: DiarizationEngine,
    ) -> Self {
        Self {
            session,
            sink,
            capture_factory,
            recognizer,
            vad,
            diarizer,
            processor: AudioProcessor::new(),
            pending: Vec::with_capacity(16_000),
            window_start: 0,
            active_samples: 0,
            speech_in_window: false,
        }
    }

    fn speakers(&mut self) -> AppResult<()> {
        let end = samples_to_ms(self.active_samples, 16_000)?;
        let intervals = self
            .diarizer
            .drain_new_intervals()
            .into_iter()
            .map(|interval| speaker_to_interval(interval, end))
            .collect::<AppResult<Vec<_>>>()?;
        if !intervals.is_empty() {
            let update = self.session.ingest_speakers(intervals)?;
            if !update.segments.is_empty() {
                self.sink.update(update);
            }
        }
        Ok(())
    }

    fn recognize_pending(&mut self) -> AppResult<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let samples = std::mem::take(&mut self.pending);
        let count = samples.len() as u64;
        if self.speech_in_window {
            let window = RecognitionWindow::from_samples(self.window_start, count, 16_000)?;
            let mut audio = ProcessedAudio::empty_for(16_000, 1);
            audio.samples_per_channel = samples.len();
            audio.samples = samples;
            audio.timestamp_ms = Some(samples_to_ms(self.window_start, 16_000)?);
            audio.timing.start_ms = audio.timestamp_ms;
            audio.timing.end_ms = Some(samples_to_ms(self.window_start + count, 16_000)?);
            let language = self.session.config().language_hint;
            for result in self.recognizer.transcribe(&audio, &language)? {
                let chunk = recognition_to_chunk(result, window)?;
                let update = self.session.ingest_recognition(chunk)?;
                if !update.segments.is_empty() {
                    self.sink.update(update);
                }
            }
        }
        self.window_start = self
            .window_start
            .checked_add(count)
            .ok_or_else(|| AppError::InvalidConfig("Active audio offset overflow".into()))?;
        self.speech_in_window = false;
        self.pending = Vec::with_capacity(16_000);
        Ok(())
    }
}

impl Backend for Engine {
    fn capture(&mut self) -> AppResult<CaptureStream> {
        (self.capture_factory)()
    }
    fn process(&mut self, chunk: AudioChunk) -> AppResult<()> {
        let processed = self.processor.process(chunk)?;
        // Split only window assembly; preprocessing is performed exactly once.
        for samples in processed.samples.chunks(160) {
            let mut block = ProcessedAudio::empty_for(16_000, 1);
            block.samples = samples.to_vec();
            block.samples_per_channel = samples.len();
            self.active_samples = self
                .active_samples
                .checked_add(samples.len() as u64)
                .ok_or_else(|| AppError::InvalidConfig("Active sample count overflow".into()))?;
            let speech = self.vad.is_speech(&block)?;
            let completed = !self.vad.take_speech_segments().is_empty();
            self.speech_in_window |= speech || completed;
            self.pending.extend_from_slice(samples);
            self.speakers()?;
            if self.pending.len() >= 16_000 {
                self.recognize_pending()?;
            }
        }
        Ok(())
    }
    fn finish(&mut self) -> AppResult<()> {
        self.vad.finish()?;
        self.speech_in_window |= !self.vad.take_speech_segments().is_empty();
        self.speakers()?;
        self.recognize_pending()?;
        self.speakers()
    }
    fn failed(&mut self, error: &AppError) {
        let previous = self.session.state();
        self.session.fail();
        self.sink.failed(error, previous);
    }
}

#[cfg(test)]
mod tests {
    include!("engine/tests.rs");
}
