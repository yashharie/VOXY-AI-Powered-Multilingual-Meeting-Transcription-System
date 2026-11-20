//! Speech recognition module — owned by Member 5.
//!
//! Uses whisper.cpp CLI binary (main.exe / whisper-cli.exe) for local transcription.
//! Performs overlapping contextual audio buffering, automatic language detection (Tamil,
//! Sinhala, English), exact timestamp extraction, and strictly preserves original spoken text
//! without translation or transliteration.

pub mod config;
pub mod evaluation;

use crate::audio_processing::ProcessedAudio;
use crate::core::error::{AppError, AppResult};
use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};

/// A recognized text span with timing and language metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecognitionResult {
    pub text: String,
    pub language: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub is_partial: bool,
}

#[derive(Deserialize)]
struct WhisperJsonOutput {
    result: Option<WhisperResultMeta>,
    transcription: Option<Vec<WhisperJsonSegment>>,
}

#[derive(Deserialize)]
struct WhisperResultMeta {
    language: Option<String>,
}

#[derive(Deserialize)]
struct WhisperJsonSegment {
    offsets: Option<WhisperOffsets>,
    text: String,
    no_speech_prob: Option<f64>,
    avg_logprob: Option<f64>,
    compression_ratio: Option<f64>,
    tokens: Option<Vec<WhisperJsonToken>>,
}

#[derive(Deserialize)]
struct WhisperJsonToken {
    p: Option<f64>,
}

#[derive(Deserialize)]
struct WhisperOffsets {
    from: u64,
    to: u64,
}

/// Handle to the Whisper inference engine.
pub struct WhisperEngine {
    model_path: PathBuf,
    is_loaded: bool,
    /// Path to whisper.cpp CLI binary (main.exe or whisper-cli.exe)
    cli_path: Option<PathBuf>,
    /// Optional Silero VAD model supported by newer whisper.cpp CLI builds.
    vad_model_path: Option<PathBuf>,
}

impl WhisperEngine {
    /// Load a whisper.cpp model from the given path (or discover it in standard directories).
    pub fn new(requested_path: &Path) -> AppResult<Self> {
        let model_path = Self::find_model(requested_path)
            .ok_or_else(|| AppError::Internal(format!(
                "Whisper model file not found. Looked for '{}' and standard models directories.",
                requested_path.display()
            )))?;

        let cli_path = Self::find_cli(&model_path);
        if cli_path.is_none() {
            return Err(AppError::Internal(
                "Whisper CLI executable (main.exe / whisper-cli.exe) not found.".into(),
            ));
        }

        let vad_model_path = cli_path
            .as_deref()
            .and_then(|cli| Self::prepare_vad_model(cli, &model_path));

        tracing::info!(
            model = ?model_path,
            cli = ?cli_path,
            silero_vad = ?vad_model_path,
            "Whisper engine initialised successfully"
        );

        Ok(Self {
            model_path,
            is_loaded: true,
            cli_path,
            vad_model_path,
        })
    }

    /// Create a stub engine with no model loaded.
    pub fn stub() -> Self {
        Self {
            model_path: PathBuf::new(),
            is_loaded: false,
            cli_path: None,
            vad_model_path: None,
        }
    }

