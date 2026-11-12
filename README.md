 **VOXY — AI Live Meeting Transcription System**

**Team:** Tesseract  
**Team size:** 9 developers  
**Project period:** 09 November 2026 – 16 November 2026  

---

## 1. Project Overview

VOXY is a Windows desktop application designed to capture live meeting audio from a user's computer and convert speech into text in near real time.

The application works independently alongside existing meeting platforms such as:

- Zoom
- Google Meet
- Microsoft Teams

VOXY does not host meetings or manage meeting participants. Its initial audio-capture approach is Windows WASAPI loopback, which captures audio produced by the computer. Optional microphone capture may also be supported.

The system prioritizes local processing, open-source technologies, privacy-conscious audio handling, and efficient CPU/RAM usage.

## 2. Main Objectives

- Capture system audio on Windows using WASAPI.
- Support optional microphone input where feasible.
- Transcribe Sinhala, Tamil, and English.
- Evaluate Sinhala-English and Tamil-English code-switching.
- Display transcript segments while a meeting is in progress.
- Associate timestamps with transcript segments.
- Investigate generic speaker labels through speaker diarization.
- Allow users to review and edit transcript text.
- Allow users to save or export transcripts.
- Clear temporary audio buffers and release resources when a session ends.
- Evaluate accuracy, latency, CPU usage, RAM usage, and long-session stability.

## 3. Project Scope

### Included in the proposed baseline

- Windows desktop application.
- Tauri 2 application shell.
- Svelte and TypeScript user interface.
- Rust application core.
- WASAPI system-audio capture.
- Optional microphone capture, subject to feasibility.
- PCM audio processing and resampling with Rubato.
- Whisper multilingual speech recognition through whisper.cpp and whisper-rs.
- Evaluation of Silero VAD and sherpa-onnx for VAD/diarization functionality.
- Transcript segment processing, timestamps, and generic speaker labels where supported.
- Live transcript display.
- Transcript review/editing and user-requested local saving/export.
- Session stop, buffer cleanup, and resource release.

### Not part of the initial baseline

- Hosting or managing meetings.
- A browser extension or a web-based meeting side panel.
- Replacing Zoom, Google Meet, or Microsoft Teams.
- Paid per-minute transcription APIs as the default processing method.
- Permanent archiving of raw meeting audio.
- A mandatory remote server for standalone operation.
- Mandatory SQLite or PostgreSQL databases.
- Confirmed Linux or macOS support.

### Conditional or future capabilities

The technical plan lists Axum, Tokio, WebSocket, SQLite, and PostgreSQL for conditional use cases. They are not mandatory components of the standalone local transcription pipeline:

- **Axum + Tokio + WebSocket:** optional shared-office server mode.
- **SQLite:** only if structured local transcript history/search is required.
- **PostgreSQL:** only if centralized shared-office storage is approved.

Do not add these to the baseline implementation without an agreed requirement and review.

## 4. Agreed Technology Stack

Use the technologies below as the project's technical baseline. Do not independently replace them with another framework, runtime, language, or service.

| Area | Technology | Responsibility |
|---|---|---|
| Desktop framework | Tauri 2 | Native desktop application |
| Frontend framework | Svelte | User interface |
| Frontend tooling | Vite | Frontend development/build |
| Frontend language | TypeScript | Frontend logic |
| Styling | CSS | Interface styling |
| Backend language | Rust | Native operations and processing |
| Async runtime | Tokio | Async operations where required |
| Optional server framework | Axum | Optional shared-server mode only |
| Local UI/backend communication | Tauri IPC and Events | Frontend ↔ Rust communication |
| Optional remote communication | WebSocket | Shared-server mode only |
| Windows audio capture | WASAPI | System audio capture |
| Rust WASAPI integration | `wasapi` crate | Rust access to WASAPI |
| Audio representation | PCM | Internal audio data |
| Audio resampling | Rubato | Sample-rate conversion |
| Primary speech model | Whisper multilingual | Speech recognition |
| Local Whisper runtime | whisper.cpp | Local inference |
| Rust Whisper integration | whisper-rs | Rust integration with whisper.cpp |
| Voice activity detection | Silero VAD | Speech/silence detection |
| VAD/diarization candidate | sherpa-onnx | Offline VAD/diarization integration candidate |
| AI inference runtime | ONNX Runtime | ONNX model execution |
| Serialization | Serde + JSON | Configuration and structured data |
| Logging | Tracing | Application diagnostics |
| Frontend unit tests | Vitest | Frontend logic testing |
| UI workflow tests | Playwright | UI testing |
| Rust tests | `cargo test` | Rust unit/integration testing |
| Optional API tests | Postman | Only if an API is introduced |
| Version control | Git | Source control |
| Code hosting | GitHub | Repository and pull requests |
| UI/UX design | Figma | Interface design |
| Desktop packaging | Tauri Bundler | Application packaging |
| Development environment | Visual Studio Code | Development |
| Frontend runtime | Node.js + npm | Frontend tooling |
| Rust toolchain | Rust + Cargo | Rust build/dependency management |
| Performance evaluation | Windows Performance Monitor + custom Rust benchmarks | CPU/RAM/performance measurement |

