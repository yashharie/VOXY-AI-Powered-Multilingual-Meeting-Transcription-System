use super::models::{validate_segment, TranscriptError, TranscriptSegment};
use crate::transcript_processing::TranscriptProcessor;

/// A temporary borrowed view of the session's processor, never a transcript store.
pub struct TranscriptManager<'a> {
    processor: &'a mut TranscriptProcessor,
}
impl<'a> TranscriptManager<'a> {
    pub fn new(processor: &'a mut TranscriptProcessor) -> Self {
        Self { processor }
    }
    pub fn get_segments(&self) -> &[TranscriptSegment] {
        &self.processor.segments
    }
    pub fn edit_segment(
        &mut self,
        id: u64,
        new_text: &str,
    ) -> Result<TranscriptSegment, TranscriptError> {
        if id == 0 {
            return Err(TranscriptError::InvalidId);
        }
        let segment = self
            .processor
            .segments
            .iter_mut()
            .find(|segment| segment.id == id)
            .ok_or(TranscriptError::NotFound(id))?;
        validate_segment(segment)?;
        segment.text = new_text.to_owned();
        // Keep only an ID flag, not another copy of the user's text.
        self.processor.edited_ids.insert(id);
        Ok(segment.clone())
    }
    pub fn delete_segment(&mut self, id: u64) -> Result<(), TranscriptError> {
        if id == 0 {
            return Err(TranscriptError::InvalidId);
        }
        let position = self
            .processor
            .segments
            .iter()
            .position(|segment| segment.id == id)
            .ok_or(TranscriptError::NotFound(id))?;
        validate_segment(&self.processor.segments[position])?;
        let segment = self.processor.segments.remove(position);
        // Time ranges only: no deleted text, language or speaker is retained.
        self.processor
            .deleted_spans
            .push((segment.start_ms, segment.end_ms));
        self.processor.edited_ids.remove(&id);
        Ok(())
    }
    pub fn search(&self, keyword: &str) -> Vec<&TranscriptSegment> {
        search_segments(self.get_segments(), keyword)
    }
    pub fn clear(&mut self) {
        self.processor.clear_for_management();
    }
}
pub fn search_segments<'a>(
    segments: &'a [TranscriptSegment],
    keyword: &str,
) -> Vec<&'a TranscriptSegment> {
    let keyword = keyword.to_lowercase();
    segments
        .iter()
        .filter(|segment| {
            segment.text.to_lowercase().contains(&keyword)
                || segment
                    .speaker
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&keyword)
        })
        .collect()
}
