use super::*;
use crate::audio_capture::types::AudioSource;
use crate::core::types::SessionConfig;
use crate::speaker_analysis::{
    create_engines_with, DiarizationConfig, EnergyVad, SpeakerEmbedder, VadConfig,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

#[derive(Default)]
struct Sink(Mutex<Vec<TranscriptUpdate>>);
impl EventSink for Sink {
    fn update(&self, update: TranscriptUpdate) {
        self.0.lock().unwrap().push(update);
    }
    fn failed(&self, _: &AppError, _: crate::core::types::SessionState) {}
}
struct TestRecognizer(Arc<AtomicUsize>);
impl Recognizer for TestRecognizer {
    fn transcribe(&mut self, audio: &ProcessedAudio, _: &str) -> AppResult<Vec<RecognitionResult>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(vec![RecognitionResult {
            text: "Hello ආයුබෝවන් வணக்கம்".into(),
            language: "ta".into(),
            start_ms: 0,
            end_ms: audio.samples.len() as u64 * 1000 / 16_000,
            is_partial: false,
        }])
    }
}
struct Embedder;
impl SpeakerEmbedder for Embedder {
    fn name(&self) -> &'static str {
        "test-only"
    }
    fn embed(&mut self, _: &[f32]) -> AppResult<Vec<f32>> {
        Ok(vec![1.0, 0.0])
    }
}
fn engine() -> (Engine, Arc<Sink>, Arc<AtomicUsize>) {
    let session = Session::new(SessionConfig::default());
    let sink = Arc::new(Sink::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let (vad, diarizer) = create_engines_with(
        Box::new(EnergyVad::new(VadConfig::default())),
        Some(Box::new(Embedder)),
        DiarizationConfig::default(),
    );
    (
        Engine::new(
            session,
            sink.clone(),
            Box::new(|| Err(AppError::AudioCaptureFailed("not used".into()))),
            Box::new(TestRecognizer(calls.clone())),
            vad,
            diarizer,
        ),
        sink,
        calls,
    )
}
#[test]
fn active_audio_offsets_final_tail_and_late_speaker_update_share_one_processor() {
    let (mut engine, sink, calls) = engine();
    for epoch in [0, 1_000_000] {
        engine
            .process(AudioChunk::new(
                vec![0.25; 16_000],
                16_000,
                epoch,
                AudioSource::Loopback,
            ))
            .unwrap();
    }
    engine
        .process(AudioChunk::new(
            vec![0.25; 8000],
            16_000,
            2_000_000,
            AudioSource::Loopback,
        ))
        .unwrap();
    engine.finish().unwrap();
    let transcript = engine.session.canonical_transcript();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(transcript.segments.len(), 3);
    assert_eq!(
        transcript
            .segments
            .iter()
            .map(|s| (s.start_ms, s.end_ms))
            .collect::<Vec<_>>(),
        [(0, 1000), (1000, 2000), (2000, 2500)]
    );
    assert!(transcript
        .segments
        .iter()
        .all(|s| s.speaker.as_deref() == Some("Speaker 1")));
    assert!(transcript
        .segments
        .iter()
        .all(|s| s.text == "Hello ආයුබෝවන් வணக்கம்"));
    assert!(sink.0.lock().unwrap().len() > 3); // late labels update existing IDs
    engine.finish().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}
#[test]
fn silence_advances_active_clock_without_inference_or_duplicate_processing() {
    let (mut engine, _, calls) = engine();
    engine
        .process(AudioChunk::new(
            vec![0.0; 16_000],
            16_000,
            0,
            AudioSource::Microphone,
        ))
        .unwrap();
    engine
        .process(AudioChunk::new(
            vec![0.25; 16_000],
            16_000,
            u64::MAX,
            AudioSource::Microphone,
        ))
        .unwrap();
    engine.finish().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        engine.session.canonical_transcript().segments[0].start_ms,
        1000
    );
}
