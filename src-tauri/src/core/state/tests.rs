use super::*;
use crate::audio_capture::AudioChunk;
use crate::core::pipeline::runtime::{Backend, CaptureControl, CaptureStream};
use crate::transcript_processing::RecognizedChunk;

struct HoldSender(Mutex<Option<tokio::sync::mpsc::Sender<AudioChunk>>>);
impl CaptureControl for HoldSender {
    fn request_stop(&self) {
        self.0.lock().take();
    }
}
struct BackendForSession(Session);
impl Backend for BackendForSession {
    fn capture(&mut self) -> AppResult<CaptureStream> {
        let (tx, receiver) = tokio::sync::mpsc::channel(1);
        Ok(CaptureStream {
            receiver,
            control: Box::new(HoldSender(Mutex::new(Some(tx)))),
        })
    }
    fn process(&mut self, _: AudioChunk) -> AppResult<()> {
        Ok(())
    }
    fn finish(&mut self) -> AppResult<()> {
        self.0.ingest_recognition(RecognizedChunk {
            start_ms: 0,
            end_ms: 100,
            text: "Final tail".into(),
            language: Some("en".into()),
            is_final: true,
        })?;
        Ok(())
    }
    fn failed(&mut self, _: &AppError) {
        self.0.fail();
    }
}
pub(crate) fn state() -> AppState {
    AppState {
        lifecycle: Mutex::new(()),
        runtimes: Mutex::new(HashMap::new()),
        sessions: RwLock::new(HashMap::new()),
        active_session_id: RwLock::new(None),
        review_session_id: RwLock::new(None),
        config: RwLock::new(AppConfig::default()),
    }
}
#[test]
fn runtime_lifecycle_keeps_stopped_review_until_successful_replacement() {
    let state = state();
    let session = state.create_session(SessionConfig::default()).unwrap();
    let id = session.id();
    state
        .start_runtime(&id, |session| {
            Pipeline::start(move || Ok(Box::new(BackendForSession(session))))
        })
        .unwrap();
    assert!(state.create_session(SessionConfig::default()).is_err());
    assert!(state.start_runtime(&id, |_| unreachable!()).is_err());
    state.pause_runtime(&id).unwrap();
    assert!(state.pause_runtime(&id).is_err());
    state.resume_runtime(&id).unwrap();
    assert!(state.resume_runtime(&id).is_err());
    state.pause_runtime(&id).unwrap();
    state.stop_session(&id).unwrap();
    assert_eq!(session.state(), SessionState::Stopped);
    assert_eq!(
        state
            .get_session(&id)
            .unwrap()
            .canonical_transcript()
            .segments[0]
            .text,
        "Final tail"
    );
    assert!(state.active_session_id().is_none());
    let mut both = SessionConfig::default();
    both.audio_source = super::super::types::AudioSource::Both;
    assert!(state.create_session(both).is_err());
    assert!(state.get_session(&id).is_ok());
    let next = state.create_session(SessionConfig::default()).unwrap();
    assert!(next.canonical_transcript().segments.is_empty());
    assert!(state.get_session(&id).is_ok());
    let next_id = next.id();
    state
        .start_runtime(&next_id, |session| {
            Pipeline::start(move || Ok(Box::new(BackendForSession(session))))
        })
        .unwrap();
    assert!(state.get_session(&id).is_err());
    state.stop_session(&next_id).unwrap();
    assert!(state.runtimes.lock().is_empty());
}
#[test]
fn failed_start_keeps_initializing_session_available_for_retry_or_stop() {
    let state = state();
    let session = state.create_session(SessionConfig::default()).unwrap();
    let id = session.id();
    assert!(state
        .start_runtime(&id, |_| Err(AppError::ModelNotFound("missing".into())))
        .is_err());
    assert_eq!(session.state(), SessionState::Initializing);
    assert!(state.runtimes.lock().is_empty());
    state.stop_session(&id).unwrap();
    assert_eq!(session.state(), SessionState::Stopped);
}

