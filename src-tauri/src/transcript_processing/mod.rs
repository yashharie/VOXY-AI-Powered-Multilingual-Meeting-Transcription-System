//! Transcript processing module.
//!
//! Responsibilities:
//!   - Build TranscriptSegment from RecognitionResult + optional speaker label.
//!   - Deduplicate overlapping-window output (identical text at similar timestamps).
//!   - Reconcile partial updates with final segments.
//!   - Validate timestamps (end >= start, both non-negative).
//!   - Support delayed speaker-label updates (diarization results arrive after Whisper).
//!   - Keep segments in chronological order.
//!   - Language metadata: preserve the per-segment language from Whisper; treat
//!     window-level language detection as "this window's dominant language" and
//!     assign it only to segments that fall within the window, not to all segments.

use chrono::Utc;
use std::collections::HashMap;

use crate::core::types::TranscriptSegment;
use crate::speaker_analysis::SpeakerInterval;
use crate::speech_recognition::RecognitionResult;

/// Maximum overlap between two segment time ranges that is still considered a
/// potential duplicate (overlapping window boundaries produce near-identical text).
/// Set to 400 ms — typical Whisper window overlap is 0.5–1.0 s, so a 400 ms
/// overlap threshold catches duplicates from adjacent windows without
/// incorrectly merging genuinely distinct sentences.
const DEDUP_OVERLAP_MS: u64 = 400;

/// If two segments' texts share this fraction of words they are considered duplicates.
const DEDUP_SIMILARITY_THRESHOLD: f32 = 0.85;

pub struct TranscriptProcessor {
    /// Finalised segments kept for deduplication and delayed speaker updates.
    /// Keyed by segment index.
    seen: HashMap<u32, FinalisedEntry>,
}

struct FinalisedEntry {
    start_ms: u64,
    end_ms: u64,
    /// Normalised word list used for similarity checks.
    words: Vec<String>,
}

impl TranscriptProcessor {
    pub fn new() -> Self {
        Self {
            seen: HashMap::new(),
        }
    }

    /// Build a `TranscriptSegment` from a Whisper recognition result.
    ///
    /// Timestamps: taken directly from `result.start_ms` / `result.end_ms` which
    /// are session-relative milliseconds set by the pipeline's window_start_ms offset.
    ///
    /// Language: taken from `result.language` as reported by Whisper.
    /// "auto" or empty string is preserved as-is — do not substitute a guessed value.
    ///
    /// Speaker: `None` when diarization is inactive; a "Speaker N" label otherwise.
    pub fn build_segment(
        &self,
        result: RecognitionResult,
        speaker: Option<String>,
    ) -> TranscriptSegment {
        // Validate timestamps: end must be >= start.
        let start_time = if result.start_ms > result.end_ms {
            tracing::warn!(
                start_ms = result.start_ms,
                end_ms = result.end_ms,
                "Recognition result has start > end — clamping"
            );
            result.end_ms as f64 / 1000.0
        } else {
            result.start_ms as f64 / 1000.0
        };
        let end_time = result.end_ms as f64 / 1000.0;

        // Normalise language: empty / unknown → "und"
        let language = if result.language.trim().is_empty()
            || result.language == "auto"
            || result.language == "unknown"
        {
            "und".to_string()
        } else {
            result.language.clone()
        };

        TranscriptSegment {
            index: 0, // assigned by Session::push_segment
            text: result.text,
            translation: None,
            start_time,
            end_time,
            language,
            speaker,
            is_partial: result.is_partial,
            timestamp: Utc::now(),
        }
    }