### AI model selection is not yet final

The technical plan proposes evaluating:

- Whisper Base — CPU and memory efficiency baseline.
- Whisper Small — accuracy/resource comparison.
- Whisper Large-v3 — higher-capability accuracy reference where hardware permits.
- AI4Bharat IndicConformer — additional Tamil benchmark only; not the primary multilingual model and does not provide Sinhala coverage.
- Sinhala-specific fine-tuned Whisper candidates — research and validation required; no Sinhala specialist model is finalized.

The final model, quantization, VAD configuration, and diarization configuration must be selected using measured results. Do not describe an untested candidate as a finalized production choice.

## 5. System Architecture

The standalone local processing flow is:

```text
Tauri 2 Desktop Application
|
+-- Svelte + TypeScript Frontend
|      |
|      +-- Tauri IPC commands
|      +-- Tauri Events (live status and transcript updates)
|
+-- Rust Application Core
       |
       +-- Session and Configuration Management
       |
       +-- WASAPI Audio Capture
       |      +-- System Audio (Loopback)
       |      +-- Optional Microphone
       |
       +-- Audio Preprocessing
       |      +-- PCM
       |      +-- Rubato Resampling
       |      +-- Normalization / Buffer Management
       |
       +-- Voice Activity Detection
       |      +-- Silero VAD
       |      +-- sherpa-onnx integration candidate
       |
       +-- AI Processing
       |      +-- Whisper / whisper.cpp / whisper-rs
       |      +-- sherpa-onnx speaker diarization candidate
       |
       +-- Transcript Processing
       |      +-- Timestamp association
       |      +-- Speaker association
       |      +-- Segment organization
       |
       +-- Transcript Management
              +-- Review / Edit
              +-- User-requested local save / export
              +-- Temporary audio cleanup
```

Tauri IPC and Tauri Events are the local frontend/backend communication mechanisms. WebSocket is not a replacement for local Tauri communication; it is reserved for an optional remote/shared-server mode.

## 6. Nine-Developer Allocation

Each developer owns a separate module directory. This is the main rule for avoiding merge conflicts. Do not edit another member's owned module unless the owner and team coordinator agree first.

### Member 1 — Frontend and User Interface

**Branch:** `feature/frontend-ui`  
**Owned directory:** `frontend/`

**Responsibilities**
- Set up the Svelte + TypeScript + Vite frontend.
- Build the main desktop interface.
- Build the meeting session screen and live transcript panel.
- Add start, pause/resume if supported, and stop controls.
- Display session status, timestamps, and speaker labels.
- Build transcript review/editing and export-related UI states.
- Use mock transcript data until backend events are available.
- Connect the UI to the agreed Tauri IPC command/event contract.

**Must not**
- Implement audio capture or AI inference.
- Modify Rust backend modules.
- Change the agreed frontend framework.

**Deliverable:** A working Svelte interface that can be demonstrated with mock data and then connected through the agreed Tauri contract.

### Member 2 — Tauri Application Core and Session Management

**Branch:** `feature/tauri-app-core`  
**Owned directory:** `src-tauri/src/core/` and root Tauri configuration files

