//! Speech recognition module — owned by Member 5.
//!
//! Uses whisper.cpp CLI binary for transcription to avoid Windows build
//! issues with whisper-rs-sys bindgen on MSVC + LLVM 23.
//! When the CLI binary is not found, the engine returns empty results (stub mode).

pub mod config;
pub mod evaluation;

use crate::audio_processing::ProcessedAudio;
use crate::core::error::{AppError, AppResult};
use std::io::Write;

/// A recognized text span with timing metadata.
#[derive(Debug, Clone)]
pub struct RecognitionResult {
    pub text: String,
    pub language: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub is_partial: bool,
}

/// Handle to the Whisper inference engine.
pub struct WhisperEngine {
    model_path: std::path::PathBuf,
    is_loaded: bool,
    /// Path to whisper.cpp CLI binary (whisper-cli.exe or main.exe)
    cli_path: Option<std::path::PathBuf>,
}

impl WhisperEngine {
    /// Load a whisper.cpp model from the given path.
    pub fn new(model_path: &std::path::Path) -> AppResult<Self> {
        let exists = model_path.is_file();
        if !exists {
            tracing::warn!(path = ?model_path, "Whisper model not found — stub mode");
        }

        // Look for whisper CLI binary next to the model or in PATH
        let cli_path = Self::find_cli(model_path);
        if let Some(ref p) = cli_path {
            tracing::info!(cli = ?p, "Found whisper.cpp CLI");
        } else {
            tracing::warn!("whisper.cpp CLI not found — will use stub transcription");
        }

        Ok(Self {
            model_path: model_path.to_path_buf(),
            is_loaded: exists,
            cli_path,
        })
    }

    /// Create a stub engine with no model loaded.
    pub fn stub() -> Self {
        Self {
            model_path: std::path::PathBuf::new(),
            is_loaded: false,
            cli_path: None,
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.is_loaded
    }

    fn find_cli(model_path: &std::path::Path) -> Option<std::path::PathBuf> {
        let model_dir = model_path.parent()?;
        let candidates = [
            model_dir.join("whisper-cli.exe"),
            model_dir.join("main.exe"),
            model_dir.parent()?.join("whisper-cli.exe"),
            model_dir.parent()?.join("main.exe"),
            std::path::PathBuf::from("whisper-cli.exe"),
            std::path::PathBuf::from("main.exe"),
        ];
        candidates.into_iter().find(|p| p.is_file())
    }

    /// Transcribe processed audio into recognition results.
    pub fn transcribe(
        &self,
        audio: &ProcessedAudio,
        language_hint: &str,
    ) -> AppResult<Vec<RecognitionResult>> {
        if !self.is_loaded {
            return Ok(Vec::new());
        }

        let cli = match &self.cli_path {
            Some(p) => p,
            None => {
                tracing::debug!("No whisper CLI — returning empty results");
                return Ok(Vec::new());
            }
        };

        // Write PCM samples to a temp WAV file
        let temp_wav = std::env::temp_dir().join(format!(
            "voxy_chunk_{}.wav",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));

        write_wav(&temp_wav, &audio.samples, audio.sample_rate)
            .map_err(|e| AppError::Internal(format!("WAV write failed: {e}")))?;

        // Run whisper CLI
        let lang = if language_hint.is_empty() || language_hint == "auto" { "auto" } else { language_hint };

        let output = std::process::Command::new(cli)
            .arg("--model").arg(&self.model_path)
            .arg("--file").arg(&temp_wav)
            .arg("--language").arg(lang)
            .arg("--output-txt")
            .arg("--no-timestamps")
            .arg("--print-special").arg("false")
            .output()
            .map_err(|e| AppError::Internal(format!("whisper CLI failed: {e}")))?;

        let _ = std::fs::remove_file(&temp_wav);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(stderr = %stderr, "whisper CLI exited with error");
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let results = parse_whisper_output(&stdout, lang);
        Ok(results)
    }
}

fn write_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) -> std::io::Result<()> {
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

fn parse_whisper_output(stdout: &str, lang: &str) -> Vec<RecognitionResult> {
    let mut results = Vec::new();
    for line in stdout.lines() {
        let text = line.trim().to_string();
        if text.is_empty() || text.starts_with('[') { continue; }
        results.push(RecognitionResult {
            text,
            language: lang.to_string(),
            start_ms: 0,
            end_ms: 0,
            is_partial: false,
        });
    }
    results
}

// SAFETY: WhisperEngine only holds paths and is safe to share.
unsafe impl Send for WhisperEngine {}
unsafe impl Sync for WhisperEngine {}