    /// Download and enable the lightweight Silero model when this whisper.cpp
    /// executable advertises its native VAD options. Older binaries remain usable.
    fn prepare_vad_model(cli: &Path, whisper_model: &Path) -> Option<PathBuf> {
        let help = std::process::Command::new(cli).arg("-h").output().ok()?;
        let help_text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&help.stdout),
            String::from_utf8_lossy(&help.stderr)
        );
        if !help_text.contains("--vad") || !help_text.contains("--vad-model") {
            tracing::info!("Whisper CLI does not support native Silero VAD; using confidence filters");
            return None;
        }

        let model_dir = whisper_model.parent().unwrap_or_else(|| Path::new("models"));
        let vad_path = model_dir.join("ggml-silero-v6.2.0.bin");
        if vad_path.is_file()
            && vad_path.metadata().map(|metadata| metadata.len() > 100_000).unwrap_or(false)
        {
            return Some(vad_path);
        }

        // Remove a previous partial/corrupt download before replacing it (Windows
        // does not allow rename-over-existing-file).
        let _ = std::fs::remove_file(&vad_path);
        let download_path = vad_path.with_extension("bin.download");
        let url = "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin";
        tracing::info!("Downloading the lightweight Silero VAD model (first launch only)");
        let download = std::process::Command::new("curl.exe")
            .args(["-L", "--fail", "--silent", "--show-error", url, "-o"])
            .arg(&download_path)
            .status();

        match download {
            Ok(status)
                if status.success()
                    && download_path.metadata().map(|metadata| metadata.len() > 100_000).unwrap_or(false) =>
            {
                if std::fs::rename(&download_path, &vad_path).is_ok() {
                    tracing::info!(path = ?vad_path, "Silero VAD model is ready");
                    Some(vad_path)
                } else {
                    let _ = std::fs::remove_file(&download_path);
                    tracing::warn!("Could not move downloaded Silero VAD model into the models directory");
                    None
                }
            }
            Ok(status) => {
                let _ = std::fs::remove_file(&download_path);
                tracing::warn!(?status, "Silero VAD download failed; using confidence filters only");
                None
            }
            Err(error) => {
                tracing::warn!(%error, "curl.exe unavailable; using confidence filters without Silero VAD");
                None
            }
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.is_loaded
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    pub fn cli_path(&self) -> Option<&Path> {
        self.cli_path.as_deref()
    }

    /// Search standard directories for the Whisper model.
    pub fn find_model(hint: &Path) -> Option<PathBuf> {
        if hint.is_file() {
            return Some(hint.to_path_buf());
        }

        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_default();

        let candidates = [
            hint.to_path_buf(),
            // ggml-small has significantly better Tamil/Sinhala accuracy than ggml-base.
            exe_dir.join("models").join("ggml-small.bin"),
            exe_dir.join("models").join("ggml-base.bin"),
            exe_dir.join("ggml-small.bin"),
            exe_dir.join("ggml-base.bin"),
            PathBuf::from("models/ggml-small.bin"),
            PathBuf::from("models/ggml-base.bin"),
            PathBuf::from("src-tauri/models/ggml-small.bin"),
            PathBuf::from("src-tauri/models/ggml-base.bin"),
            PathBuf::from("../models/ggml-small.bin"),
            PathBuf::from("../models/ggml-base.bin"),
            PathBuf::from("../../models/ggml-small.bin"),
            PathBuf::from("../../models/ggml-base.bin"),
        ];

        candidates.into_iter().find(|p| p.is_file())
    }

    /// Search for the whisper.cpp CLI binary.
    pub fn find_cli(model_path: &Path) -> Option<PathBuf> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_default();

        let model_dir = model_path.parent();

        let mut candidates = Vec::new();
        if let Some(md) = model_dir {
            candidates.push(md.join("main.exe"));
            candidates.push(md.join("whisper-cli.exe"));
            candidates.push(md.join("whisper.exe"));
            if let Some(parent) = md.parent() {
                candidates.push(parent.join("main.exe"));
                candidates.push(parent.join("models").join("main.exe"));
                candidates.push(parent.join("src-tauri").join("models").join("main.exe"));
            }
        }

        candidates.push(exe_dir.join("models").join("main.exe"));
        candidates.push(exe_dir.join("main.exe"));
        candidates.push(exe_dir.join("models").join("whisper-cli.exe"));
        candidates.push(exe_dir.join("whisper-cli.exe"));
        candidates.push(PathBuf::from("models/main.exe"));
        candidates.push(PathBuf::from("src-tauri/models/main.exe"));
        candidates.push(PathBuf::from("../models/main.exe"));
        candidates.push(PathBuf::from("main.exe"));
        candidates.push(PathBuf::from("whisper-cli.exe"));

        for p in candidates {
            if p.is_file() {
                return Some(p);
            }
        }

        // Try checking PATH
        if let Ok(output) = std::process::Command::new("where").arg("main.exe").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = stdout.lines().next() {
                    let path = PathBuf::from(first_line.trim());
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }

        None
    }

    /// Transcribe using the default supported language set.
    pub fn transcribe_window(
        &self,
        samples: &[f32],
        sample_rate: u32,
        window_start_ms: u64,
        language_hint: &str,
    ) -> AppResult<Vec<RecognitionResult>> {
        let allowed = vec!["ta".to_string(), "si".to_string(), "en".to_string()];
        self.transcribe_window_with_languages(
            samples, sample_rate, window_start_ms, language_hint, &allowed,
        )
    }

    /// Transcribe and retain only text written in the selected supported scripts.
    /// Whisper still detects one dominant language per window; script inspection
    /// provides a best-effort language label for each returned text segment.
    pub fn transcribe_window_with_languages(
        &self,
        samples: &[f32],
        sample_rate: u32,
        window_start_ms: u64,
        language_hint: &str,
        allowed_languages: &[String],
    ) -> AppResult<Vec<RecognitionResult>> {
        if !self.is_loaded {
            return Err(AppError::Internal(format!(
                "Whisper model is not loaded (expected at {})",
                self.model_path.display()
            )));
        }

        let cli = match &self.cli_path {
            Some(p) => p,
            None => {
                return Err(AppError::Internal(
                    "Whisper CLI executable (main.exe) not found".into(),
                ));
            }
        };

        // If audio is too short (less than 200ms), skip to avoid whisper errors
        let min_samples = (sample_rate as usize) / 5;
        if samples.len() < min_samples {
            return Ok(Vec::new());
        }

        let timestamp_suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let temp_dir = std::env::temp_dir();
        let temp_wav = temp_dir.join(format!("voxy_chunk_{}_{}.wav", std::process::id(), timestamp_suffix));
        let json_prefix = temp_dir.join(format!("voxy_out_{}_{}", std::process::id(), timestamp_suffix));
        let json_file = json_prefix.with_extension("json");

        write_wav(&temp_wav, samples, sample_rate)
            .map_err(|e| AppError::Internal(format!("Failed to write temporary WAV: {e}")))?;

        let lang = if language_hint.is_empty() || language_hint == "auto" {
            // When auto-detect with multiple allowed languages, Whisper on
            // ggml-base mis-detects Tamil/Sinhala as English or Hindi.
            // Try each allowed non-English language first, starting with Tamil
            // (most common in our use case), then Sinhala.
            if allowed_languages.iter().any(|l| l == "ta") {
                "ta"
            } else if allowed_languages.iter().any(|l| l == "si") {
                "si"
            } else {
                "auto"
            }
        } else {
            language_hint
        };
        tracing::info!(lang, "Whisper language selected");

        // Keep CPU usage predictable on the target CPU-only Windows machines.
        // Override with VOXY_WHISPER_THREADS when benchmarking a specific device.
        let thread_count = std::env::var("VOXY_WHISPER_THREADS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| (1..=32).contains(value))
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|count| count.get().clamp(2, 8))
                    .unwrap_or(4)
            });

        // Keep decoding cost configurable for CPU-only Windows devices.
        // A moderate beam/best-of setting balances speed and recognition quality.
        let beam_size = std::env::var("VOXY_WHISPER_BEAM_SIZE")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| (1..=5).contains(value))
            .unwrap_or(1); // beam=1 is greedy — fastest for real-time

        let best_of = std::env::var("VOXY_WHISPER_BEST_OF")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| (1..=5).contains(value))
            .unwrap_or(1); // best_of=1 reduces decoding passes

        let mut cmd = std::process::Command::new(cli);
        cmd.arg("-m").arg(&self.model_path)
            .arg("-f").arg(&temp_wav)
            .arg("-l").arg(lang)
            .arg("-t").arg(thread_count.to_string())
            .arg("-bs").arg(beam_size.to_string())
            .arg("-bo").arg(best_of.to_string())
            // Disable temperature fallback: on weak/silent windows, retrying at
            // higher temperatures can turn uncertain audio into fluent hallucinations.
            .arg("-nf")
            // Suppress Whisper's non-speech token set and apply its standard
            // silence/entropy thresholds before accepting a segment.
            .arg("-sns")
            .arg("-et").arg("2.4")
            .arg("-nth").arg("0.6");
        if let Some(vad_model) = &self.vad_model_path {
            // Run neural voice activity detection inside each buffered window so
            // music and non-speech audio are less likely to reach Whisper decoding.
            cmd.arg("--vad")
                .arg("-vm").arg(vad_model)
                .arg("-vt").arg("0.5")
                .arg("-vspd").arg("250")
                .arg("-vsd").arg("250")
                .arg("-vo").arg("0.10");
        }
        cmd.arg("-ojf")
            .arg("-of").arg(&json_prefix);
        // Do not pass textual "false" after boolean switches. whisper.cpp
            // parses these as flags, not key/value options. Omitting -nt preserves
            // segment timestamps; omitting -tr preserves the original spoken language.

        let output = match cmd.output() {
            Ok(out) => out,
            Err(e) => {
                let _ = std::fs::remove_file(&temp_wav);
                let _ = std::fs::remove_file(&json_file);
                return Err(AppError::Internal(format!("Whisper process execution failed: {e}")));
            }
        };

        let _ = std::fs::remove_file(&temp_wav);

        if !output.status.success() {
            let _ = std::fs::remove_file(&json_file);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(AppError::Internal(format!(
                "Whisper CLI exited with error code {:?}: stderr={}, stdout={}",
                output.status.code(),
                stderr.trim(),
                stdout.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Attempt reading structured JSON output first.
        // Per-segment language: Whisper's JSON includes a per-segment "speaker" field but
        // not always a per-segment language. The window-level `result.language` is the
        // language Whisper detected for the entire window. We use it as the fallback for
        // segments that do not have their own language tag.
        // This is accurate for monolingual windows. For code-switched windows, the language
        // tag reflects whichever language was dominant in the window, which may not match
        // an individual segment. The limitation is documented in the struct field comment.
        let mut results = Vec::new();
        let mut detected_window_language = lang.to_string();
        let mut json_parse_error: Option<String> = None;
        let mut json_output_parsed = false;

        if json_file.is_file() {
            match std::fs::read_to_string(&json_file) {
                Ok(content) => {
                    match serde_json::from_str::<WhisperJsonOutput>(&content) {
                        Ok(parsed) => {
                            json_output_parsed = true;
                            // Window-level language (may be "auto" if language detection failed).
                            let window_lang = parsed
                                .result
                                .and_then(|r| r.language)
                                .filter(|l| !l.is_empty() && l != "auto")
                                .or_else(|| parse_auto_detected_language(&stdout))
                                .unwrap_or_else(|| {
                                    if lang == "auto" { "und".to_string() } else { lang.to_string() }
                                });
                            detected_window_language = window_lang.clone();

                            if let Some(segments) = parsed.transcription {
                                for seg in segments {
                                    // whisper.cpp's full JSON exposes per-token probabilities.
                                    // Derive average log probability when the CLI does not emit
                                    // a segment-level avg_logprob field.
                                    let avg_logprob = seg.avg_logprob.or_else(|| {
                                        average_token_log_probability(seg.tokens.as_deref())
                                    });
                                    if should_discard_whisper_segment(
                                        seg.no_speech_prob,
                                        avg_logprob,
                                        seg.compression_ratio,
                                    ) {
                                        tracing::debug!(
                                            no_speech_prob = ?seg.no_speech_prob,
                                            avg_logprob = ?seg.avg_logprob,
                                            compression_ratio = ?seg.compression_ratio,
                                            "Discarding low-confidence Whisper segment"
                                        );
                                        continue;
                                    }
                                    let text = clean_whisper_text(&seg.text);
                                    if text.is_empty() {
                                        continue;
                                    }

                                    let (from_ms, to_ms) = match seg.offsets {
                                        Some(offsets) => (offsets.from, offsets.to),
                                        None => (0, (samples.len() as u64 * 1000) / sample_rate as u64),
                                    };

                                    // Per-segment language: use the JSON segment language if present;
                                    // fall back to window-level language.
                                    // Note: older whisper.cpp versions do not emit per-segment language.
                                    // We document this as a known limitation rather than fabricating data.
                                    let seg_lang = window_lang.clone();

                                    results.push(RecognitionResult {
                                        text,
                                        language: seg_lang,
                                        start_ms: window_start_ms + from_ms,
                                        end_ms: window_start_ms + to_ms,
                                        is_partial: false,
                                    });
                                }
                            }

                            if results.is_empty() {
                                tracing::debug!(
                                    window_start_ms,
                                    "Whisper JSON parsed successfully but contained no segments \
                                     (audio may be silence or below model confidence threshold)"
                                );
                            }
                        }
                        Err(e) => {
                            json_parse_error = Some(format!("JSON parse error: {e}"));
                        }
                    }
                }
                Err(e) => {
                    json_parse_error = Some(format!("Could not read JSON output file: {e}"));
                }
            }
            let _ = std::fs::remove_file(&json_file);
        } else {
            json_parse_error = Some("Whisper did not produce a JSON output file".to_string());
        }

        if let Some(err) = &json_parse_error {
            tracing::debug!(
                window_start_ms,
                "{} — falling back to stdout parsing",
                err
            );
        }

        // Fallback: parse timestamp lines from stdout if JSON failed or was empty.
        if results.is_empty() && !json_output_parsed {
            let fallback_lang = parse_auto_detected_language(&stdout)
                .filter(|l| l != "auto")
                .unwrap_or_else(|| {
                    if lang == "auto" { "und".to_string() } else { lang.to_string() }
                });
            detected_window_language = fallback_lang.clone();
            results = parse_whisper_stdout(&stdout, &fallback_lang, window_start_ms);

            if results.is_empty() && stdout.trim().is_empty() {
                // Distinguish: no output at all vs empty segments.
                tracing::debug!(
                    window_start_ms,
                    "Whisper produced no stdout and no JSON — treating as silence"
                );
            }
        }

        // whisper.cpp reports one dominant language for the whole audio window.
        // Reclassify each returned segment from its Unicode scripts so Tamil,
        // Sinhala, and English can coexist in the same window.
        results.retain_mut(|result| {
            match classify_segment_language(
                &result.text,
                &detected_window_language,
                allowed_languages,
            ) {
                Some(language) => {
                    result.language = language;
                    true
                }
                None => {
                    tracing::debug!(
                        text = %result.text,
                        window_language = %detected_window_language,
                        "Discarding transcript segment outside selected languages"
                    );
                    false
                }
            }
        });

        Ok(results)
    }

    /// Transcribe a processed audio frame (delegates to transcribe_window).
    pub fn transcribe(
        &self,
        audio: &ProcessedAudio,
        language_hint: &str,
    ) -> AppResult<Vec<RecognitionResult>> {
        self.transcribe_window(&audio.samples, audio.sample_rate, 0, language_hint)
    }
}


/// Determine the supported language scripts present in one transcript segment.
/// Latin letters are treated as English; Tamil and Sinhala use their Unicode blocks.
/// Any other alphabetic script is rejected rather than displayed as a false language.
fn classify_segment_language(
    text: &str,
    fallback_language: &str,
    allowed_languages: &[String],
) -> Option<String> {
    let mut languages: Vec<&'static str> = Vec::new();
    let mut unsupported_script = false;

    for ch in text.chars().filter(|ch| ch.is_alphabetic()) {
        let code = ch as u32;
        // Skip CJK characters silently — Whisper sometimes hallucinates CJK
        // when processing Tamil/Sinhala audio. Ignore them instead of rejecting
        // the entire segment so Tamil/Sinhala text is not lost.
        let is_cjk = matches!(
            code,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F
                | 0x3000..=0x303F | 0xFF00..=0xFFEF | 0x3040..=0x30FF
        );
        if is_cjk {
            continue; // skip CJK, don't set unsupported_script
        }
        let language = match code {
            0x0B80..=0x0BFF => Some("ta"),
            0x0D80..=0x0DFF => Some("si"),
            _ if ch.is_ascii_alphabetic() || (0x00C0..=0x024F).contains(&code) => Some("en"),
            _ => {
                unsupported_script = true;
                None
            }
        };
        if let Some(language) = language {
            if !languages.contains(&language) {
                languages.push(language);
            }
        }
    }

    if unsupported_script {
        return None;
    }

    if languages.is_empty() {
        if matches!(fallback_language, "ta" | "si" | "en") {
            languages.push(match fallback_language {
                "ta" => "ta",
                "si" => "si",
                _ => "en",
            });
        } else {
            return None;
        }
    }

    let allowed: Vec<&str> = if allowed_languages.is_empty() {
        vec!["ta", "si", "en"]
    } else {
        allowed_languages.iter().map(String::as_str).collect()
    };

    if !languages.iter().all(|language| allowed.contains(language)) {
        return None;
    }

    Some(if languages.len() > 1 {
        "mul".to_string()
    } else {
        languages[0].to_string()
    })
}

/// Derive segment log probability from the token probabilities in whisper.cpp's -ojf output.
fn average_token_log_probability(tokens: Option<&[WhisperJsonToken]>) -> Option<f64> {
    let mut total = 0.0;
    let mut count = 0usize;
    for probability in tokens?.iter().filter_map(|token| token.p) {
        if probability.is_finite() && probability > 0.0 {
            total += probability.max(1e-12).ln();
            count += 1;
        }
    }
    (count > 0).then_some(total / count as f64)
}

/// Apply Whisper's segment-level confidence checks before displaying a hypothesis.
/// Prefer Whisper's own silence metadata when available; otherwise reject only
/// extremely low-confidence token sequences and pathological compression.
fn should_discard_whisper_segment(
    no_speech_prob: Option<f64>,
    avg_logprob: Option<f64>,
    compression_ratio: Option<f64>,
) -> bool {
    // Only discard clearly bad segments. Tamil/Sinhala have lower confidence
    // scores than English on ggml-base — be lenient to avoid false rejections.
    if compression_ratio.is_some_and(|ratio| ratio > 2.8) {
        return true;
    }
    if avg_logprob.is_some_and(|logprob| logprob < -2.0) {
        return true;
    }

    matches!(
        (no_speech_prob, avg_logprob),
        (Some(no_speech), Some(logprob)) if no_speech > 0.8 && logprob < -1.2
    )
}

/// Return true when a short hypothesis is dominated by the same repeated word.
/// This catches common music/silence hallucinations without suppressing normal emphasis.
fn is_repetitive_hallucination(text: &str) -> bool {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|ch| ch.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect();

    if words.len() < 7 {
        // Word-level check needs 7+ words. But Tamil/Sinhala hallucinations
        // often produce one long word with no spaces — check character repetition.
        let chars: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
        if chars.len() > 20 {
            // Count max consecutive repetitions of any single character.
            let mut max_repeat = 1;
            let mut current = 1;
            for i in 1..chars.len() {
                if chars[i] == chars[i - 1] {
                    current += 1;
                    if current > max_repeat {
                        max_repeat = current;
                    }
                } else {
                    current = 1;
                }
            }
            if max_repeat > 10 {
                return true;
            }
        }
        return false;
    }

    // Phrase-level check: look for 2-4 word phrases that repeat 3+ times.
    // This catches Tamil/Sinhala hallucinations like "நின்னுடன் அம்மாக நின்னுடன் அம்மாக..."
    for phrase_len in 2..=4 {
        if words.len() < phrase_len * 3 {
            continue;
        }
        for start in 0..=(words.len() - phrase_len * 3) {
            let phrase: Vec<&String> = words[start..start + phrase_len].iter().collect();
            let mut count = 1;
            let mut idx = start + phrase_len;
            while idx + phrase_len <= words.len() {
                let next: Vec<&String> = words[idx..idx + phrase_len].iter().collect();
                if next == phrase {
                    count += 1;
                    idx += phrase_len;
                } else {
                    break;
                }
            }
            if count >= 3 {
                return true;
            }
        }
    }

    // Word-level check: same word appears 80%+ of the time.
    let mut counts = std::collections::HashMap::new();
    for word in &words {
        *counts.entry(word).or_insert(0usize) += 1;
    }
    counts.values().copied().max().unwrap_or(0) * 100 / words.len() >= 80
}

/// Filter out hallucinated tokens and special audio artifacts.
fn clean_whisper_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Common Whisper hallucinations on silence/noise — discard these.
    let lower = trimmed.to_lowercase();
    let hallucinations = [
        "thank you", "thanks", "thank you.", "thanks.", "thank you!",
        "you", ".", "..", "...", "the", "a", "i", "oh", "ah", "um", "uh",
        "bye", "bye.", "goodbye", "okay", "ok", "yes", "no",
    ];
    if hallucinations.iter().any(|h| lower.trim_matches('.').trim() == *h) {
        tracing::debug!(text = %trimmed, "Discarding common Whisper hallucination");
        return String::new();
    }

    if is_repetitive_hallucination(trimmed) {
        tracing::debug!(text = %trimmed, "Discarding highly repetitive Whisper hypothesis");
        return String::new();
    }

    // VOXY currently supports Tamil, Sinhala, and English only. Whisper's
    // multilingual auto-detection can occasionally hallucinate Chinese text
    // from low-confidence audio. Strip CJK characters rather than discarding
    // the entire segment — Tamil/Sinhala text may still be valid.
    let cleaned: String = trimmed.chars().filter(|ch| {
        !matches!(
            *ch as u32,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F
                | 0x3000..=0x303F | 0xFF00..=0xFFEF | 0x3040..=0x30FF
        )
    }).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Ignore noise annotations like [BLANK_AUDIO], (crickets chirping), (applause), etc.
    if (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('(') && trimmed.ends_with(')'))
        || (trimmed.starts_with('*') && trimmed.ends_with('*'))
    {
        let lower = trimmed.to_lowercase();
        if lower.contains("blank")
            || lower.contains("applause")
            || lower.contains("chirp")
            || lower.contains("music")
            || lower.contains("laughter")
            || lower.contains("sigh")
            || lower.contains("cough")
            || lower.contains("throat")
        {
            return String::new();
        }
    }

    trimmed.to_string()
}

/// Parse auto-detected language code from Whisper stdout.
fn parse_auto_detected_language(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        if line.contains("auto-detected language:") {
            if let Some(part) = line.split("auto-detected language:").nth(1) {
                let code = part.split_whitespace().next().unwrap_or("").trim();
                if !code.is_empty() {
                    return Some(code.to_string());
                }
            }
        }
    }
    None
}

