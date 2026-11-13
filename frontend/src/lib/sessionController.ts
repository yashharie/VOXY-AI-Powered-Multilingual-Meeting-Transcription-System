/**
 * Session controller — persistent application-level session lifecycle manager.
 *
 * Lives at the App root level and is never destroyed by navigation.
 * All session state mutations happen here; components only read from
 * the stores and call the exported functions.
 */

import {
  sessionStatus,
  selectedDeviceId,
  errorMessage,
  sessionStartTime,
  segments,
  appendSegment,
  clearSession,
} from './sessionStore';
import {
  startSession,
  pauseSession,
  resumeSession,
  stopSession,
  listAudioDevices,
  onTranscriptSegment,
  onSessionStatusChange,
} from './tauri';
import { audioDevices } from './sessionStore';
import { nextMockSegment, resetMockSegments } from './mockData';
import type { SessionStatus } from '../types';
import { get } from 'svelte/store';

// ── Internal state (not reactive stores — lives here only) ────────────────────

let _unlistenTranscript: (() => void) | null = null;
let _unlistenStatus: (() => void) | null = null;
let _mockTimer: ReturnType<typeof setInterval> | null = null;
let _initialized = false;

// ── Initialise once at app startup ────────────────────────────────────────────

export async function initSessionController(): Promise<void> {
  if (_initialized) return;
  _initialized = true;

  // Load audio devices
  const devices = await listAudioDevices();
  audioDevices.set(devices);
  if (devices.length > 0) {
    const current = get(selectedDeviceId);
    // Only set default if no device is already selected or current is unknown
    const known = devices.find((d) => d.id === current);
    if (!known) {
      selectedDeviceId.set(
        devices.find((d) => d.is_default)?.id ?? devices[0].id,
      );
    }
  }

  // Listen for backend session status events
  _unlistenStatus = await onSessionStatusChange((status: SessionStatus) => {
    sessionStatus.set(status);
  });
}

// ── Session commands ──────────────────────────────────────────────────────────

export async function handleStart(deviceId: string): Promise<void> {
  errorMessage.set(null);

  // Snapshot current segments in case start fails — restore them
  const previousSegments = get(segments);

  // Clear only after confirming we can proceed
  sessionStatus.set('starting');

  try {
    await startSession(deviceId);

    // Start succeeded — now clear previous session data
    clearSession();
    resetMockSegments();

    _unlistenTranscript = await onTranscriptSegment((segment) => {
      appendSegment(segment);
    });

    sessionStatus.set('active');
    sessionStartTime.set(Date.now());

    // Mock feed for browser development (no Tauri shell)
    if (!('__TAURI__' in window)) {
      _mockTimer = setInterval(() => {
        if (get(sessionStatus) === 'active') {
          appendSegment(nextMockSegment());
        }
      }, 2500);
    }
  } catch (err) {
    // Restore previous transcript — start failed, nothing was cleared yet
    errorMessage.set(String(err));
    sessionStatus.set('idle');
    // Segments were not cleared (clearSession was not called before failure)
    segments.set(previousSegments);
  }
}

export async function handlePause(): Promise<void> {
  try {
    await pauseSession();
    sessionStatus.set('paused');
    _stopMockTimer();
  } catch (err) {
    errorMessage.set(String(err));
    // Status remains active — backend did not confirm pause
  }
}

export async function handleResume(): Promise<void> {
  try {
    await resumeSession();
    sessionStatus.set('active');

    if (!('__TAURI__' in window)) {
      _mockTimer = setInterval(() => {
        if (get(sessionStatus) === 'active') {
          appendSegment(nextMockSegment());
        }
      }, 2500);
    }
  } catch (err) {
    errorMessage.set(String(err));
    // Status remains paused — backend did not confirm resume
  }
}

export async function handleStop(): Promise<void> {
  // Record the pre-stop status so we can restore it if stop fails
  const previousStatus = get(sessionStatus);
  sessionStatus.set('stopping');

  try {
    await stopSession();
    _stopMockTimer();
    _unlistenTranscript?.();
    _unlistenTranscript = null;
    sessionStatus.set('stopped');
    sessionStartTime.set(null);
  } catch (err) {
    errorMessage.set(String(err));
    // Restore previous status — stop failed, session may still be running
    sessionStatus.set(previousStatus);
  }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

function _stopMockTimer(): void {
  if (_mockTimer) {
    clearInterval(_mockTimer);
    _mockTimer = null;
  }
}
