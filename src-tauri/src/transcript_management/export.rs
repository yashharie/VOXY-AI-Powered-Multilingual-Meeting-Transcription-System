use super::models::TranscriptSegment;
use std::fs;
use std::io;
use std::path::Path;

pub fn export_to_txt(segments: &[TranscriptSegment], file_path: &Path) -> io::Result<()> {
    let content = format_as_txt(segments);
    fs::write(file_path, content)
}

/// Format transcript as plain text without writing a file.
/// Both copy and file-export use the same formatting.
pub fn format_as_txt(segments: &[TranscriptSegment]) -> String {
    let mut content = String::from("VOXY Meeting Transcript\n");
    content.push_str("=======================\n\n");

    for segment in segments {
        let total_seconds = segment.start_time as u64;
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        let speaker = segment
            .speaker
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Unknown speaker");

        content.push_str(&format!(
            "[{:02}:{:02}] {}: {}\n",
            minutes, seconds, speaker, segment.text
        ));
    }

    content
}
