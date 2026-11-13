
use super::manager::TranscriptManager;
use super::models::TranscriptSegment;
use super::export::export_to_txt;

#[test]
fn test_add_and_edit_transcript() {
    let mut manager = TranscriptManager::new();

    let segment = TranscriptSegment::new(
        "1".to_string(),
        "Speaker 1".to_string(),
        "Hello".to_string(),
        0.0,
        5.0,
        "en".to_string(),
    );

    manager.add_segment(segment);

    assert_eq!(manager.get_segments().len(), 1);

    manager.edit_segment("1", "Hello World").unwrap();

    assert_eq!(
        manager.get_segments()[0].text,
        "Hello World"
    );
}

#[test]
fn test_multilingual_search() {
    let mut manager = TranscriptManager::new();

    manager.add_segment(TranscriptSegment::new(
        "2".to_string(),
        "Speaker 2".to_string(),
        "வணக்கம் நண்பர்களே".to_string(),
        5.0,
        10.0,
        "ta".to_string(),
    ));

    assert_eq!(manager.search("வணக்கம்").len(), 1);
}

#[test]
fn test_session_cleanup() {
    let mut manager = TranscriptManager::new();

    manager.add_segment(TranscriptSegment::new(
        "3".to_string(),
        "Speaker 1".to_string(),
        "Test transcript".to_string(),
        0.0,
        3.0,
        "en".to_string(),
    ));

    manager.clear();

    assert!(manager.get_segments().is_empty());
}

#[test]
fn test_txt_export() {
    let segments = vec![TranscriptSegment::new(
        "4".to_string(),
        "Speaker 1".to_string(),
        "ආයුබෝවන්".to_string(),
        12.0,
        15.0,
        "si".to_string(),
    )];

    let file_path = std::env::temp_dir().join(format!(
        "voxy_test_{}.txt",
        std::process::id()
    ));

    export_to_txt(&segments, &file_path).unwrap();

    let content = std::fs::read_to_string(&file_path).unwrap();

    assert!(content.contains("ආයුබෝවන්"));
    assert!(content.contains("[00:12]"));

    std::fs::remove_file(&file_path).unwrap();
}
#[test]
fn test_delete_transcript_segment() {
    let mut manager = TranscriptManager::new();

    manager.add_segment(TranscriptSegment::new(
        "5".to_string(),
        "Speaker 1".to_string(),
        "Delete this transcript".to_string(),
        0.0,
        5.0,
        "en".to_string(),
    ));

    assert_eq!(manager.get_segments().len(), 1);

    manager.delete_segment("5").unwrap();

    assert!(manager.get_segments().is_empty());
    assert!(manager.delete_segment("5").is_err());
}
#[test]
fn test_search_by_speaker() {
    let mut manager = TranscriptManager::new();

    manager.add_segment(TranscriptSegment::new(
        "6".to_string(),
        "Speaker 2".to_string(),
        "Meeting discussion".to_string(),
        0.0,
        5.0,
        "en".to_string(),
    ));

    assert_eq!(manager.search("speaker 2").len(), 1);
    assert_eq!(manager.search("SPEAKER 2").len(), 1);
    assert_eq!(manager.search("unknown").len(), 0);
}

// Each export test owns a separate directory, including when tests run in parallel.
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
            NEXT.fetch_add(1, Ordering::Relaxed),
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

fn sample_segment(id: &str, text: &str, start_time: f64) -> TranscriptSegment {
    TranscriptSegment::new(
        id.into(), "Speaker 1".into(), text.into(), start_time,
        start_time + 1.0, "en".into(),
    )
}

#[test]
fn missing_edit_and_delete_leave_transcript_unchanged() {
    let mut manager = TranscriptManager::new();
    manager.add_segment(sample_segment("one", "Original", 0.0));
    let before = manager.get_segments().to_vec();
    assert!(manager.edit_segment("missing", "Replacement").is_err());
    assert!(manager.delete_segment("missing").is_err());
    assert_eq!(manager.get_segments(), before);
}

