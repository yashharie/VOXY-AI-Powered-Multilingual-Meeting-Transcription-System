/**
 * VOXY — Tauri IPC command wrappers.
 * All backend communication goes through this module.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  AppVersion,
  SessionConfig,
  SessionInfo,
  TranscriptSegment,
  VoxyResult,
} from "./types";

// ─── Helpers ─────────────────────────────────────────────────────────────────

function unwrap<T>(result: VoxyResult<T>): T {
  if ("Ok" in result) return result.Ok;
  throw new Error(`[${result.Err.code}] ${result.Err.message}`);
}

// ─── Application commands ────────────────────────────────────────────────────

/** Returns the application version metadata. */
export async function getAppVersion(): Promise<AppVersion> {
  return invoke<AppVersion>("get_app_version");
}

/** Pings the backend. Returns "pong" on success. */
export async function ping(): Promise<string> {
  return invoke<string>("ping");
}

// ─── Session lifecycle commands ───────────────────────────────────────────────

/**
 * Creates a new session and returns its info.
 * The session is in Initializing state after this call.
 */
export async function initSession(config: SessionConfig): Promise<SessionInfo> {
  const result = await invoke<VoxyResult<SessionInfo>>("init_session", { config });
  return unwrap(result);
}

/**
 * Transitions the session from Initializing/Paused → Active.
 * Starts or resumes audio capture and transcription.
 */
export async function startSession(sessionId: string): Promise<SessionInfo> {
  const result = await invoke<VoxyResult<SessionInfo>>("start_session", {
    sessionId,
  });
  return unwrap(result);
}

/**
 * Transitions the session from Active → Paused.
 * Audio capture halts; transcript history is preserved.
 */
export async function pauseSession(sessionId: string): Promise<SessionInfo> {
  const result = await invoke<VoxyResult<SessionInfo>>("pause_session", {
    sessionId,
  });
  return unwrap(result);
}

/**
 * Resumes a paused session.  Equivalent to startSession after pause.
 */
export async function resumeSession(sessionId: string): Promise<SessionInfo> {
  const result = await invoke<VoxyResult<SessionInfo>>("resume_session", {
    sessionId,
  });
  return unwrap(result);
}

/**
 * Stops the session and triggers cleanup.
 * Transitions Active/Paused → Stopping → Stopped.
 */
export async function stopSession(sessionId: string): Promise<SessionInfo> {
  const result = await invoke<VoxyResult<SessionInfo>>("stop_session", {
    sessionId,
  });
  return unwrap(result);
}

/** Returns the current state of a session by ID. */
export async function getSessionInfo(sessionId: string): Promise<SessionInfo> {
  const result = await invoke<VoxyResult<SessionInfo>>("get_session_info", {
    sessionId,
  });
  return unwrap(result);
}

// ─── Transcript commands ──────────────────────────────────────────────────────

/** Returns all finalized transcript segments for a session. */
export async function getTranscript(sessionId: string): Promise<TranscriptSegment[]> {
  const result = await invoke<VoxyResult<TranscriptSegment[]>>(
    "get_transcript",
    { sessionId }
  );
  return unwrap(result);
}

/**
 * Saves the transcript to a local file chosen by the user.
 * Returns the absolute path of the saved file.
 */
export async function saveTranscript(
  sessionId: string,
  format: "txt" | "json"
): Promise<string> {
  const result = await invoke<VoxyResult<string>>("save_transcript", {
    sessionId,
    format,
  });
  return unwrap(result);
}
