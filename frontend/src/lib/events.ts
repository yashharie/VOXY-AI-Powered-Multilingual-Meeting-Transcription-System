/**
 * VOXY — Tauri event subscription helpers.
 * Wraps listen() calls so components don't need to import Tauri directly.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AudioLevelPayload,
  ErrorEventPayload,
  SessionStateChangedPayload,
  TranscriptSegmentPayload,
} from "./types";

// Event channel names — must match the Rust backend constants in
// src-tauri/src/core/events.rs
export const EVENTS = {
  SESSION_STATE_CHANGED: "voxy://session-state-changed",
  TRANSCRIPT_SEGMENT: "voxy://transcript-segment",
  ERROR: "voxy://error",
  AUDIO_LEVEL: "voxy://audio-level",
  SESSION_CLEANUP_DONE: "voxy://session-cleanup-done",
} as const;

export type VoxyEventName = (typeof EVENTS)[keyof typeof EVENTS];

/** Listen for session state changes. */
export function onSessionStateChanged(
  handler: (payload: SessionStateChangedPayload) => void
): Promise<UnlistenFn> {
  return listen<SessionStateChangedPayload>(
    EVENTS.SESSION_STATE_CHANGED,
    (event) => handler(event.payload)
  );
}

/** Listen for incoming transcript segments (partial or final). */
export function onTranscriptSegment(
  handler: (payload: TranscriptSegmentPayload) => void
): Promise<UnlistenFn> {
  return listen<TranscriptSegmentPayload>(
    EVENTS.TRANSCRIPT_SEGMENT,
    (event) => handler(event.payload)
  );
}

/** Listen for backend error events. */
export function onError(
  handler: (payload: ErrorEventPayload) => void
): Promise<UnlistenFn> {
  return listen<ErrorEventPayload>(EVENTS.ERROR, (event) =>
    handler(event.payload)
  );
}

/** Listen for audio level updates (used for the VU meter). */
export function onAudioLevel(
  handler: (payload: AudioLevelPayload) => void
): Promise<UnlistenFn> {
  return listen<AudioLevelPayload>(EVENTS.AUDIO_LEVEL, (event) =>
    handler(event.payload)
  );
}

/** Listen for session cleanup completion (safe to dismiss or reset UI). */
export function onSessionCleanupDone(
  handler: (sessionId: string) => void
): Promise<UnlistenFn> {
  return listen<{ session_id: string }>(EVENTS.SESSION_CLEANUP_DONE, (event) =>
    handler(event.payload.session_id)
  );
}
