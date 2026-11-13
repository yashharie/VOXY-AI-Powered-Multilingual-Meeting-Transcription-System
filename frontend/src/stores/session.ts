/** Compatibility facade for the earlier core view; no second state or listeners. */
import { derived, get } from 'svelte/store';
import { sessionInfo, sessionStatus, segments, busy, audioLevel, errorMessage } from '../lib/sessionStore';
import { initSessionController, destroySessionController, configureSession, handleStart, handlePause, handleResume, handleStop } from '../lib/sessionController';
import type { SessionConfig, SessionState } from '../lib/types';
const view = derived([sessionInfo, segments, busy, audioLevel, errorMessage], ([session, transcript, loading, level, error]) => ({ session, transcript, partialSegment: transcript.find(s => s.status === 'partial') ?? null, loading, audioLevel: level, error }));
export const sessionStore = {
  subscribe: view.subscribe, initialize: initSessionController, destroy: destroySessionController,
  init: async (config: SessionConfig) => { configureSession(config); },
  start: () => handleStart(), pause: handlePause, resume: handleResume, stop: handleStop,
  clearError: () => errorMessage.set(null), reset: () => errorMessage.set(null),
};
export const sessionState = derived(sessionStatus, status => ({ idle: 'Idle', starting: 'Initializing', active: 'Active', paused: 'Paused', stopping: 'Stopping', stopped: 'Stopped', error: 'Error' })[status] as SessionState);
export const isRecording = derived(sessionStatus, s => s === 'active');
export const isPaused = derived(sessionStatus, s => s === 'paused');
export const canStart = derived(sessionStatus, s => ['idle', 'stopped', 'error'].includes(s));
export const canPause = isRecording;
export const canStop = derived(sessionStatus, s => ['active', 'paused', 'error'].includes(s));