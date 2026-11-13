import { listen } from '@tauri-apps/api/event';
import type { TranscriptUpdate, TranscriptDeletion, SessionStateChangedPayload, ErrorEventPayload, AudioLevelPayload } from './types';
export const EVENTS = {
  SESSION_STATE_CHANGED: 'voxy://session-state-changed', TRANSCRIPT_SEGMENT: 'transcript:update',
  TRANSCRIPT_DELETED: 'transcript:deleted', ERROR: 'voxy://error', AUDIO_LEVEL: 'voxy://audio-level', SESSION_CLEANUP_DONE: 'voxy://session-cleanup-done',
} as const;
export type VoxyEventName = typeof EVENTS[keyof typeof EVENTS];
export const onSessionStateChanged = (handler: (payload: SessionStateChangedPayload) => void) => listen<SessionStateChangedPayload>(EVENTS.SESSION_STATE_CHANGED, e => handler(e.payload));
export const onTranscriptSegment = (handler: (payload: TranscriptUpdate) => void) => listen<TranscriptUpdate>(EVENTS.TRANSCRIPT_SEGMENT, e => handler(e.payload));
export const onTranscriptDeleted = (handler: (payload: TranscriptDeletion) => void) => listen<TranscriptDeletion>(EVENTS.TRANSCRIPT_DELETED, e => handler(e.payload));
export const onError = (handler: (payload: ErrorEventPayload) => void) => listen<ErrorEventPayload>(EVENTS.ERROR, e => handler(e.payload));
export const onAudioLevel = (handler: (payload: AudioLevelPayload) => void) => listen<AudioLevelPayload>(EVENTS.AUDIO_LEVEL, e => handler(e.payload));
export const onSessionCleanupDone = (handler: (id: string) => void) => listen<{session_id: string}>(EVENTS.SESSION_CLEANUP_DONE, e => handler(e.payload.session_id));