**Responsibilities**
- Maintain the Tauri 2 application shell and Rust application entry points.
- Implement session lifecycle and unique session identifiers.
- Implement configuration loading with Serde and JSON.
- Coordinate initialization and shutdown of the modules.
- Define and maintain the Tauri IPC command and event registration points.
- Own shared Rust dependency/configuration files such as `Cargo.toml`, `tauri.conf.json`, and the Rust application entry point.
- Integrate approved module interfaces without rewriting another member's module.

**Must not**
- Implement the internal WASAPI, preprocessing, or AI algorithms inside the core module.
- Change the agreed stack without review.

**Deliverable:** A Tauri desktop application core that starts, configures, coordinates, and safely ends a transcription session.

### Member 3 — Windows Audio Capture

**Branch:** `feature/wasapi-audio-capture`  
**Owned directory:** `src-tauri/src/audio_capture/`

**Responsibilities**
- Investigate and implement Windows WASAPI loopback capture using the Rust `wasapi` crate.
- Detect and select available audio devices.
- Capture system output audio from supported meeting applications.
- Investigate optional microphone capture.
- Handle device errors, disconnections, sample-rate information, and capture interruptions.
- Provide captured audio to the agreed audio-input interface.
- Create a mock audio source for tests if practical.

**Must not**
- Implement resampling or AI inference.
- Modify the application entry point or shared dependency files directly; request dependency changes from Member 2.

**Deliverable:** An independently testable WASAPI capture module that provides audio frames in the agreed format.

### Member 4 — Audio Preprocessing

**Branch:** `feature/audio-preprocessing`  
**Owned directory:** `src-tauri/src/audio_processing/`

**Responsibilities**
- Receive audio frames through the agreed capture interface.
- Convert captured audio into the agreed PCM representation.
- Implement resampling with Rubato where required.
- Implement appropriate audio-level normalization.
- Manage bounded audio buffers and audio chunk boundaries.
- Prepare consistent audio input for VAD, speech recognition, and diarization.
- Add unit tests using supplied/sample PCM data.

**Must not**
- Access WASAPI devices directly.
- Implement Whisper or diarization.
- Introduce another audio-processing framework without approval.

**Deliverable:** A reusable preprocessing module that accepts audio frames and produces consistent PCM audio for downstream modules.

### Member 5 — AI Speech Recognition

**Branch:** `feature/whisper-speech-recognition`  
**Owned directory:** `src-tauri/src/speech_recognition/`

**Responsibilities**
- Integrate Whisper through whisper.cpp and whisper-rs.
- Evaluate Whisper Base and Small on the target CPU environment.
- Evaluate Large-v3 only where hardware permits.
- Investigate Sinhala-specific fine-tuned Whisper candidates.
- Record Tamil, Sinhala, English, and code-switching test results.
- Return recognized text and available timing/language metadata through the agreed interface.
- Measure inference latency and memory usage.

**Must not**
- Implement audio capture or VAD.
- Replace whisper.cpp/whisper-rs with a different ASR runtime without review.
- Claim model accuracy before evaluation.

**Deliverable:** An independently testable Rust speech-recognition module with documented model evaluation results.

### Member 6 — VAD and Speaker Diarization

**Branch:** `feature/vad-diarization`  
**Owned directory:** `src-tauri/src/speaker_analysis/`

**Responsibilities**
- Investigate Silero VAD and its ONNX Runtime/sherpa-onnx integration.
- Investigate sherpa-onnx offline speaker diarization.
- Detect speech and non-speech regions.
- Produce generic labels such as Speaker 1 and Speaker 2 where supported.
- Return speaker intervals/timestamps through the agreed interface.
- Evaluate speakers taking turns, interruptions, and overlapping speech.
- Measure the additional CPU and RAM cost of running alongside Whisper.

**Must not**
- Implement speech-to-text.
- Claim that diarization separates overlapping voices; diarization and speech separation are different tasks.
- Add SepFormer or Conv-TasNet to the baseline. The technical plan lists them only as research candidates, not baseline components.

**Deliverable:** A separately testable VAD/diarization module and a feasibility report describing model compatibility and measured limitations.