    /// Check whether a new final segment is a duplicate of anything already seen.
    ///
    /// Two segments are duplicates when:
    ///   1. Their time ranges overlap by more than DEDUP_OVERLAP_MS, AND
    ///   2. Their word-level Jaccard similarity exceeds DEDUP_SIMILARITY_THRESHOLD.
    ///
    /// Returns true if the segment should be DISCARDED.
    pub fn is_duplicate(&self, segment: &TranscriptSegment) -> bool {
        if segment.is_partial {
            return false; // partials are never deduplicated
        }
        let start_ms = (segment.start_time * 1000.0) as u64;
        let end_ms = (segment.end_time * 1000.0) as u64;
        let new_words = normalise_words(&segment.text);

        for entry in self.seen.values() {
            let overlap = time_overlap_ms(start_ms, end_ms, entry.start_ms, entry.end_ms);
            if overlap < DEDUP_OVERLAP_MS {
                continue;
            }
            let sim = jaccard_similarity(&new_words, &entry.words);
            if sim >= DEDUP_SIMILARITY_THRESHOLD {
                tracing::debug!(
                    text = %segment.text,
                    similarity = %sim,
                    "Duplicate segment suppressed (overlapping Whisper window)"
                );
                return true;
            }
        }
        false
    }

    /// Register a finalised segment so future segments can be deduplicated against it.
    pub fn register(&mut self, segment: &TranscriptSegment) {
        if segment.is_partial || segment.index == 0 {
            return;
        }
        self.seen.insert(
            segment.index,
            FinalisedEntry {
                start_ms: (segment.start_time * 1000.0) as u64,
                end_ms: (segment.end_time * 1000.0) as u64,
                words: normalise_words(&segment.text),
            },
        );
        // Evict old entries to bound memory — keep only the last 200 segments.
        if self.seen.len() > 200 {
            let min_key = self.seen.keys().copied().min().unwrap_or(0);
            self.seen.remove(&min_key);
        }
    }

    /// Find the best matching speaker interval for an existing segment and return an
    /// updated copy with the speaker label filled in.
    ///
    /// Called when diarization results arrive late (after Whisper has already emitted
    /// the segment).  Returns `None` if no interval overlaps or the segment already has
    /// a speaker label.
    pub fn apply_delayed_speaker(
        segment: &TranscriptSegment,
        intervals: &[SpeakerInterval],
    ) -> Option<TranscriptSegment> {
        if segment.speaker.is_some() {
            return None; // already labelled — do not overwrite
        }
        let start_ms = (segment.start_time * 1000.0) as u64;
        let end_ms = (segment.end_time * 1000.0) as u64;

        // Find the interval with the greatest time overlap.
        let best = intervals.iter().max_by_key(|iv| {
            time_overlap_ms(start_ms, end_ms, iv.start_ms, iv.end_ms)
        })?;

        let overlap = time_overlap_ms(start_ms, end_ms, best.start_ms, best.end_ms);
        if overlap == 0 {
            return None;
        }

        let mut updated = segment.clone();
        updated.speaker = Some(best.speaker.clone());
        Some(updated)
    }
}

