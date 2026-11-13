use super::models::{TranscriptError, TranscriptSegment};

/// In-session transcript store — holds segments in memory only.
/// Cleared when the session ends; no data is persisted.
#[derive(Debug, Default)]
pub struct TranscriptManager {
    segments: Vec<TranscriptSegment>,
}

impl TranscriptManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_segment(&mut self, segment: TranscriptSegment) {
        self.segments.push(segment);
    }

    pub fn get_segments(&self) -> &[TranscriptSegment] {
        &self.segments
    }

    pub fn edit_segment(&mut self, index: u32, new_text: &str) -> Result<(), TranscriptError> {
        let segment = self
            .segments
            .iter_mut()
            .find(|s| s.index == index)
            .ok_or(TranscriptError::NotFound(index as u64))?;
        segment.text = new_text.to_owned();
        Ok(())
    }

    pub fn delete_segment(&mut self, index: u32) -> Result<(), TranscriptError> {
        let pos = self
            .segments
            .iter()
            .position(|s| s.index == index)
            .ok_or(TranscriptError::NotFound(index as u64))?;
        self.segments.remove(pos);
        Ok(())
    }

    pub fn search(&self, keyword: &str) -> Vec<&TranscriptSegment> {
        let kw = keyword.to_lowercase();
        self.segments
            .iter()
            .filter(|s| {
                s.text.to_lowercase().contains(&kw)
                    || s.speaker
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&kw)
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.segments.clear();
    }
}