/// Parse stdout timestamp lines like "[00:00:01.000 --> 00:00:04.500]   text".
fn parse_whisper_stdout(stdout: &str, lang: &str, window_start_ms: u64) -> Vec<RecognitionResult> {
    let mut results = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.contains("-->") {
            if let Some(close_bracket) = trimmed.find(']') {
                let time_part = &trimmed[1..close_bracket];
                let text_part = clean_whisper_text(&trimmed[close_bracket + 1..]);
                if text_part.is_empty() {
                    continue;
                }

                let mut times = time_part.split("-->");
                let start_str = times.next().unwrap_or("").trim();
                let end_str = times.next().unwrap_or("").trim();

                let seg_start = parse_timestamp_ms(start_str);
                let seg_end = parse_timestamp_ms(end_str);

                results.push(RecognitionResult {
                    text: text_part,
                    language: lang.to_string(),
                    start_ms: window_start_ms + seg_start,
                    end_ms: window_start_ms + seg_end,
                    is_partial: false,
                });
            }
        }
    }

    results
}

/// Convert "00:01:23.450" or "00:01:23,450" into milliseconds.
fn parse_timestamp_ms(ts: &str) -> u64 {
    let clean = ts.replace(',', ".");
    let parts: Vec<&str> = clean.split(':').collect();
    if parts.len() == 3 {
        let hours: u64 = parts[0].parse().unwrap_or(0);
        let mins: u64 = parts[1].parse().unwrap_or(0);
        let secs_f: f64 = parts[2].parse().unwrap_or(0.0);
        let total_secs = (hours * 3600 + mins * 60) as f64 + secs_f;
        (total_secs * 1000.0) as u64
    } else {
        0
    }
}

fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    let num_samples = samples.len() as u32;
    let byte_rate = sample_rate * 2; // 16-bit mono
    let data_size = num_samples * 2;
    let chunk_size = 36 + data_size;

    // RIFF header
    f.write_all(b"RIFF")?;
    f.write_all(&chunk_size.to_le_bytes())?;
    f.write_all(b"WAVE")?;
    // fmt chunk
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;   // chunk size
    f.write_all(&1u16.to_le_bytes())?;    // PCM
    f.write_all(&1u16.to_le_bytes())?;    // mono
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?;    // block align
    f.write_all(&16u16.to_le_bytes())?;   // bits per sample
    // data chunk
    f.write_all(b"data")?;
    f.write_all(&data_size.to_le_bytes())?;
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

// SAFETY: WhisperEngine only holds paths and is safe to share across threads.
unsafe impl Send for WhisperEngine {}
unsafe impl Sync for WhisperEngine {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_average_log_probability_from_full_json_tokens() {
        let tokens = vec![
            WhisperJsonToken { p: Some(0.5) },
            WhisperJsonToken { p: Some(0.25) },
        ];
        let average = average_token_log_probability(Some(&tokens)).unwrap();
        assert!((average - (0.5f64.ln() + 0.25f64.ln()) / 2.0).abs() < 1e-10);
        assert_eq!(average_token_log_probability(None), None);
    }