### Member 7 — Transcript Processing and Live Event Payloads

**Branch:** `feature/transcript-processing`  
**Owned directory:** `src-tauri/src/transcript_processing/`

**Responsibilities**
- Define structured transcript segments using Rust types and Serde.
- Combine recognized text with timestamps and speaker intervals.
- Associate speaker labels with transcript time ranges.
- Handle provisional/partial updates and finalized segments where supported.
- Organize segments in chronological order.
- Prevent duplicate or stale transcript updates.
- Prepare the agreed payload for the Tauri event layer.
- Add tests for timestamp alignment, segment updates, and speaker association.

**Must not**
- Implement the frontend UI.
- Implement the AI model or audio capture.
- Register Tauri commands/events in the shared application entry point; provide payload types and event requirements to Member 2.

**Deliverable:** A tested transcript-processing module and documented transcript event payload.

### Member 8 — Transcript Review, Save, Export, and Cleanup

**Branch:** `feature/transcript-management`  
**Owned directory:** `src-tauri/src/transcript_management/`

**Responsibilities**
- Implement transcript review/edit support on the Rust side where required.
- Implement user-requested local transcript saving and file handling.
- Evaluate and implement the agreed export formats (TXT first; PDF/DOCX only if confirmed and feasible).
- Ensure exported content reflects the user's latest edits.
- Coordinate session completion and temporary-data cleanup with Member 2.
- Ensure raw meeting audio is not permanently archived.
- Do not add SQLite unless structured history/search is explicitly approved.

**Must not**
- Add a database by default.
- Change transcript-processing data structures without agreement with Member 7.
- Modify the frontend UI owned by Member 1.

**Deliverable:** A tested transcript-management module for review/edit, explicit local save/export, and safe cleanup.

### Member 9 — Quality Assurance and Performance Evaluation

**Branch:** `test/qa-performance`  
**Owned directories:** `tests/` and `docs/testing/`

**Responsibilities**
- Prepare test plans and test cases for each module.
- Create integration tests using the agreed interfaces and mock inputs.
- Test Sinhala, Tamil, English, and code-switched samples.
- Evaluate WER and CER for speech recognition.
- Evaluate DER and speaker attribution separately for diarization.
- Measure first-result latency, processing latency, CPU usage, RAM usage, model-loading time, and long-session stability.
- Test system audio capture with Zoom, Google Meet, and Microsoft Teams separately.
- Test failure cases: unavailable devices, missing models, interruptions, and session shutdown.
- Report defects with reproduction steps and evidence.
- Review pull requests for test coverage; do not rewrite module implementations.

**Must not**
- Own or modify production modules.
- Change the selected technology stack.
- Treat unverified accuracy or latency as an acceptance guarantee.

**Deliverable:** Test suite, test report, language evaluation results, and CPU/RAM/latency benchmark report.


**Ownership rule:** One owner per directory. A developer may create files only inside their owned directory unless a shared-file change has been agreed and reviewed.

## 7. Git and GitHub Workflow

### 7.1 Branches

Use one branch per member/workstream:

| Member | Branch |
|---|---|
| 1 | `feature/frontend-ui` |
| 2 | `feature/tauri-app-core` |
| 3 | `feature/wasapi-audio-capture` |
| 4 | `feature/audio-preprocessing` |
| 5 | `feature/whisper-speech-recognition` |
| 6 | `feature/vad-diarization` |
| 7 | `feature/transcript-processing` |
| 8 | `feature/transcript-management` |
| 9 | `test/qa-performance` |

### 7.2 Rules

1. Protect `main`; do not commit directly to it.
2. Start each task from the latest agreed base branch.
3. Work only in your owned module directory.
4. Keep commits small and focused.
5. Push your branch regularly.
6. Open a pull request for every completed task.
7. Request review from the relevant module owner and Member 2 for integration-sensitive changes.
8. Do not merge a PR that breaks the agreed build or tests.
9. Do not change shared dependency/configuration files without Member 2's review.
10. Do not force-push to another member's branch.
11. Pull the latest base branch before starting a new task.
12. Resolve conflicts with the owner of the affected files; do not overwrite their work.


---
**VOXY --- Every voice matters.**
