use super::*;
use crate::transcript_processing::{RecognizedChunk, SpeakerInterval};

struct ExportDir(std::path::PathBuf);
impl Drop for ExportDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> (AppState, String, ExportDir) {
    let state = crate::core::state::tests::state();
    let session = state.create_session(SessionConfig::default()).unwrap();
    let id = session.id();
    for (i, text, language) in [(0, "English", "en"), (1, "සිංහල", "si"), (2, "தமிழ்", "ta")]
    {
        session
            .ingest_recognition(RecognizedChunk {
                start_ms: i * 1000,
                end_ms: (i + 1) * 1000,
                text: text.into(),
                language: Some(language.into()),
                is_final: true,
            })
            .unwrap();
    }
    session
        .ingest_speakers(vec![SpeakerInterval {
            start_ms: 0,
            end_ms: 3000,
            speaker: "Speaker 1".into(),
        }])
        .unwrap();
    state.stop_session(&id).unwrap();
    let dir = std::env::temp_dir().join(format!("voxy-command-{}", id));
    std::fs::create_dir(&dir).unwrap();
    (state, id, ExportDir(dir))
}

#[test]
fn explicit_txt_save_uses_latest_session_edits_and_the_copy_formatter() {
    let (state, id, dir) = fixture();
    let session = state.get_session(&id).unwrap();
    session
        .edit_transcript_segment(1, "Edited English ආයුබෝවන් வணக்கம்")
        .unwrap();
    session.delete_transcript_segment(2).unwrap();
    let path = dir.0.join("සිංහල-தமிழ்.txt");
    let destination = path.to_str().unwrap();
    assert_eq!(
        save_transcript_file(&state, &id, "txt", destination).unwrap(),
        destination
    );
    let content = std::fs::read_to_string(path).unwrap();
    assert_eq!(content, session.transcript_text().unwrap());
    assert_eq!(content, "VOXY Meeting Transcript\n=======================\n\n[00:00] Speaker 1: Edited English ආයුබෝවන් வணக்கம்\n[00:02] Speaker 1: தமிழ்\n");
}

#[test]
fn save_rejects_missing_sessions_formats_and_paths_without_creating_history() {
    let (state, id, dir) = fixture();
    let path = dir.0.join("out.txt");
    let destination = path.to_str().unwrap();
    assert_eq!(
        save_transcript_file(&state, "missing", "txt", destination)
            .unwrap_err()
            .code(),
        "SESSION_NOT_FOUND"
    );
    for format in ["json", "pdf", ""] {
        assert_eq!(
            save_transcript_file(&state, &id, format, destination)
                .unwrap_err()
                .code(),
            "INVALID_TRANSCRIPT"
        );
    }
    for invalid in ["", "relative.txt"] {
        assert_eq!(
            save_transcript_file(&state, &id, "txt", invalid)
                .unwrap_err()
                .code(),
            "INVALID_TRANSCRIPT"
        );
    }
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
    let missing = dir.0.join("missing").join("out.txt");
    assert_eq!(
        save_transcript_file(&state, &id, "txt", missing.to_str().unwrap())
            .unwrap_err()
            .code(),
        "IO_ERROR"
    );
    assert!(!missing.parent().unwrap().exists());
    assert_eq!(
        save_transcript_file(&state, &id, "txt", dir.0.to_str().unwrap())
            .unwrap_err()
            .code(),
        "IO_ERROR"
    );
}

#[test]
fn transcript_errors_follow_existing_ipc_error_codes() {
    let (state, id, _) = fixture();
    let error = state
        .get_session(&id)
        .unwrap()
        .edit_transcript_segment(999, "Missing")
        .unwrap_err();
    assert!(error.is_recoverable());
    let result: IpcResult<super::super::types::TranscriptSegment> =
        Err::<super::super::types::TranscriptSegment, _>(error).into();
    // Tagged result envelope matches frontend/src/lib/ipc.ts.
    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["Err"]["code"], "TRANSCRIPT_SEGMENT_NOT_FOUND");
    assert!(json["Err"]["message"].as_str().unwrap().contains("999"));
}
