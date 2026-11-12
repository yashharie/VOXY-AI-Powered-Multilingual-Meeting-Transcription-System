
use super::models::TranscriptSegment;

#[derive(Debug, Default)]
pub struct TranscriptManager {
    segments: Vec<TranscriptSegment>,
}

impl TranscriptManager {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn add_segment(&mut self, segment: TranscriptSegment) {
        self.segments.push(segment);
    }

    pub fn get_segments(&self) -> &[TranscriptSegment] {
        &self.segments
    }

    pub fn edit_segment(
        &mut self,
        id: &str,
        new_text: &str,
    ) -> Result<(), String> {
        let segment = self
            .segments
            .iter_mut()
            .find(|segment| segment.id == id)
            .ok_or("Transcript segment not found")?;

        segment.text = new_text.to_string();
        Ok(())
    }

    pub fn search(&self, keyword: &str) -> Vec<&TranscriptSegment> {
        self.segments
            .iter()
            .filter(|segment| {
                segment
                    .text
                    .to_lowercase()
                    .contains(&keyword.to_lowercase())
            })
            .collect()
    }
    pub fn delete_segment(&mut self, id: &str) -> Result<(), String> {
    let position = self
        .segments
        .iter()
        .position(|segment| segment.id == id)
        .ok_or("Transcript segment not found")?;

    self.segments.remove(position);
    Ok(())
}

    pub fn clear(&mut self) {
        self.segments.clear();
    }
}
