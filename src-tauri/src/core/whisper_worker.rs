//! Python whisper worker bridge — spawns whisper_worker.py as a persistent
//! subprocess and communicates via JSON Lines over stdin/stdout.
//!
//! This replaces the whisper.cpp CLI approach with faster-whisper + a
//! Tamil fine-tuned model for significantly better Tamil accuracy.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::error::{AppError, AppResult};

/// One transcription request sent to the Python worker.
#[derive(Debug, Serialize)]
struct WhisperRequest {
    wav_path: String,
    language: String,
    start_ms: u64,
}

/// One segment returned by the Python worker.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkerSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub language: String,
}

/// Full transcription response from the Python worker.
#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
    language: String,
    start_ms: u64,
    end_ms: u64,
    segments: Vec<WorkerSegment>,
    #[serde(default)]
    error: Option<String>,
}

/// Persistent Python whisper worker process.
pub struct WhisperWorker {
    child: Mutex<Option<Child>>,
    script_path: PathBuf,
    is_ready: Mutex<bool>,
}

impl WhisperWorker {
    /// Find the whisper_worker.py script.
    fn find_script() -> Option<PathBuf> {
        let candidates = [
            // Relative to src-tauri (dev mode)
            PathBuf::from("whisper_worker.py"),
            PathBuf::from("src-tauri/whisper_worker.py"),
            // Relative to exe (production)
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("whisper_worker.py")))
                .unwrap_or_default(),
        ];

        for p in &candidates {
            if p.is_file() {
                return Some(p.clone());
            }
        }
        None
    }

    /// Create a new WhisperWorker (does not start the process yet).
    pub fn new() -> Self {
        let script_path = Self::find_script().unwrap_or_else(|| {
            tracing::warn!("whisper_worker.py not found — falling back to whisper.cpp CLI");
            PathBuf::new()
        });

        Self {
            child: Mutex::new(None),
            script_path,
            is_ready: Mutex::new(false),
        }
    }

    /// Start the Python worker subprocess.
    pub fn ensure_started(&self) -> AppResult<()> {
        let mut child_guard = self.child.lock().map_err(|e| {
            AppError::Internal(format!("WhisperWorker lock error: {e}"))
        })?;

        if child_guard.is_some() {
            return Ok(());
        }

        if !self.script_path.is_file() {
            return Err(AppError::Internal(
                "whisper_worker.py script not found".into(),
            ));
        }

        tracing::info!(
            script = %self.script_path.display(),
            "Starting Python whisper worker"
        );

        let mut cmd = Command::new("python");
        cmd.arg(&self.script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let child = cmd.spawn()
            .map_err(|e| AppError::Internal(format!(
                "Failed to start whisper worker: {e}. Is Python installed?"
            )))?;

        *child_guard = Some(child);
        *self.is_ready.lock().map_err(|e| {
            AppError::Internal(format!("is_ready lock error: {e}"))
        })? = true;

        tracing::info!("Python whisper worker started");

        // Give it a moment to load the model
        std::thread::sleep(std::time::Duration::from_secs(2));

        Ok(())
    }

    /// Transcribe a WAV file by sending it to the Python worker.
    /// Returns the segments produced by faster-whisper.
    pub fn transcribe(
        &self,
        wav_path: &Path,
        language: &str,
        start_ms: u64,
    ) -> AppResult<Vec<WorkerSegment>> {
        self.ensure_started()?;

        let mut guard = self.child.lock().map_err(|e| {
            AppError::Internal(format!("WhisperWorker lock error: {e}"))
        })?;

        let child = guard.as_mut().ok_or_else(|| {
            AppError::Internal("Whisper worker not running".into())
        })?;

        // Send request as JSON line
        let request = WhisperRequest {
            wav_path: wav_path.to_string_lossy().to_string(),
            language: language.to_string(),
            start_ms,
        };

        let json_line = serde_json::to_string(&request)
            .map_err(|e| AppError::SerializationError(e.to_string()))?;

        if let Some(stdin) = child.stdin.as_mut() {
            writeln!(stdin, "{}", json_line)
                .map_err(|e| AppError::Internal(format!("Write to worker failed: {e}")))?;
            stdin.flush()
                .map_err(|e| AppError::Internal(format!("Flush to worker failed: {e}")))?;
        }

        // Read response line from stdout
        let stdout_reader = BufReader::new(child.stdout.as_mut().ok_or_else(|| {
            AppError::Internal("Cannot read whisper worker stdout".into())
        })?);

        // Read lines until we get a valid JSON response
        let mut reader = stdout_reader;
        for _ in 0..100 {
            let mut line = String::new();
            let n = reader.read_line(&mut line)
                .map_err(|e| AppError::Internal(format!("Read from worker failed: {e}")))?;
            if n == 0 {
                return Err(AppError::Internal("Whisper worker closed stdout".into()));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() || !trimmed.starts_with('{') {
                continue;
            }

            let response: WhisperResponse = serde_json::from_str(trimmed)
                .map_err(|e| AppError::Internal(format!(
                    "Invalid worker response: {e}\nRaw: {trimmed}"
                )))?;

            if let Some(err) = response.error {
                return Err(AppError::Internal(format!("Whisper worker error: {err}")));
            }

            return Ok(response.segments);
        }

        Err(AppError::Internal("Whisper worker did not produce a response".into()))
    }

    /// Shut down the Python worker.
    pub fn shutdown(&self) {
        let mut guard = self.child.lock();
        if let Ok(mut guard) = guard {
            if let Some(mut child) = guard.take() {
                // Close stdin to signal the worker to exit
                drop(child.stdin.take());
                let _ = child.wait();
                tracing::info!("Python whisper worker shut down");
            }
        }
    }
}

impl Default for WhisperWorker {
    fn default() -> Self {
        Self::new()
    }
}
