
use super::models::TranscriptSegment;
use std::fs;
use std::io;
use std::path::Path;

pub fn export_to_txt(
    segments: &[TranscriptSegment],
    file_path: &Path,
) -> io::Result<()> {
    // Validate before opening the destination so invalid data cannot truncate
    // an existing export. Float-to-integer casts otherwise hide invalid times.
    if segments.iter().any(|segment| {
        !segment.start_time.is_finite()
            || segment.start_time < 0.0
            || segment.start_time >= u64::MAX as f64
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Transcript start times must be finite, non-negative seconds within range",
        ));
    }

    let mut content = String::from("VOXY Meeting Transcript\n");
    content.push_str("=======================\n\n");

    for segment in segments {
        let total_seconds = segment.start_time.floor() as u64;
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;

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
