
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