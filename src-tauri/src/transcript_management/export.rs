use super::models::{validate_segment, TranscriptSegment};
use std::fs;
use std::io;
use std::path::Path;

pub fn export_to_txt(segments: &[TranscriptSegment], file_path: &Path) -> io::Result<()> {
    let content = format_as_txt(segments)?;
    fs::write(file_path, content)
}

/// Format the agreed TXT content without writing a file or changing segments.
/// Copy and file-export callers can share the same formatting and validation.
pub fn format_as_txt(segments: &[TranscriptSegment]) -> io::Result<String> {
    // Validate before opening the destination. Canonical timestamps are u64
    // milliseconds, so negative/NaN/infinite offsets cannot enter this API.
    let mut ids = std::collections::HashSet::new();
    for segment in segments {
        validate_segment(segment)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        if !ids.insert(segment.id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Duplicate transcript segment ID",
            ));
        }
    }

    let mut content = String::from("VOXY Meeting Transcript\n");
    content.push_str("=======================\n\n");

    for segment in segments {
        let total_seconds = segment.start_ms / 1000;
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;

        content.push_str(&format!(
            "[{:02}:{:02}] {}: {}\n",
            minutes,
            seconds,
            segment
                .speaker
                .as_deref()
                .filter(|speaker| !speaker.trim().is_empty())
                .unwrap_or("Unknown speaker"),
            segment.text
        ));
    }

    Ok(content)
}
