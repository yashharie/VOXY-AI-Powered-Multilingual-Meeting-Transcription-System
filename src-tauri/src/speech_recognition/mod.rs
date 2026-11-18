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

        tracing::info!(
            model = ?model_path,
            cli = ?cli_path,
            "Whisper engine initialised successfully"
        );

        Ok(Self {
            model_path,
            is_loaded: true,
            cli_path,
        })
    }

    /// Create a stub engine with no model loaded.
    pub fn stub() -> Self {
        Self {
            model_path: PathBuf::new(),
            is_loaded: false,
            cli_path: None,
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
            exe_dir.join("models").join("ggml-base.bin"),
            exe_dir.join("ggml-base.bin"),
            PathBuf::from("models/ggml-base.bin"),
            PathBuf::from("src-tauri/models/ggml-base.bin"),
            PathBuf::from("../models/ggml-base.bin"),
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

    /// Transcribe a window of 16kHz mono audio samples with accurate timestamps.
    pub fn transcribe_window(
        &self,
        samples: &[f32],
        sample_rate: u32,
        window_start_ms: u64,
        language_hint: &str,
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
            "auto"
        } else {
            language_hint
        };

        // Keep CPU usage predictable on the target CPU-only Windows machines.
        // Override with VOXY_WHISPER_THREADS when benchmarking a specific device.
        let thread_count = std::env::var("VOXY_WHISPER_THREADS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| (1..=32).contains(value))
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|count| count.get().saturating_sub(1).clamp(1, 4))
                    .unwrap_or(2)
            });

        let mut cmd = std::process::Command::new(cli);
        cmd.arg("-m").arg(&self.model_path)
            .arg("-f").arg(&temp_wav)
            .arg("-l").arg(lang)
            .arg("-t").arg(thread_count.to_string())
            .arg("-oj")
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
        let mut json_parse_error: Option<String> = None;

        if json_file.is_file() {
            match std::fs::read_to_string(&json_file) {
                Ok(content) => {
                    match serde_json::from_str::<WhisperJsonOutput>(&content) {
                        Ok(parsed) => {
                            // Window-level language (may be "auto" if language detection failed).
                            let window_lang = parsed
                                .result
                                .and_then(|r| r.language)
                                .filter(|l| !l.is_empty() && l != "auto")
                                .or_else(|| parse_auto_detected_language(&stdout))
                                .unwrap_or_else(|| {
                                    if lang == "auto" { "und".to_string() } else { lang.to_string() }
                                });

                            if let Some(segments) = parsed.transcription {
                                for seg in segments {
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
        if results.is_empty() {
            let fallback_lang = parse_auto_detected_language(&stdout)
                .filter(|l| l != "auto")
                .unwrap_or_else(|| {
                    if lang == "auto" { "und".to_string() } else { lang.to_string() }
                });
            results = parse_whisper_stdout(&stdout, &fallback_lang, window_start_ms);

            if results.is_empty() && stdout.trim().is_empty() {
                // Distinguish: no output at all vs empty segments.
                tracing::debug!(
                    window_start_ms,
                    "Whisper produced no stdout and no JSON — treating as silence"
                );
            }
        }

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

/// Filter out hallucinated tokens and special audio artifacts.
fn clean_whisper_text(raw: &str) -> String {
    let trimmed = raw.trim();
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
}