#[test]
fn failed_replacement_start_preserves_previous_transcript_until_retry_succeeds() {
    let state = state();
    let previous = state.create_session(SessionConfig::default()).unwrap();
    let previous_id = previous.id();
    state
        .start_runtime(&previous_id, |session| {
            Pipeline::start(move || Ok(Box::new(BackendForSession(session))))
        })
        .unwrap();
    state.stop_session(&previous_id).unwrap();
    let expected = previous.transcript_text().unwrap();
    let candidate = state.create_session(SessionConfig::default()).unwrap();
    let candidate_id = candidate.id();
    assert!(state
        .start_runtime(&candidate_id, |_| {
            Err(AppError::ModelNotFound("missing".into()))
        })
        .is_err());
    assert_eq!(
        state
            .get_session(&previous_id)
            .unwrap()
            .transcript_text()
            .unwrap(),
        expected
    );
    assert!(candidate.transcript().is_empty());
    state
        .start_runtime(&candidate_id, |session| {
            Pipeline::start(move || Ok(Box::new(BackendForSession(session))))
        })
        .unwrap();
    assert!(state.get_session(&previous_id).is_err());
    state.stop_session(&candidate_id).unwrap();
}

struct FailingFinish(Session);
impl Backend for FailingFinish {
    fn capture(&mut self) -> AppResult<CaptureStream> {
        BackendForSession(self.0.clone()).capture()
    }
    fn process(&mut self, _: AudioChunk) -> AppResult<()> {
        Ok(())
    }
    fn finish(&mut self) -> AppResult<()> {
        Err(AppError::SpeechRecognitionError(
            "final inference failed".into(),
        ))
    }
    fn failed(&mut self, _: &AppError) {
        self.0.fail();
    }
}
#[test]
fn failed_final_inference_still_releases_runtime_and_active_slot() {
    let state = state();
    let session = state.create_session(SessionConfig::default()).unwrap();
    let id = session.id();
    state
        .start_runtime(&id, |session| {
            Pipeline::start(move || Ok(Box::new(FailingFinish(session))))
        })
        .unwrap();
    assert!(state
        .stop_session(&id)
        .unwrap_err()
        .to_string()
        .contains("final inference failed"));
    assert_eq!(session.state(), SessionState::Stopped);
    assert!(state.active_session_id().is_none());
    assert!(state.runtimes.lock().is_empty());
    assert!(state.create_session(SessionConfig::default()).is_ok());
}

#[test]
fn transcript_review_export_and_missing_sessions_follow_replacement() {
    let state = state();
    assert_eq!(
        state.get_session("missing").unwrap_err().code(),
        "SESSION_NOT_FOUND"
    );
    let session = state.create_session(SessionConfig::default()).unwrap();
    let id = session.id();
    state
        .start_runtime(&id, |session| {
            Pipeline::start(move || Ok(Box::new(BackendForSession(session))))
        })
        .unwrap();
    state.stop_session(&id).unwrap();
    let retained = state.get_session(&id).unwrap();
    retained
        .edit_transcript_segment(1, "தமிழ் සිංහල English")
        .unwrap();
    let path = std::env::temp_dir().join(format!("voxy-review-{}.txt", id));
    retained.export_transcript(&path).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        retained.transcript_text().unwrap()
    );
    let mut invalid = SessionConfig::default();
    invalid.audio_source = super::super::types::AudioSource::Both;
    assert!(state.create_session(invalid).is_err());
    assert_eq!(
        state
            .get_session(&id)
            .unwrap()
            .search_transcript("English")
            .len(),
        1
    );
    let next = state.create_session(SessionConfig::default()).unwrap();
    assert!(state.get_session(&id).is_ok());
    assert!(next.transcript().is_empty());
    next.ingest_recognition(RecognizedChunk {
        start_ms: 0,
        end_ms: 1000,
        text: "Fresh partial".into(),
        language: None,
        is_final: false,
    })
    .unwrap();
    next.ingest_recognition(RecognizedChunk {
        start_ms: 0,
        end_ms: 1000,
        text: "Fresh final".into(),
        language: None,
        is_final: true,
    })
    .unwrap();
    assert_eq!(next.transcript()[0].text, "Fresh final");
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains("தமிழ் සිංහල English"));
    std::fs::remove_file(path).unwrap();
}
