use super::export::{export_to_txt, format_as_txt};
use super::manager::TranscriptManager;
use super::models::{SegmentStatus, TranscriptError, TranscriptSegment};
use crate::transcript_processing::{RecognizedChunk, SpeakerInterval, TranscriptProcessor};

fn processor() -> TranscriptProcessor {
    TranscriptProcessor::new("session-one")
}
fn ingest(
    p: &mut TranscriptProcessor,
    text: &str,
    start_ms: u64,
    end_ms: u64,
    is_final: bool,
) -> u64 {
    p.ingest_chunk(RecognizedChunk {
        start_ms,
        end_ms,
        text: text.into(),
        language: Some("en".into()),
        is_final,
    })[0]
        .id
}
fn segment(id: u64, text: &str, start_ms: u64) -> TranscriptSegment {
    TranscriptSegment {
        id,
        text: text.into(),
        start_ms,
        end_ms: start_ms.saturating_add(1000),
        speaker: Some("Speaker 1".into()),
        language: Some("en".into()),
        status: SegmentStatus::Final,
    }
}
struct ExportDirectory(std::path::PathBuf);
impl ExportDirectory {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "voxy_export_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for ExportDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_add_and_edit_transcript() {
    let mut p = processor();
    let id = ingest(&mut p, "Hello", 0, 5000, true);
    let mut manager = TranscriptManager::new(&mut p);
    manager.edit_segment(id, "Hello World").unwrap();
    assert_eq!(manager.get_segments().len(), 1);
    assert_eq!(manager.get_segments()[0].text, "Hello World");
}
#[test]
fn test_multilingual_search() {
    let mut p = processor();
    ingest(&mut p, "வணக்கம் நண்பர்களே", 0, 1000, true);
    assert_eq!(TranscriptManager::new(&mut p).search("வணக்கம்").len(), 1);
}
#[test]
fn test_session_cleanup() {
    let mut p = processor();
    ingest(&mut p, "Old session", 0, 1000, true);
    let mut manager = TranscriptManager::new(&mut p);
    manager.clear();
    assert!(manager.get_segments().is_empty());
    assert!(manager.search("Old").is_empty());
}
#[test]
fn test_txt_export() {
    let dir = ExportDirectory::new();
    let path = dir.0.join("transcript.txt");
    export_to_txt(&[segment(1, "ආයුබෝවන්", 12_000)], &path).unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "VOXY Meeting Transcript\n=======================\n\n[00:12] Speaker 1: ආයුබෝවන්\n"
    );
}
#[test]
fn test_delete_transcript_segment() {
    let mut p = processor();
    let id = ingest(&mut p, "Delete", 0, 1000, true);
    let mut manager = TranscriptManager::new(&mut p);
    manager.delete_segment(id).unwrap();
    assert!(manager.get_segments().is_empty());
    assert_eq!(
        manager.delete_segment(id),
        Err(TranscriptError::NotFound(id))
    );
}
#[test]
fn test_search_by_speaker() {
    let mut p = processor();
    ingest(&mut p, "Meeting", 0, 1000, true);
    p.add_speaker_intervals(vec![SpeakerInterval {
        start_ms: 0,
        end_ms: 1000,
        speaker: "Speaker 2".into(),
    }]);
    let manager = TranscriptManager::new(&mut p);
    assert_eq!(manager.search("speaker 2").len(), 1);
    assert_eq!(manager.search("SPEAKER 2").len(), 1);
    assert!(manager.search("unknown").is_empty());
}
#[test]
fn missing_edit_and_delete_leave_transcript_unchanged() {
    let mut p = processor();
    ingest(&mut p, "Original", 0, 1000, true);
    let before = p.session_transcript();
    let mut manager = TranscriptManager::new(&mut p);
    assert_eq!(
        manager.edit_segment(999, "Changed"),
        Err(TranscriptError::NotFound(999))
    );
    assert_eq!(
        manager.delete_segment(999),
        Err(TranscriptError::NotFound(999))
    );
    assert_eq!(
        manager.edit_segment(0, "Changed"),
        Err(TranscriptError::InvalidId)
    );
    assert_eq!(manager.delete_segment(0), Err(TranscriptError::InvalidId));
    assert_eq!(p.session_transcript(), before);
}
#[test]
fn edit_preserves_metadata_and_accepts_unicode_and_empty_text() {
    let mut p = processor();
    let id = ingest(&mut p, "Original", 12_500, 15_000, false);
    let mut expected = p.session_transcript().segments[0].clone();
    expected.text = "Hello ආයුබෝවන් வணக்கம்\nSecond line".into();
    let mut manager = TranscriptManager::new(&mut p);
    assert_eq!(manager.edit_segment(id, &expected.text).unwrap(), expected);
    expected.text.clear();
    assert_eq!(manager.edit_segment(id, "").unwrap(), expected);
}
#[test]
fn search_matches_unicode_text_and_speakers_without_mutation() {
    let mut p = processor();
    let id = ingest(&mut p, "Hello ආයුබෝවන් வணக்கம்", 0, 1000, true);
    let second = ingest(&mut p, "Discussion", 2000, 3000, true);
    p.add_speaker_intervals(vec![SpeakerInterval {
        start_ms: 2000,
        end_ms: 3000,
        speaker: "කථිකයා பேச்சாளர்".into(),
    }]);
    let before = p.session_transcript();
    let manager = TranscriptManager::new(&mut p);
    for query in ["HELLO", "ආයුබෝවන්", "வணக்கம்"] {
        assert_eq!(manager.search(query)[0].id, id);
    }
    for query in ["කථිකයා", "பேச்சாளர்"] {
        assert_eq!(manager.search(query)[0].id, second);
    }
    assert_eq!(manager.search("").len(), 2);
    assert!(manager.search("missing").is_empty());
    assert_eq!(p.session_transcript(), before);
}
#[test]
fn delete_preserves_remaining_ids_order_and_search_results() {
    let mut p = processor();
    let third = ingest(&mut p, "third", 4000, 5000, true);
    let first = ingest(&mut p, "first", 0, 1000, true);
    let second = ingest(&mut p, "second", 2000, 3000, true);
    let mut manager = TranscriptManager::new(&mut p);
    manager.delete_segment(second).unwrap();
    assert_eq!(
        manager
            .get_segments()
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        [first, third]
    );
    assert!(manager.search("second").is_empty());
    manager.delete_segment(first).unwrap();
    manager.delete_segment(third).unwrap();
    assert!(manager.get_segments().is_empty());
}
#[test]
fn export_reflects_edits_deletions_unicode_and_timestamp_boundaries() {
    let dir = ExportDirectory::new();
    let path = dir.0.join("edited.txt");
    let mut p = processor();
    let first = ingest(&mut p, "Old", 59_999, 60_000, true);
    let deleted = ingest(&mut p, "Deleted", 61_000, 62_000, true);
    ingest(&mut p, "English", 3_600_900, 3_601_000, true);
    p.add_speaker_intervals(vec![SpeakerInterval {
        start_ms: 0,
        end_ms: 4_000_000,
        speaker: "Speaker 1".into(),
    }]);
    let mut manager = TranscriptManager::new(&mut p);
    manager.edit_segment(first, "Hello ආයුබෝවන් வணக்கம்").unwrap();
    manager.delete_segment(deleted).unwrap();
    export_to_txt(manager.get_segments(), &path).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "VOXY Meeting Transcript\n=======================\n\n[00:59] Speaker 1: Hello ආයුබෝවන් வணக்கம்\n[60:00] Speaker 1: English\n");
}
#[test]
fn empty_export_replaces_previous_content_with_header() {
    let dir = ExportDirectory::new();
    let path = dir.0.join("empty.txt");
    std::fs::write(&path, "Previous").unwrap();
    export_to_txt(&[], &path).unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "VOXY Meeting Transcript\n=======================\n\n"
    );
}
#[test]
fn invalid_timestamps_do_not_overwrite_existing_export() {
    let dir = ExportDirectory::new();
    let path = dir.0.join("existing.txt");
    std::fs::write(&path, "Keep this export").unwrap();
    let mut invalid_time = segment(2, "Bad", 1000);
    invalid_time.end_ms = 999;
    for invalid in [
        invalid_time,
        segment(0, "Bad ID", 0),
        segment(1, "Duplicate ID", 0),
    ] {
        let error = export_to_txt(&[segment(1, "Valid", 0), invalid], &path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Keep this export");
    }
}
#[test]
fn export_returns_errors_for_invalid_destinations_without_creating_parents() {
    let dir = ExportDirectory::new();
    let segments = [segment(1, "Hello", 0)];
    let parent = dir.0.join("missing");
    assert_eq!(
        export_to_txt(&segments, &parent.join("out.txt"))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(!parent.exists());
    assert!(export_to_txt(&segments, &dir.0).is_err());
    assert!(export_to_txt(&segments, std::path::Path::new("")).is_err());
    let file = dir.0.join("file.txt");
    std::fs::write(&file, "Keep parent").unwrap();
    assert!(export_to_txt(&segments, &file.join("out.txt")).is_err());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "Keep parent");
}
#[test]
fn txt_formatter_matches_file_export_without_mutating_segments() {
    let dir = ExportDirectory::new();
    let path = dir.0.join("out.txt");
    let segments = [segment(1, "English ආයුබෝවන් வணக்கம்", 60_500)];
    let before = segments.clone();
    let text = format_as_txt(&segments).unwrap();
    assert!(!path.exists());
    export_to_txt(&segments, &path).unwrap();
    assert_eq!(text.as_bytes(), std::fs::read(path).unwrap());
    assert_eq!(segments, before);
    assert!(text.contains("[01:00] Speaker 1: English ආයුබෝවන් வணக்கம்"));
}
#[test]
fn clearing_manager_allows_reuse_without_retaining_old_content_or_removing_export() {
    let dir = ExportDirectory::new();
    let path = dir.0.join("saved.txt");
    let mut p = processor();
    let old = ingest(&mut p, "Previous session", 0, 1000, true);
    export_to_txt(&p.session_transcript().segments, &path).unwrap();
    let saved = std::fs::read(&path).unwrap();
    let mut manager = TranscriptManager::new(&mut p);
    manager.clear();
    manager.clear();
    assert!(manager.edit_segment(old, "Stale").is_err());
    assert!(manager.delete_segment(old).is_err());
    let current = ingest(&mut p, "Current session", 0, 1000, true);
    assert!(current > old);
    assert!(!format_as_txt(&p.session_transcript().segments)
        .unwrap()
        .contains("Previous"));
    assert_eq!(std::fs::read(path).unwrap(), saved);
}
#[test]
fn manual_partial_edit_survives_finalization_and_late_speaker_update() {
    let mut p = processor();
    let id = ingest(&mut p, "Partial", 0, 1000, false);
    TranscriptManager::new(&mut p)
        .edit_segment(id, "  என் edit සිංහල\nHello  ")
        .unwrap();
    let finalized = p.ingest_chunk(RecognizedChunk {
        start_ms: 0,
        end_ms: 1500,
        text: "ASR replacement".into(),
        language: Some("ta".into()),
        is_final: true,
    });
    assert_eq!(finalized[0].id, id);
    assert_eq!(finalized[0].status, SegmentStatus::Final);
    assert_eq!(finalized[0].text, "  என் edit සිංහල\nHello  ");
    let late = p.add_speaker_intervals(vec![SpeakerInterval {
        start_ms: 0,
        end_ms: 1600,
        speaker: "பேச்சாளர்".into(),
    }]);
    assert_eq!(late[0].id, id);
    assert_eq!(late[0].text, finalized[0].text);
    assert_eq!(late[0].speaker.as_deref(), Some("பேச்சாளர்"));
}
#[test]
fn deleted_partial_is_not_resurrected_by_finalization_or_late_speakers() {
    let mut p = processor();
    let id = ingest(&mut p, "Partial", 0, 1000, false);
    TranscriptManager::new(&mut p).delete_segment(id).unwrap();
    assert!(p
        .ingest_chunk(RecognizedChunk {
            start_ms: 0,
            end_ms: 1500,
            text: "Stale final".into(),
            language: None,
            is_final: true
        })
        .is_empty());
    assert!(p
        .add_speaker_intervals(vec![SpeakerInterval {
            start_ms: 0,
            end_ms: 1600,
            speaker: "Speaker 1".into()
        }])
        .is_empty());
    let next = ingest(&mut p, "New utterance", 2000, 3000, true);
    assert!(next > id);
    assert_eq!(p.session_transcript().segments.len(), 1);
}
#[test]
fn optional_speakers_and_maximum_integer_timestamps_format_without_float_loss() {
    let mut unknown = segment(1, "Text", 0);
    unknown.speaker = None;
    let mut blank = segment(2, "Text", 60_000);
    blank.speaker = Some("  ".into());
    let max = segment(3, "End", u64::MAX);
    let content = format_as_txt(&[unknown, blank, max]).unwrap();
    assert!(content.contains("[00:00] Unknown speaker: Text"));
    assert!(content.contains("[01:00] Unknown speaker: Text"));
    let seconds = u64::MAX / 1000;
    assert!(content.contains(&format!(
        "[{:02}:{:02}] Speaker 1: End",
        seconds / 60,
        seconds % 60
    )));
}

#[test]
fn maximum_timestamp_overlap_and_deletion_do_not_overflow() {
    let mut p = processor();
    let id = ingest(&mut p, "Huge", 0, u64::MAX, false);
    assert_eq!(ingest(&mut p, "Final", 0, u64::MAX, true), id);
    TranscriptManager::new(&mut p).delete_segment(id).unwrap();
    assert!(p
        .ingest_chunk(RecognizedChunk {
            start_ms: 0,
            end_ms: u64::MAX,
            text: "Stale".into(),
            language: None,
            is_final: true
        })
        .is_empty());
}

#[test]
fn canonical_schema_rejects_negative_timestamps_at_deserialization() {
    let json = r#"{"id":1,"startMs":-1,"endMs":2,"speaker":null,"text":"Bad","language":null,"status":"final"}"#;
    assert!(serde_json::from_str::<TranscriptSegment>(json).is_err());
}
