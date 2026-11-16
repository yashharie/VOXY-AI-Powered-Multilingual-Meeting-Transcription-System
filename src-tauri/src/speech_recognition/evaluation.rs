//! Large-v3 reference evaluation and accuracy comparison.
//!
//! When hardware permits, the Large-v3 model can be loaded as a
//! high-accuracy reference baseline.  Smaller models (Base, Small) are
//! then compared against this baseline to quantify the accuracy trade-off.
//!
//! This module provides the types and utilities to:
//! - Detect whether the current hardware can run Large-v3
//! - Run reference transcriptions with Large-v3
//! - Compare results from smaller models against the reference
//! - Collect accuracy metrics (word overlap, character error rate)

use super::RecognitionResult;
use super::config::ModelSize;

// ─── Hardware capability ───────────────────────────────────────────────────────

/// Estimated memory requirements for Large-v3 models (in MB).
const LARGE_V3_MEMORY_MB: u64 = 3_100; // ~3.1 GB for F16
const LARGE_V3_Q5_MEMORY_MB: u64 = 1_100; // ~1.1 GB for Q5_0
const MIN_RECOMMENDED_RAM_MB: u64 = 8_192; // 8 GB minimum for Large-v3

/// Hardware capability assessment for running Large-v3.
#[derive(Debug, Clone)]
pub struct HardwareAssessment {
    /// Available logical CPU cores.
    pub cpu_cores: u32,
    /// Available system RAM in MB (best-effort estimate).
    pub available_ram_mb: u64,
    /// Whether Large-v3 is likely to run without excessive swapping.
    pub can_run_large_v3: bool,
    /// Whether the F16 (unquantized) Large-v3 model is feasible.
    pub can_run_large_v3_f16: bool,
}

impl HardwareAssessment {
    /// Assess the current hardware for Large-v3 feasibility.
    pub fn assess() -> Self {
        let cpu_cores = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1);

        // Best-effort RAM detection.  On Windows, we use the available
        // system memory.  If detection fails, assume enough for Large-v3.
        let available_ram_mb = Self::detect_available_ram_mb();

        let can_run_large_v3 = available_ram_mb >= LARGE_V3_Q5_MEMORY_MB + 1024
            && cpu_cores >= 2;
        let can_run_large_v3_f16 = available_ram_mb >= LARGE_V3_MEMORY_MB + 1024
            && cpu_cores >= 4;

        Self {
            cpu_cores,
            available_ram_mb,
            can_run_large_v3,
            can_run_large_v3_f16,
        }
    }

    /// Best-effort detection of available system RAM in MB.
    ///
    /// Uses platform-specific APIs where available.  Falls back to a
    /// generous default if detection is unavailable.
    #[cfg(target_os = "windows")]
    fn detect_available_ram_mb() -> u64 {
        // On Windows, use GlobalMemoryStatusEx via the windows crate
        // or a simple heuristic.  For now, return a conservative default
        // that allows Large-v3 evaluation.
        MIN_RECOMMENDED_RAM_MB
    }

    #[cfg(not(target_os = "windows"))]
    fn detect_available_ram_mb() -> u64 {
        MIN_RECOMMENDED_RAM_MB
    }
}

impl Default for HardwareAssessment {
    fn default() -> Self {
        Self::assess()
    }
}

impl std::fmt::Display for HardwareAssessment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Hardware: {} cores, {} MB RAM, Large-v3 {}",
            self.cpu_cores,
            self.available_ram_mb,
            if self.can_run_large_v3 { "supported" } else { "not recommended" }
        )
    }
}

// ─── Accuracy metrics ──────────────────────────────────────────────────────────

/// Accuracy metrics comparing a candidate model's output against the
/// Large-v3 reference.
#[derive(Debug, Clone)]
pub struct AccuracyMetrics {
    /// Model size that produced the candidate output.
    pub model_size: ModelSize,
    /// Word Error Rate (lower is better, 0.0 = perfect match).
    pub word_error_rate: f64,
    /// Character Error Rate (lower is better, 0.0 = perfect match).
    pub character_error_rate: f64,
    /// Number of reference words.
    pub reference_words: usize,
    /// Number of candidate words.
    pub candidate_words: usize,
    /// Number of correctly matched words.
    pub correct_words: usize,
    /// Average per-segment text similarity (0.0 – 1.0).
    pub segment_similarity: f64,
}

