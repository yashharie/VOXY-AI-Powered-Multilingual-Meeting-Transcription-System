/**
 * Session controller — persistent application-level session lifecycle manager.
 *
 * Lives at the App root level and is never destroyed by navigation.
 * Follows the real Tauri IPC contract:
 *   init_session(config) → session_id → start_session(session_id)
 */

import {
  sessionStatus,
  selectedDeviceId,
  errorMessage,
  sessionStartTime,
  segments,
  appendSegment,
  clearSession,
  audioDevices,
} from './sessionStore';
import {
  initSession,
  startSession,
  pauseSession,
  resumeSession,
  stopSession,
  listAudioDevices,
  onTranscriptSegment,
  onSessionStatusChange,
  isTauri,
} from './tauri';
import { nextMockSegment, resetMockSegments } from './mockData';
import type { SessionStatus } from '../types';
import { get } from 'svelte/store';

// ── Internal state ────────────────────────────────────────────────────────────

let _sessionId: string | null = null;
let _unlistenTranscript: (() => void) | null = null;
let _unlistenStatus: (() => void) | null = null;
let _mockTimer: ReturnType<typeof setInterval> | null = null;
let _initialized = false;

// ── Init once at app startup ──────────────────────────────────────────────────

export async function initSessionController(): Promise<void> {
  if (_initialized) return;
  _initialized = true;

  const devices = await listAudioDevices();
  audioDevices.set(devices);
  if (devices.length > 0) {
    const current = get(selectedDeviceId);
    const known = devices.find((d) => d.id === current);
    if (!known) {
      selectedDeviceId.set(
        devices.find((d) => d.is_default)?.id ?? devices[0].id,
      );
    }
  }

  _unlistenStatus = await onSessionStatusChange((status: SessionStatus) => {
    sessionStatus.set(status);
  });
}

// ── Session commands ──────────────────────────────────────────────────────────

export async function handleStart(_deviceId: string): Promise<void> {
  errorMessage.set(null);
  const previousSegments = get(segments);
  sessionStatus.set('starting');

  try {
    // Step 1: Create the session on the backend
    const sessionInfo = await initSession({
      audio_source: 'Loopback',
      language_hint: 'auto',
      enable_diarization: false,
      show_partial_results: true,
    });

    _sessionId = sessionInfo.session_id;

    // Step 2: Start the session
    await startSession(_sessionId);

    clearSession();
    resetMockSegments();

    _unlistenTranscript = await onTranscriptSegment((segment) => {
      appendSegment(segment);
    });

    sessionStatus.set('active');
    sessionStartTime.set(Date.now());

    // Mock feed when not in Tauri (browser dev) or when backend has no audio yet
    if (!isTauri()) {
      _mockTimer = setInterval(() => {
        if (get(sessionStatus) === 'active') {
          appendSegment(nextMockSegment());
        }
      }, 2500);
    }
  } catch (err) {
    errorMessage.set(String(err));
    sessionStatus.set('idle');
    segments.set(previousSegments);
    _sessionId = null;
  }
}

export async function handlePause(): Promise<void> {
  if (!_sessionId) return;
  try {
    await pauseSession(_sessionId);
    sessionStatus.set('paused');
    _stopMockTimer();
  } catch (err) {
    errorMessage.set(String(err));
  }
}

export async function handleResume(): Promise<void> {
  if (!_sessionId) return;
  try {
    await resumeSession(_sessionId);
    sessionStatus.set('active');
    if (!isTauri()) {
      _mockTimer = setInterval(() => {
        if (get(sessionStatus) === 'active') {
          appendSegment(nextMockSegment());
        }
      }, 2500);
    }
  } catch (err) {
    errorMessage.set(String(err));
  }
}

export async function handleStop(): Promise<void> {
  if (!_sessionId) return;
  const previousStatus = get(sessionStatus);
  sessionStatus.set('stopping');

  try {
    await stopSession(_sessionId);
    _stopMockTimer();
    _unlistenTranscript?.();
    _unlistenTranscript = null;
    sessionStatus.set('stopped');
    sessionStartTime.set(null);
    _sessionId = null;
  } catch (err) {
    errorMessage.set(String(err));
    sessionStatus.set(previousStatus);
  }
}

function _stopMockTimer(): void {
  if (_mockTimer) {
    clearInterval(_mockTimer);
    _mockTimer = null;
  }
}