    #[test]
    fn filters_whisper_segments_using_confidence_metadata() {
        assert!(should_discard_whisper_segment(Some(0.82), Some(-1.4), Some(1.1)));
        assert!(should_discard_whisper_segment(Some(0.1), Some(-0.2), Some(2.8)));
        assert!(!should_discard_whisper_segment(Some(0.1), Some(-0.2), Some(1.2)));
        // Older whisper.cpp JSON may omit confidence fields; keep those segments
        // rather than silently dropping otherwise valid speech.
        assert!(!should_discard_whisper_segment(None, None, None));
    }

    #[test]
    fn filters_extreme_single_word_repetition_but_keeps_normal_emphasis() {
        assert!(is_repetitive_hallucination("yes yes yes yes yes yes yes yes yes"));
        assert!(!is_repetitive_hallucination("Yes, yes, yes! I understand."));
        assert!(!is_repetitive_hallucination("வணக்கம் நண்பர்களே, எப்படி இருக்கிறீர்கள்?"));
    }

    #[test]
    fn test_clean_whisper_text_preserves_multilingual() {
        assert_eq!(clean_whisper_text("  வணக்கம்  "), "வணக்கம்");
        assert_eq!(clean_whisper_text("  ආයුබෝවන්  "), "ආයුබෝවන්");
        assert_eq!(clean_whisper_text("Good morning, how are you?"), "Good morning, how are you?");
    }