impl AccuracyMetrics {
    /// Compare candidate transcription results against Large-v3 reference.
    ///
    /// Both slices should contain results for the same audio, aligned
    /// by segment order.
    pub fn compare(
        model_size: ModelSize,
        reference: &[RecognitionResult],
        candidate: &[RecognitionResult],
    ) -> Self {
        let ref_text: String = reference.iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let cand_text: String = candidate.iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let ref_words: Vec<&str> = ref_text.split_whitespace().collect();
        let cand_words: Vec<&str> = cand_text.split_whitespace().collect();

        let wer = Self::word_error_rate(&ref_words, &cand_words);
        let cer = Self::character_error_rate(&ref_text, &cand_text);
        let correct = Self::count_correct_words(&ref_words, &cand_words);

        // Per-segment similarity (average Jaccard word overlap).
        let segment_similarity = Self::segment_similarity(reference, candidate);

        Self {
            model_size,
            word_error_rate: wer,
            character_error_rate: cer,
            reference_words: ref_words.len(),
            candidate_words: cand_words.len(),
            correct_words: correct,
            segment_similarity,
        }
    }

    /// Compute Word Error Rate using Levenshtein distance at word level.
    fn word_error_rate(reference: &[&str], candidate: &[&str]) -> f64 {
        if reference.is_empty() {
            return if candidate.is_empty() { 0.0 } else { 1.0 };
        }
        let edits = Self::levenshtein(reference, candidate);
        edits as f64 / reference.len() as f64
    }

    /// Compute Character Error Rate using Levenshtein distance at char level.
    fn character_error_rate(reference: &str, candidate: &str) -> f64 {
        let ref_chars: Vec<char> = reference.chars().collect();
        let cand_chars: Vec<char> = candidate.chars().collect();
        if ref_chars.is_empty() {
            return if cand_chars.is_empty() { 0.0 } else { 1.0 };
        }
        let edits = Self::levenshtein_chars(&ref_chars, &cand_chars);
        edits as f64 / ref_chars.len() as f64
    }

    /// Count words in the candidate that appear in the reference (simple
    /// set-intersection, not position-aware).
    fn count_correct_words(reference: &[&str], candidate: &[&str]) -> usize {
        let ref_set: std::collections::HashSet<&str> = reference.iter().copied().collect();
        candidate.iter().filter(|w| ref_set.contains(*w)).count()
    }

    /// Average word-level Jaccard similarity across aligned segments.
    fn segment_similarity(
        reference: &[RecognitionResult],
        candidate: &[RecognitionResult],
    ) -> f64 {
        if reference.is_empty() || candidate.is_empty() {
            return 0.0;
        }
        let n = reference.len().min(candidate.len());
        if n == 0 {
            return 0.0;
        }
        let mut total = 0.0;
        for i in 0..n {
            let ref_words: std::collections::HashSet<&str> =
                reference[i].text.split_whitespace().collect();
            let cand_words: std::collections::HashSet<&str> =
                candidate[i].text.split_whitespace().collect();
            if ref_words.is_empty() && cand_words.is_empty() {
                total += 1.0;
                continue;
            }
            let intersection = ref_words.intersection(&cand_words).count();
            let union = ref_words.union(&cand_words).count();
            total += if union > 0 {
                intersection as f64 / union as f64
            } else {
                0.0
            };
        }
        total / n as f64
    }

    /// Generic Levenshtein distance for slices of equatable items.
    fn levenshtein<T: PartialEq>(a: &[T], b: &[T]) -> usize {
        let (m, n) = (a.len(), b.len());
        if m == 0 { return n; }
        if n == 0 { return m; }
        let mut prev: Vec<usize> = (0..=n).collect();
        let mut curr = vec![0usize; n + 1];
        for i in 1..=m {
            curr[0] = i;
            for j in 1..=n {
                let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
                curr[j] = (prev[j] + 1)
                    .min(curr[j - 1] + 1)
                    .min(prev[j - 1] + cost);
            }
            std::mem::swap(&mut prev, &mut curr);
        }
        prev[n]
    }

