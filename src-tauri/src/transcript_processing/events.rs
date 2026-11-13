//! Async pipeline + event delivery to the frontend.
//! This file does not depend on Tauri. Member 2 implements `TranscriptEmitter`
//! with the real `AppHandle` (see README.md in this folder).

use super::processor::TranscriptProcessor;
use super::types::*;
use tokio::sync::mpsc::Receiver;

/// Name of the Tauri event the frontend listens to.
pub const TRANSCRIPT_UPDATE_EVENT: &str = "transcript:update";

pub trait TranscriptEmitter: Send + Sync {
    fn emit_update(&self, update: &TranscriptUpdate) -> Result<(), String>;
}

/// Runs until the sender side is dropped (session end), then returns the
/// final transcript so the review/export module can use it.
pub async fn run_transcript_pipeline<E: TranscriptEmitter>(
    session_id: String,
    mut rx: Receiver<TranscriptInput>,
    emitter: E,
) -> SessionTranscript {
    let mut processor = TranscriptProcessor::new(session_id.clone());

    while let Some(input) = rx.recv().await {
        let changed = match input {
            TranscriptInput::Recognized(chunk) => processor.ingest_chunk(chunk),
            TranscriptInput::Speakers(intervals) => processor.add_speaker_intervals(intervals),
        };
        if changed.is_empty() {
            continue;
        }
        let update = TranscriptUpdate { session_id: session_id.clone(), segments: changed };
        if let Err(e) = emitter.emit_update(&update) {
            tracing::warn!("failed to emit transcript update: {e}");
        }
    }
    processor.session_transcript()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MockEmitter(Arc<Mutex<Vec<TranscriptUpdate>>>);

    impl TranscriptEmitter for MockEmitter {
        fn emit_update(&self, update: &TranscriptUpdate) -> Result<(), String> {
            self.0.lock().unwrap().push(update.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn pipeline_emits_updates_and_returns_final_transcript() {
        let emitter = MockEmitter::default();
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        tx.send(TranscriptInput::Recognized(RecognizedChunk {
            start_ms: 0, end_ms: 2000, text: "வணக்கம்".into(), language: Some("ta".into()), is_final: true,
        })).await.unwrap();
        tx.send(TranscriptInput::Speakers(vec![SpeakerInterval {
            start_ms: 0, end_ms: 2500, speaker: "Speaker 1".into(),
        }])).await.unwrap();
        drop(tx);

        let result = run_transcript_pipeline("s1".into(), rx, emitter.clone()).await;

        assert_eq!(emitter.0.lock().unwrap().len(), 2); // text update + speaker update
        assert_eq!(result.segments[0].text, "வணக்கம்");
        assert_eq!(result.segments[0].speaker.as_deref(), Some("Speaker 1"));
    }
}