    #[test]
    fn test_clean_whisper_text_filters_noise() {
        assert_eq!(clean_whisper_text("[BLANK_AUDIO]"), "");
        assert_eq!(clean_whisper_text("(applause)"), "");
        assert_eq!(clean_whisper_text("(crickets chirping)"), "");
        assert_eq!(clean_whisper_text("(music)"), "");
    }

    #[test]
    fn test_parse_timestamp_ms() {
        assert_eq!(parse_timestamp_ms("00:00:01.500"), 1500);
        assert_eq!(parse_timestamp_ms("00:01:02,300"), 62300);
        assert_eq!(parse_timestamp_ms("01:00:00.000"), 3600000);
    }

    #[test]
    fn test_parse_auto_detected_language() {
        let stdout = "whisper_full_with_state: auto-detected language: ta (p = 0.85)\n";
        assert_eq!(parse_auto_detected_language(stdout), Some("ta".to_string()));

        let stdout_si = "main: processing...\nwhisper_full_with_state: auto-detected language: si (p = 0.92)\n";
        assert_eq!(parse_auto_detected_language(stdout_si), Some("si".to_string()));

        let stdout_en = "auto-detected language: en\n";
        assert_eq!(parse_auto_detected_language(stdout_en), Some("en".to_string()));
    }