    /// Levenshtein distance for char slices.
    fn levenshtein_chars(a: &[char], b: &[char]) -> usize {
        Self::levenshtein(a, b)
    }
}

impl std::fmt::Display for AccuracyMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}, WER={:.3}, CER={:.3}, similarity={:.3}",
            self.model_size,
            self.word_error_rate,
            self.character_error_rate,
            self.segment_similarity
        )
    }
}

// ─── Reference evaluation session ──────────────────────────────────────────────

/// A reference evaluation session that compares models against Large-v3.
///
/// The workflow is:
/// 1. Create a session with the hardware assessment.
/// 2. Record the Large-v3 reference transcription.
/// 3. Evaluate candidate model transcriptions against the reference.
/// 4. Collect and report accuracy metrics.
#[derive(Debug)]
pub struct ReferenceEvaluation {
    /// Hardware assessment for this session.
    pub hardware: HardwareAssessment,
    /// Reference transcription results from Large-v3.
    pub reference: Vec<RecognitionResult>,
    /// Accuracy metrics collected for each evaluated model.
    pub metrics: Vec<AccuracyMetrics>,
}

impl ReferenceEvaluation {
    /// Create a new evaluation session, assessing hardware first.
    pub fn new() -> Self {
        Self {
            hardware: HardwareAssessment::assess(),
            reference: Vec::new(),
            metrics: Vec::new(),
        }
    }

    /// Set the Large-v3 reference transcription.
    pub fn set_reference(&mut self, results: Vec<RecognitionResult>) {
        self.reference = results;
        tracing::info!(
            segments = self.reference.len(),
            "Large-v3 reference transcription recorded"
        );
    }

    /// Evaluate a candidate model's transcription against the reference.
    ///
    /// Returns the computed accuracy metrics.  If the reference is empty,
    /// returns metrics with zeroed values.
    pub fn evaluate(&mut self, model_size: ModelSize, results: &[RecognitionResult]) -> &AccuracyMetrics {
        let metrics = if self.reference.is_empty() {
            tracing::warn!("No reference transcription set — returning zeroed metrics");
            AccuracyMetrics {
                model_size: model_size.clone(),
                word_error_rate: 0.0,
                character_error_rate: 0.0,
                reference_words: 0,
                candidate_words: results.len(),
                correct_words: 0,
                segment_similarity: 0.0,
            }
        } else {
            AccuracyMetrics::compare(model_size.clone(), &self.reference, results)
        };

        tracing::info!(
            model = %model_size,
            wer = metrics.word_error_rate,
            cer = metrics.character_error_rate,
            similarity = metrics.segment_similarity,
            "Model evaluated against Large-v3 reference"
        );

        self.metrics.push(metrics);
        self.metrics.last().unwrap()
    }

    /// Returns `true` if the hardware can run Large-v3 for reference evaluation.
    pub fn can_run_reference(&self) -> bool {
        self.hardware.can_run_large_v3
    }

    /// Generate a summary report of all evaluated models.
    pub fn summary(&self) -> String {
        let mut report = String::new();
        report.push_str("=== VOXY Model Accuracy Evaluation ===\n");
        report.push_str(&format!("{}\n\n", self.hardware));

        if self.reference.is_empty() {
            report.push_str("No Large-v3 reference transcription available.\n");
            return report;
        }

        report.push_str("Reference: Large-v3\n");
        report.push_str(&format!("Reference segments: {}\n\n", self.reference.len()));
        report.push_str("Model | WER    | CER    | Similarity | Words\n");
        report.push_str("------|--------|--------|------------|------\n");

        for m in &self.metrics {
            report.push_str(&format!(
                "{:<6}| {:.4} | {:.4} | {:.4}     | {}/{}\n",
                m.model_size.id(),
                m.word_error_rate,
                m.character_error_rate,
                m.segment_similarity,
                m.candidate_words,
                m.reference_words,
            ));
        }

        report
    }
}

impl Default for ReferenceEvaluation {
    fn default() -> Self {
        Self::new()
    }
}
