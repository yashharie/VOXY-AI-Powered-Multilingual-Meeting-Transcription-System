//! Core logic: merge text + timestamps + speakers, keep order, drop duplicates.
//! No Tauri or async code here, so it is easy to unit test.

use super::types::*;
use std::collections::HashSet;

fn overlap_ms(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> u64 {
    a_end.min(b_end).saturating_sub(a_start.max(b_start))
}

/// Two time spans describe the same utterance when they overlap by at least
/// half of the shorter one (or start together when one has zero length).
fn is_same_span(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> bool {
    let shorter = a_end
        .saturating_sub(a_start)
        .min(b_end.saturating_sub(b_start));
    if shorter == 0 {
        return a_start == b_start;
    }
    overlap_ms(a_start, a_end, b_start, b_end).saturating_mul(2) >= shorter
}

/// Trim and collapse whitespace only. Characters are never changed,
/// so Sinhala, Tamil and English text stays exactly as recognized.
pub fn normalize_text(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug)]
pub struct TranscriptProcessor {
    session_id: String,
    pub(crate) segments: Vec<TranscriptSegment>,
    pub(crate) edited_ids: HashSet<u64>,
    pub(crate) deleted_spans: Vec<(u64, u64)>,
    speakers: Vec<SpeakerInterval>,
    next_id: u64,
}

impl TranscriptProcessor {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            segments: Vec::new(),
            edited_ids: HashSet::new(),
            deleted_spans: Vec::new(),
            speakers: Vec::new(),
            next_id: 1,
        }
    }

    /// Speaker with the largest time overlap, or None when nobody overlaps.
    fn speaker_for(&self, start_ms: u64, end_ms: u64) -> Option<String> {
        let mut best: Option<(u64, &str)> = None;
        for s in &self.speakers {
            let ov = overlap_ms(start_ms, end_ms, s.start_ms, s.end_ms);
            if ov > 0 && best.map_or(true, |(b, _)| ov > b) {
                best = Some((ov, s.speaker.as_str()));
            }
        }
        best.map(|(_, name)| name.to_string())
    }

    fn sort(&mut self) {
        self.segments.sort_by_key(|s| (s.start_ms, s.end_ms, s.id));
    }

    /// Handle one result from Whisper. Returns the segments that changed
    /// (empty when the chunk was ignored).
    ///
    /// Policy:
    /// - empty text or end < start: ignored
    /// - overlaps a Partial segment: that segment is updated in place (same id)
    /// - overlaps a Final segment: ignored (repeat or stale update)
    /// - otherwise: new segment, inserted in chronological order
    pub fn ingest_chunk(&mut self, chunk: RecognizedChunk) -> Vec<TranscriptSegment> {
        let text = normalize_text(&chunk.text);
        if text.is_empty() || chunk.end_ms < chunk.start_ms {
            return Vec::new();
        }
        if self
            .deleted_spans
            .iter()
            .any(|&(start, end)| is_same_span(start, end, chunk.start_ms, chunk.end_ms))
        {
            return Vec::new();
        }
        let status = if chunk.is_final {
            SegmentStatus::Final
        } else {
            SegmentStatus::Partial
        };
        let speaker = self.speaker_for(chunk.start_ms, chunk.end_ms);

        let existing = self
            .segments
            .iter()
            .position(|s| is_same_span(s.start_ms, s.end_ms, chunk.start_ms, chunk.end_ms));

        let id = match existing {
            Some(i) => {
                if self.segments[i].status == SegmentStatus::Final {
                    return Vec::new();
                }
                let seg = &mut self.segments[i];
                seg.start_ms = chunk.start_ms;
                seg.end_ms = chunk.end_ms;
                if !self.edited_ids.contains(&seg.id) {
                    seg.text = text;
                }
                seg.language = chunk.language;
                seg.speaker = speaker;
                seg.status = status;
                seg.id
            }
            None => {
                let id = self.next_id;
                self.next_id += 1;
                self.segments.push(TranscriptSegment {
                    id,
                    start_ms: chunk.start_ms,
                    end_ms: chunk.end_ms,
                    speaker,
                    text,
                    language: chunk.language,
                    status,
                });
                id
            }
        };

        self.sort();
        self.segments
            .iter()
            .filter(|s| s.id == id)
            .cloned()
            .collect()
    }

    /// Handle speaker intervals from diarization. They may arrive after the text,
    /// so existing segments are relabelled. Returns the segments whose speaker changed.
    pub fn add_speaker_intervals(
        &mut self,
        intervals: Vec<SpeakerInterval>,
    ) -> Vec<SegmentWithSpeakerChange> {
        self.speakers.extend(intervals);
        let labels: Vec<Option<String>> = self
            .segments
            .iter()
            .map(|s| self.speaker_for(s.start_ms, s.end_ms))
            .collect();
        let mut changed = Vec::new();
        for (seg, label) in self.segments.iter_mut().zip(labels) {
            if label.is_some() && seg.speaker != label {
                seg.speaker = label;
                changed.push(seg.clone());
            }
        }
        changed
    }

    /// Full ordered transcript for the review/export module.
    pub fn session_transcript(&self) -> SessionTranscript {
        SessionTranscript {
            session_id: self.session_id.clone(),
            segments: self.segments.clone(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn clear_for_management(&mut self) {
        self.segments.clear();
        self.speakers.clear();
        self.edited_ids.clear();
        self.deleted_spans.clear();
        // IDs are not reused within the lifetime of a session processor.
    }
}

/// Alias to make the return type of `add_speaker_intervals` self-explanatory.
pub type SegmentWithSpeakerChange = TranscriptSegment;

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(start: u64, end: u64, text: &str, fin: bool) -> RecognizedChunk {
        RecognizedChunk {
            start_ms: start,
            end_ms: end,
            text: text.into(),
            language: Some("en".into()),
            is_final: fin,
        }
    }
    fn spk(start: u64, end: u64, name: &str) -> SpeakerInterval {
        SpeakerInterval {
            start_ms: start,
            end_ms: end,
            speaker: name.into(),
        }
    }

    #[test]
    fn merges_text_timestamp_and_speaker() {
        let mut p = TranscriptProcessor::new("s1");
        p.add_speaker_intervals(vec![
            spk(0, 5000, "Speaker 1"),
            spk(5000, 9000, "Speaker 2"),
        ]);
        let out = p.ingest_chunk(chunk(5200, 8800, "Hello everyone", true));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].speaker.as_deref(), Some("Speaker 2"));
        assert_eq!((out[0].start_ms, out[0].end_ms), (5200, 8800));
    }

    #[test]
    fn segments_are_chronological() {
        let mut p = TranscriptProcessor::new("s1");
        p.ingest_chunk(chunk(10_000, 12_000, "third", true));
        p.ingest_chunk(chunk(0, 2_000, "first", true));
        p.ingest_chunk(chunk(5_000, 7_000, "second", true));
        let texts: Vec<_> = p
            .session_transcript()
            .segments
            .into_iter()
            .map(|s| s.text)
            .collect();
        assert_eq!(texts, vec!["first", "second", "third"]);
    }

    #[test]
    fn partial_is_updated_then_finalized_with_same_id() {
        let mut p = TranscriptProcessor::new("s1");
        let a = p.ingest_chunk(chunk(0, 2000, "How are", false));
        let b = p.ingest_chunk(chunk(0, 3000, "How are you today", true));
        assert_eq!(a[0].id, b[0].id);
        assert_eq!(b[0].text, "How are you today");
        assert_eq!(b[0].status, SegmentStatus::Final);
        assert_eq!(p.session_transcript().segments.len(), 1);
    }

    #[test]
    fn repeated_final_is_ignored() {
        let mut p = TranscriptProcessor::new("s1");
        p.ingest_chunk(chunk(0, 3000, "same text", true));
        assert!(p.ingest_chunk(chunk(0, 3000, "same text", true)).is_empty());
        assert_eq!(p.session_transcript().segments.len(), 1);
    }

    #[test]
    fn stale_partial_after_final_is_ignored() {
        let mut p = TranscriptProcessor::new("s1");
        p.ingest_chunk(chunk(0, 3000, "final text", true));
        assert!(p.ingest_chunk(chunk(0, 2500, "final", false)).is_empty());
        assert_eq!(p.session_transcript().segments[0].text, "final text");
    }

    #[test]
    fn empty_text_is_ignored() {
        let mut p = TranscriptProcessor::new("s1");
        assert!(p.ingest_chunk(chunk(0, 1000, "   \n ", true)).is_empty());
    }

    #[test]
    fn sinhala_tamil_english_are_preserved() {
        let mut p = TranscriptProcessor::new("s1");
        let si = "ආයුබෝවන්   සියලුදෙනාටම";
        let ta = "வணக்கம் எல்லோருக்கும்";
        let mixed = "Meeting එක start කරමු";
        p.ingest_chunk(chunk(0, 1000, si, true));
        p.ingest_chunk(chunk(2000, 3000, ta, true));
        p.ingest_chunk(chunk(4000, 5000, mixed, true));
        let t = p.session_transcript().segments;
        assert_eq!(t[0].text, "ආයුබෝවන් සියලුදෙනාටම"); // only extra spaces collapsed
        assert_eq!(t[1].text, ta);
        assert_eq!(t[2].text, mixed);
    }

    #[test]
    fn late_speaker_info_relabels_existing_segments() {
        let mut p = TranscriptProcessor::new("s1");
        let first = p.ingest_chunk(chunk(1000, 3000, "hi", true));
        assert_eq!(first[0].speaker, None);
        let changed = p.add_speaker_intervals(vec![spk(0, 4000, "Speaker 1")]);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].speaker.as_deref(), Some("Speaker 1"));
    }

    #[test]
    fn json_uses_camel_case() {
        let mut p = TranscriptProcessor::new("s1");
        let seg = p.ingest_chunk(chunk(0, 1000, "hi", false)).remove(0);
        let json = serde_json::to_string(&seg).unwrap();
        assert!(json.contains("\"startMs\":0") && json.contains("\"status\":\"partial\""));
    }
}
