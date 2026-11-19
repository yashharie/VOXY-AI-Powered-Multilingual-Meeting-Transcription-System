//! Persistent local translation process backed by CTranslate2 + M2M100.
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

struct WorkerProcess {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Drop for WorkerProcess {
    fn drop(&mut self) {
        let _ = self._child.kill();
        let _ = self._child.wait();
    }
}

#[derive(Default)]
pub struct TranslationWorker {
    process: Mutex<Option<WorkerProcess>>,
}

impl TranslationWorker {
    pub fn new() -> Self {
        Self {
            process: Mutex::new(None),
        }
    }

    pub fn translate(&self, text: &str, source: &str, target: &str) -> Result<String, String> {
        if text.trim().is_empty() {
            return Ok(String::new());
        }
        if !matches!(target, "en" | "ta" | "si") {
            return Err(format!("Unsupported translation target: {target}"));
        }

        // Keep "mul" intact: the Python worker splits a code-switched segment
        // by Unicode script and translates each span independently. Collapsing
        // it to the dominant script here would skip translation whenever that
        // dominant script already matches the selected target language.
        let source = resolve_source_language(source, text);
        if source == target && source != "mul" {
            return Ok(text.to_string());
        }

        let mut guard = self
            .process
            .lock()
            .map_err(|_| "Translation worker lock poisoned".to_string())?;
        if guard.is_none() {
            *guard = Some(start_worker()?);
        }
        let process = guard.as_mut().expect("worker was initialised");
        let request = serde_json::json!({
            "text": text,
            "source": source,
            "target": target
        });

        writeln!(process.stdin, "{}", request)
            .map_err(|e| format!("Could not send text to translation worker: {e}"))?;
        process
            .stdin
            .flush()
            .map_err(|e| format!("Could not flush translation request: {e}"))?;

        let mut line = String::new();
        process
            .stdout
            .read_line(&mut line)
            .map_err(|e| format!("Could not read translation result: {e}"))?;
        if line.trim().is_empty() {
            return Err("Translation worker exited without returning a result".to_string());
        }

        let response: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| format!("Invalid response from translation worker: {e}"))?;
        if let Some(error) = response.get("error").and_then(|v| v.as_str()) {
            return Err(error.to_string());
        }

        response
            .get("translation")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| "Translation worker returned no translated text".to_string())
    }
}

fn start_worker() -> Result<WorkerProcess, String> {
    let script = find_worker_script().ok_or_else(|| {
        "VOXY translation_worker.py was not found. Run VOXY from the project folder.".to_string()
    })?;

    let mut commands = Vec::new();
    if let Ok(python) = std::env::var("VOXY_PYTHON") {
        commands.push((python, Vec::<String>::new()));
    }
    commands.push(("python".to_string(), Vec::new()));
    commands.push(("py".to_string(), vec!["-3".to_string()]));

    let mut last_error = String::new();
    for (program, args) in commands {
        let mut command = Command::new(&program);
        command
            .args(&args)
            .arg("-u")
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        match command.spawn() {
            Ok(mut child) => {
                let stdin = child
                    .stdin
                    .take()
                    .ok_or("Could not open translation worker stdin")?;
                let stdout = child
                    .stdout
                    .take()
                    .ok_or("Could not open translation worker stdout")?;
                tracing::info!(
                    python = %program,
                    script = %script.display(),
                    "Started persistent translation worker"
                );
                return Ok(WorkerProcess {
                    _child: child,
                    stdin,
                    stdout: BufReader::new(stdout),
                });
            }
            Err(error) => last_error = format!("{program}: {error}"),
        }
    }

    Err(format!(
        "Python 3 is required for local translation ({last_error}). Install Python 3.10+ or set VOXY_PYTHON."
    ))
}

fn find_worker_script() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri").join("translation_worker.py"));
        candidates.push(cwd.join("translation_worker.py"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("translation_worker.py"));
            candidates.push(dir.join("resources").join("translation_worker.py"));
            candidates.push(dir.join("..").join("resources").join("translation_worker.py"));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn resolve_source_language(source: &str, text: &str) -> String {
    if matches!(source, "en" | "ta" | "si" | "mul") {
        source.to_string()
    } else {
        detect_script_language(text).to_string()
    }
}

fn detect_script_language(text: &str) -> &'static str {
    let mut ta = 0usize;
    let mut si = 0usize;
    let mut en = 0usize;

    for ch in text.chars() {
        let code = ch as u32;
        if (0x0B80..=0x0BFF).contains(&code) {
            ta += 1;
        } else if (0x0D80..=0x0DFF).contains(&code) {
            si += 1;
        } else if ch.is_ascii_alphabetic() {
            en += 1;
        }
    }

    if ta >= si && ta >= en && ta > 0 {
        "ta"
    } else if si >= en && si > 0 {
        "si"
    } else {
        "en"
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_source_language;

    #[test]
    fn preserves_mixed_language_marker_for_script_aware_translation() {
        assert_eq!(resolve_source_language("mul", "வணக்கம் hello"), "mul");
    }

    #[test]
    fn keeps_known_single_language_tags() {
        assert_eq!(resolve_source_language("en", "Hello"), "en");
        assert_eq!(resolve_source_language("ta", "வணக்கம்"), "ta");
        assert_eq!(resolve_source_language("si", "ආයුබෝවන්"), "si");
    }

    #[test]
    fn detects_unknown_language_from_text_script() {
        assert_eq!(resolve_source_language("und", "வணக்கம்"), "ta");
        assert_eq!(resolve_source_language("und", "ආයුබෝවන්"), "si");
        assert_eq!(resolve_source_language("und", "Hello"), "en");
    }
}
