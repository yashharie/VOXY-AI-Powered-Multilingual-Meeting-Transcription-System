/**
 * VOXY — Shared TypeScript types mirroring Rust backend structures.
 * These types must stay in sync with src-tauri/src/core/types.rs.
 */

// ─── Session ────────────────────────────────────────────────────────────────

/** All possible states a transcription session can be in. */
export type SessionState =
  | "Idle"
  | "Initializing"
  | "Active"
  | "Paused"
  | "Stopping"
  | "Stopped"
  | "Error";

/** Returned when a session is created or queried. */
export interface SessionInfo {
  /** Unique UUID for the session. */
  session_id: string;
  /** Current lifecycle state. */
  state: SessionState;
  /** Unix timestamp (seconds) when the session was created. */
  created_at: number;
  /** Unix timestamp (seconds) when the session became Active. null if not yet started. */
  started_at: number | null;
  /** Unix timestamp (seconds) when the session ended. null if still ongoing. */
  ended_at: number | null;
  /** Human-readable display name, auto-generated from creation time. */
  display_name: string;
}

/** Configuration supplied by the user/app when starting a session. */
export interface SessionConfig {
  /** Audio source: system loopback, microphone, or both. */
  audio_source: "Loopback" | "Microphone" | "Both";
  /** Preferred language hint for Whisper (e.g. "si", "ta", "en", "auto"). */
  language_hint: string;
  /** Whether to attempt speaker diarization. */
  enable_diarization: boolean;
  /** Whether to show partial/provisional transcript segments. */
  show_partial_results: boolean;
}

// ─── Transcript ─────────────────────────────────────────────────────────────

/** A single finalized transcript segment emitted by the backend. */
export interface TranscriptSegment {
  /** Auto-incrementing index within the session. */
  index: number;
  /** Recognized text. */
  text: string;
  /** Session-relative start time in seconds. */
  start_time: number;
  /** Session-relative end time in seconds. */
  end_time: number;
  /** Detected language code. */
  language: string;
  /** Generic speaker label, e.g. "Speaker 1". null when diarization is off. */
  speaker: string | null;
  /** True when this is a provisional update; false when finalized. */
  is_partial: boolean;
  /** Wall-clock timestamp (ISO 8601) when the segment was produced. */
  timestamp: string;
}

// ─── Events ─────────────────────────────────────────────────────────────────

/** Payload for the `voxy://session-state-changed` event. */
export interface SessionStateChangedPayload {
  session_id: string;
  previous_state: SessionState;
  new_state: SessionState;
  reason: string | null;
}

/** Payload for the `voxy://transcript-segment` event. */
export interface TranscriptSegmentPayload {
  session_id: string;
  segment: TranscriptSegment;
}

/** Payload for the `voxy://error` event. */
export interface ErrorEventPayload {
  session_id: string | null;
  code: string;
  message: string;
  recoverable: boolean;
}

/** Payload for the `voxy://audio-level` event. */
export interface AudioLevelPayload {
  session_id: string;
  /** RMS level in the range 0.0 – 1.0. */
  level: number;
}

// ─── IPC command results ─────────────────────────────────────────────────────

/** Generic success/error wrapper returned by IPC commands. */
export type VoxyResult<T> =
  | { Ok: T }
  | { Err: { code: string; message: string } };

/** Returned by get_app_version command. */
export interface AppVersion {
  version: string;
  tauri_version: string;
  build_target: string;
}
