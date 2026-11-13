use super::*;
#[test]
fn one_processor_preserves_partial_final_ids_and_late_labels() {
    let session = Session::new(SessionConfig::default());
    let partial = session
        .ingest_recognition(RecognizedChunk {
            start_ms: 0,
            end_ms: 1000,
            text: "வணக்கம்".into(),
            language: Some("ta".into()),
            is_final: false,
        })
        .unwrap();
    let final_update = session
        .ingest_recognition(RecognizedChunk {
            start_ms: 0,
            end_ms: 1500,
            text: "வணக்கம் ආයුබෝවන් Hello".into(),
            language: Some("ta".into()),
            is_final: true,
        })
        .unwrap();
    assert_eq!(partial.segments[0].id, final_update.segments[0].id);
    let speaker = session
        .ingest_speakers(vec![SpeakerInterval {
            start_ms: 0,
            end_ms: 1600,
            speaker: "Speaker 1".into(),
        }])
        .unwrap();
    assert_eq!(speaker.segments[0].id, partial.segments[0].id);
    assert_eq!(speaker.segments[0].speaker.as_deref(), Some("Speaker 1"));
    assert_eq!(session.canonical_transcript().segments.len(), 1);
    session.start().unwrap();
    session.pause().unwrap();
    session.resume().unwrap();
    session.begin_stop().unwrap();
    session.finish_stop().unwrap();
    assert_eq!(session.canonical_transcript().segments, speaker.segments);
    assert_eq!(session.transcript()[0].end_ms, 1500);
}
#[test]
fn invalid_transitions_leave_state_and_transcript_intact() {
    let session = Session::new(SessionConfig::default());
    assert!(session.pause().is_err());
    assert_eq!(session.state(), SessionState::Initializing);
    session.fail();
    session.begin_stop().unwrap();
    session.finish_stop().unwrap();
    assert!(session.start().is_err());
    assert_eq!(session.state(), SessionState::Stopped);
}

#[test]
fn review_operations_validate_ids_states_and_timestamp_input() {
    let session = Session::new(SessionConfig::default());
    assert_eq!(
        session.edit_transcript_segment(1, "No").unwrap_err().code(),
        "TRANSCRIPT_OPERATION_UNAVAILABLE"
    );
    assert_eq!(
        session.transcript_segment(0).unwrap_err().code(),
        "INVALID_TRANSCRIPT"
    );
    session.start().unwrap();
    assert_eq!(
        session.edit_transcript_segment(0, "No").unwrap_err().code(),
        "INVALID_TRANSCRIPT"
    );
    assert_eq!(
        session.delete_transcript_segment(99).unwrap_err().code(),
        "TRANSCRIPT_SEGMENT_NOT_FOUND"
    );
    assert!(session
        .ingest_recognition(RecognizedChunk {
            start_ms: 10,
            end_ms: 9,
            text: "Bad".into(),
            language: None,
            is_final: true
        })
        .is_err());
    assert!(session
        .ingest_speakers(vec![SpeakerInterval {
            start_ms: 2,
            end_ms: 1,
            speaker: "Bad".into()
        }])
        .is_err());
    assert!(session.transcript().is_empty());
    session
        .ingest_recognition(RecognizedChunk {
            start_ms: 0,
            end_ms: 1000,
            text: "Original".into(),
            language: Some("en".into()),
            is_final: true,
        })
        .unwrap();
    session.begin_stop().unwrap();
    assert_eq!(
        session.delete_transcript_segment(1).unwrap_err().code(),
        "TRANSCRIPT_OPERATION_UNAVAILABLE"
    );
    session.finish_stop().unwrap();
    session
        .edit_transcript_segment(1, "Edited ආයුබෝවන් வணக்கம்")
        .unwrap();
    assert_eq!(session.search_transcript("EDITED").len(), 1);
    assert!(session
        .transcript_text()
        .unwrap()
        .contains("Edited ආයුබෝවන් வணக்கம்"));
    assert!(session
        .ingest_recognition(RecognizedChunk {
            start_ms: 2000,
            end_ms: 3000,
            text: "Stale".into(),
            language: None,
            is_final: true
        })
        .is_err());
    assert!(session
        .ingest_speakers(vec![SpeakerInterval {
            start_ms: 0,
            end_ms: 1000,
            speaker: "Stale".into()
        }])
        .is_err());
    session.delete_transcript_segment(1).unwrap();
    assert!(session.transcript().is_empty());
}