#[test]
fn edit_preserves_metadata_and_accepts_unicode_and_empty_text() {
    let mut manager = TranscriptManager::new();
    let original = sample_segment("one", "Original", 12.5);
    manager.add_segment(original.clone());
    let mut expected = original;
    expected.text = "Hello ආයුබෝවන් வணக்கம்\nSecond line".into();
    manager.edit_segment("one", &expected.text).unwrap();
    assert_eq!(manager.get_segments(), &[expected.clone()]);
    manager.edit_segment("one", "").unwrap();
    expected.text.clear();
    assert_eq!(manager.get_segments(), &[expected]);
}

#[test]
fn search_matches_unicode_text_and_speakers_without_mutation() {
    let mut manager = TranscriptManager::new();
    manager.add_segment(sample_segment("one", "Hello ආයුබෝවන් வணக்கம்", 0.0));
    let mut second = sample_segment("two", "Discussion", 1.0);
    second.speaker = "කථිකයා பேச்சாளர்".into();
    manager.add_segment(second);
    let before = manager.get_segments().to_vec();
    for query in ["HELLO", "ආයුබෝවන්", "வணக்கம்", "SPEAKER 1"] {
        let matches = manager.search(query);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "one");
    }
    for query in ["කථිකයා", "பேச்சாளர்"] {
        assert_eq!(manager.search(query)[0].id, "two");
    }
    assert_eq!(manager.search("").len(), 2);
    assert!(manager.search("missing").is_empty());
    assert_eq!(manager.get_segments(), before);
}

#[test]
fn delete_preserves_remaining_ids_order_and_search_results() {
    let mut manager = TranscriptManager::new();
    for id in ["one", "two", "three"] {
        manager.add_segment(sample_segment(id, id, 0.0));
    }
    manager.delete_segment("two").unwrap();
    assert_eq!(manager.get_segments().iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["one", "three"]);
    assert!(manager.search("two").is_empty());
    manager.delete_segment("one").unwrap();
    manager.delete_segment("three").unwrap();
    assert!(manager.get_segments().is_empty());
}

#[test]
fn export_reflects_edits_deletions_unicode_and_timestamp_boundaries() {
    let dir = ExportDirectory::new();
    let path = dir.0.join("transcript.txt");
    let mut manager = TranscriptManager::new();
    manager.add_segment(sample_segment("one", "Old text", 59.999));
    manager.add_segment(sample_segment("two", "Deleted text", 60.0));
    manager.add_segment(sample_segment("three", "English", 3600.9));
    manager.edit_segment("one", "Hello ආයුබෝවන් வணக்கம்").unwrap();
    manager.delete_segment("two").unwrap();
    export_to_txt(manager.get_segments(), &path).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(),
        "VOXY Meeting Transcript\n=======================\n\n[00:59] Speaker 1: Hello ආයුබෝවන් வணக்கம்\n[60:00] Speaker 1: English\n");
}

#[test]
fn empty_export_replaces_previous_content_with_header() {
    let dir = ExportDirectory::new();
    let path = dir.0.join("transcript.txt");
    std::fs::write(&path, "Previous transcript").unwrap();
    export_to_txt(&[], &path).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(),
        "VOXY Meeting Transcript\n=======================\n\n");
}

#[test]
fn invalid_timestamps_do_not_overwrite_existing_export() {
    let dir = ExportDirectory::new();
    let path = dir.0.join("transcript.txt");
    std::fs::write(&path, "Keep this export").unwrap();
    for start in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, u64::MAX as f64] {
        // A valid first segment must not cause partial output before validation.
        let segments = [sample_segment("one", "Valid", 0.0), sample_segment("two", "Invalid", start)];
        let error = export_to_txt(&segments, &path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Keep this export");
    }
}

#[test]
fn export_returns_errors_for_invalid_destinations_without_creating_parents() {
    let dir = ExportDirectory::new();
    let segments = [sample_segment("one", "Hello", 60.0)];
    let missing_parent = dir.0.join("missing");
    let error = export_to_txt(&segments, &missing_parent.join("transcript.txt")).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(!missing_parent.exists());
    assert!(export_to_txt(&segments, &dir.0).is_err());
    assert!(export_to_txt(&segments, std::path::Path::new("")).is_err());
    let file_parent = dir.0.join("file.txt");
    std::fs::write(&file_parent, "Keep parent").unwrap();
    assert!(export_to_txt(&segments, &file_parent.join("transcript.txt")).is_err());
    assert_eq!(std::fs::read_to_string(file_parent).unwrap(), "Keep parent");
}