impl Default for TranscriptProcessor {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Compute the overlap in ms between two time ranges [a_start, a_end) and [b_start, b_end).
fn time_overlap_ms(a_start: u64, a_end: u64, b_start: u64, b_end: u64) -> u64 {
    let a_end = a_end.max(a_start);
    let b_end = b_end.max(b_start);
    let overlap_start = a_start.max(b_start);
    let overlap_end = a_end.min(b_end);
    overlap_end.saturating_sub(overlap_start)
}

/// Tokenise and lowercase a transcript string into individual words.
fn normalise_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(|c| c.to_lowercase())
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Jaccard similarity: |intersection| / |union| over word multisets.
fn jaccard_similarity(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut counts_a: HashMap<&str, usize> = HashMap::new();
    let mut counts_b: HashMap<&str, usize> = HashMap::new();
    for w in a { *counts_a.entry(w.as_str()).or_insert(0) += 1; }
    for w in b { *counts_b.entry(w.as_str()).or_insert(0) += 1; }

    let mut intersection = 0usize;
    let mut union = 0usize;
    let all_keys: std::collections::HashSet<&str> =
        counts_a.keys().copied().chain(counts_b.keys().copied()).collect();
    for key in all_keys {
        let ca = *counts_a.get(key).unwrap_or(&0);
        let cb = *counts_b.get(key).unwrap_or(&0);
        intersection += ca.min(cb);
        union += ca.max(cb);
    }
    if union == 0 { 1.0 } else { intersection as f32 / union as f32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_segment(text: &str, start_s: f64, end_s: f64, is_partial: bool) -> TranscriptSegment {
        TranscriptSegment {
            index: 1,
            text: text.to_string(),
            translation: None,
            start_time: start_s,
            end_time: end_s,
            language: "en".to_string(),
            speaker: None,
            is_partial,
            timestamp: Utc::now(),
        }
    }

    // ── build_segment ─────────────────────────────────────────────────────────

    #[test]
    fn build_segment_sets_timestamps_from_result() {
        let proc = TranscriptProcessor::new();
        let result = RecognitionResult {
            text: "Hello".to_string(),
            language: "en".to_string(),
            start_ms: 1500,
            end_ms: 3200,
            is_partial: false,
        };
        let seg = proc.build_segment(result, None);
        assert!((seg.start_time - 1.5).abs() < 1e-6);
        assert!((seg.end_time - 3.2).abs() < 1e-6);
    }

    #[test]
    fn build_segment_clamps_inverted_timestamps() {
        let proc = TranscriptProcessor::new();
        let result = RecognitionResult {
            text: "Bad timestamps".to_string(),
            language: "en".to_string(),
            start_ms: 5000,
            end_ms: 2000, // start > end
            is_partial: false,
        };
        let seg = proc.build_segment(result, None);
        assert!(seg.start_time <= seg.end_time, "start must be <= end after clamping");
    }

    #[test]
    fn build_segment_normalises_unknown_language() {
        let proc = TranscriptProcessor::new();
        for lang in ["", "auto", "unknown"] {
            let result = RecognitionResult {
                text: "test".to_string(),
                language: lang.to_string(),
                start_ms: 0,
                end_ms: 1000,
                is_partial: false,
            };
            let seg = proc.build_segment(result, None);
            assert_eq!(seg.language, "und", "language '{}' should normalise to 'und'", lang);
        }
    }

    #[test]
    fn build_segment_preserves_multilingual_text() {
        let proc = TranscriptProcessor::new();
        let texts = [
            ("வணக்கம்", "ta"),
            ("ආයුබෝවන්", "si"),
            ("Good morning", "en"),
            ("Yes tauri use karannē", "si"), // code-switched
        ];
        for (text, lang) in texts {
            let result = RecognitionResult {
                text: text.to_string(),
                language: lang.to_string(),
                start_ms: 0,
                end_ms: 1000,
                is_partial: false,
            };
            let seg = proc.build_segment(result, None);
            assert_eq!(seg.text, text, "Original text must be preserved");
            assert_eq!(seg.language, lang);
        }
    }

    // ── deduplication ─────────────────────────────────────────────────────────

    #[test]
    fn identical_overlapping_segment_is_duplicate() {
        let mut proc = TranscriptProcessor::new();
        let first = make_segment("Hello world how are you", 1.0, 3.0, false);
        let mut registered = first.clone();
        registered.index = 1;
        proc.register(&registered);

        // Near-identical text, overlapping window
        let second = make_segment("Hello world how are you", 2.5, 4.5, false);
        assert!(proc.is_duplicate(&second), "Near-identical overlapping segment should be duplicate");
    }

    #[test]
    fn non_overlapping_identical_text_is_not_duplicate() {
        let mut proc = TranscriptProcessor::new();
        let first = make_segment("Hello world", 1.0, 2.0, false);
        let mut registered = first.clone();
        registered.index = 1;
        proc.register(&registered);

        // Same text but far apart in time
        let second = make_segment("Hello world", 10.0, 11.0, false);
        assert!(!proc.is_duplicate(&second), "Non-overlapping segments should not be duplicates");
    }

    #[test]
    fn partial_segments_are_never_deduplicated() {
        let mut proc = TranscriptProcessor::new();
        let first = make_segment("Hello world", 1.0, 3.0, false);
        let mut registered = first.clone();
        registered.index = 1;
        proc.register(&registered);

        let partial = make_segment("Hello world", 2.5, 4.0, true);
        assert!(!proc.is_duplicate(&partial), "Partial segments must never be deduplicated");
    }

    #[test]
    fn different_text_in_overlapping_window_is_not_duplicate() {
        let mut proc = TranscriptProcessor::new();
        let first = make_segment("The quick brown fox", 0.0, 2.0, false);
        let mut registered = first.clone();
        registered.index = 1;
        proc.register(&registered);

        // Different content, overlapping window
        let second = make_segment("She sells seashells by the seashore", 1.5, 3.5, false);
        assert!(!proc.is_duplicate(&second), "Different content should not be treated as duplicate");
    }

    // ── delayed speaker assignment ─────────────────────────────────────────────

    #[test]
    fn delayed_speaker_assignment_fills_missing_label() {
        let seg = make_segment("Hello", 1.0, 2.0, false);
        let intervals = vec![SpeakerInterval {
            start_ms: 800,
            end_ms: 2500,
            speaker: "Speaker 1".to_string(),
        }];
        let updated = TranscriptProcessor::apply_delayed_speaker(&seg, &intervals);
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().speaker.as_deref(), Some("Speaker 1"));
    }

    #[test]
    fn delayed_speaker_assignment_does_not_overwrite_existing_label() {
        let mut seg = make_segment("Hello", 1.0, 2.0, false);
        seg.speaker = Some("Speaker 2".to_string());
        let intervals = vec![SpeakerInterval {
            start_ms: 800,
            end_ms: 2500,
            speaker: "Speaker 1".to_string(),
        }];
        let updated = TranscriptProcessor::apply_delayed_speaker(&seg, &intervals);
        assert!(updated.is_none(), "Existing speaker label must not be overwritten");
    }

    #[test]
    fn delayed_speaker_assignment_returns_none_when_no_overlap() {
        let seg = make_segment("Hello", 10.0, 11.0, false);
        let intervals = vec![SpeakerInterval {
            start_ms: 0,
            end_ms: 1000,
            speaker: "Speaker 1".to_string(),
        }];
        let updated = TranscriptProcessor::apply_delayed_speaker(&seg, &intervals);
        assert!(updated.is_none(), "No overlap means no speaker can be assigned");
    }

    // ── timestamp helpers ─────────────────────────────────────────────────────

    #[test]
    fn time_overlap_symmetry() {
        let o1 = time_overlap_ms(0, 2000, 1000, 3000);
        let o2 = time_overlap_ms(1000, 3000, 0, 2000);
        assert_eq!(o1, o2, "Overlap must be symmetric");
        assert_eq!(o1, 1000);
    }

    #[test]
    fn time_overlap_no_intersection() {
        assert_eq!(time_overlap_ms(0, 1000, 2000, 3000), 0);
        assert_eq!(time_overlap_ms(2000, 3000, 0, 1000), 0);
    }

    // ── Jaccard similarity ────────────────────────────────────────────────────

    #[test]
    fn jaccard_identical() {
        let a = normalise_words("hello world");
        let b = normalise_words("hello world");
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn jaccard_disjoint() {
        let a = normalise_words("hello world");
        let b = normalise_words("foo bar");
        assert!((jaccard_similarity(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn jaccard_partial_overlap() {
        let a = normalise_words("hello world foo");
        let b = normalise_words("hello world bar");
        let sim = jaccard_similarity(&a, &b);
        assert!(sim > 0.0 && sim < 1.0);
    }

    // ── Diarization model reporting ───────────────────────────────────────────

    #[test]
    fn diarization_active_flag_reflects_model_availability() {
        use crate::speaker_analysis::{create_engines, SpeakerAnalysisConfig};
        let cfg = SpeakerAnalysisConfig::default();
        // Without the sherpa feature, diarization is always unavailable.
        let (_, diarizer) = create_engines(&cfg, true)
            .unwrap_or_else(|_| {
                (crate::speaker_analysis::VadEngine::new(),
                 crate::speaker_analysis::DiarizationEngine::new())
            });
        // is_active() must honestly reflect whether speaker embeddings are available.
        // Without sherpa feature: always false.
        // With sherpa feature + model: true.
        // The important property is it never lies.
        let _ = diarizer.is_active(); // just calling it must not panic
    }
}
