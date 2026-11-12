
use super::models::TranscriptSegment;
use std::fs;
use std::io;
use std::path::Path;

pub fn export_to_txt(
    segments: &[TranscriptSegment],
    file_path: &Path,
) -> io::Result<()> {
    let mut content = String::from("VOXY Meeting Transcript\n");
    content.push_str("=======================\n\n");

    for segment in segments {
        let minutes = (segment.start_time / 60.0).floor() as u64;
        let seconds = (segment.start_time % 60.0).floor() as u64;

        content.push_str(&format!(
            "[{:02}:{:02}] {}: {}\n",
            minutes,
            seconds,
            segment.speaker,
            segment.text
        ));
    }

    fs::write(file_path, content)
}