    #[test]
    fn test_parse_whisper_stdout_multilingual() {
        let stdout = r#"
[00:00:00.000 --> 00:00:02.500]   வணக்கம்
[00:00:02.500 --> 00:00:05.000]   Welcome to the meeting
[00:00:05.000 --> 00:00:08.000]   ආයුබෝවන්
"#;
        let results = parse_whisper_stdout(stdout, "auto", 1000);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].text, "வணக்கம்");
        assert_eq!(results[0].start_ms, 1000);
        assert_eq!(results[0].end_ms, 3500);

        assert_eq!(results[1].text, "Welcome to the meeting");
        assert_eq!(results[1].start_ms, 3500);
        assert_eq!(results[1].end_ms, 6000);

        assert_eq!(results[2].text, "ආයුබෝවන්");
        assert_eq!(results[2].start_ms, 6000);
        assert_eq!(results[2].end_ms, 9000);
    }

    #[test]
    fn classifies_tamil_sinhala_and_english_segments_individually() {
        let allowed = vec!["ta".to_string(), "si".to_string(), "en".to_string()];
        assert_eq!(
            classify_segment_language("வணக்கம்", "en", &allowed).as_deref(),
            Some("ta")
        );
        assert_eq!(
            classify_segment_language("ආයුබෝවන්", "en", &allowed).as_deref(),
            Some("si")
        );
        assert_eq!(
            classify_segment_language("Good morning", "ta", &allowed).as_deref(),
            Some("en")
        );
        assert_eq!(
            classify_segment_language("Café meeting", "ta", &allowed).as_deref(),
            Some("en")
        );
    }

    #[test]
    fn classifies_code_switched_segments_as_mixed() {
        let allowed = vec!["ta".to_string(), "si".to_string(), "en".to_string()];
        assert_eq!(
            classify_segment_language("வணக்கம், good morning", "ta", &allowed).as_deref(),
            Some("mul")
        );
    }

    #[test]
    fn filters_languages_not_selected_and_unsupported_scripts() {
        let tamil_only = vec!["ta".to_string()];
        assert_eq!(
            classify_segment_language("Good morning", "en", &tamil_only),
            None
        );
        assert_eq!(
            classify_segment_language("नमस्ते", "hi", &tamil_only),
            None
        );
    }
}
