
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSegment {
    pub id: String,
    pub speaker: String,
    pub text: String,
    pub start_time: f64,
    pub end_time: f64,
    pub language: String,
}

impl TranscriptSegment {
    pub fn new(
        id: String,
        speaker: String,
        text: String,
        start_time: f64,
        end_time: f64,
        language: String,
    ) -> Self {
        Self {
            id,
            speaker,
            text,
            start_time,
            end_time,
            language,
        }
    }
}
