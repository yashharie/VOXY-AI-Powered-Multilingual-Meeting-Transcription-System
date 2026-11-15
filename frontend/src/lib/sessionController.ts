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
let _speechRecognition: any = null;
let _initialized = false;

function _startSpeechRecognition(): void {
  if (typeof window === 'undefined') return;
  const SpeechRec =
    (window as any).SpeechRecognition ||
    (window as any).webkitSpeechRecognition;
  if (!SpeechRec) {
    return;
  }

  try {
    const recognition = new SpeechRec();
    recognition.continuous = true;
    recognition.interimResults = true;
    recognition.lang = 'en-US';

    recognition.onresult = (event: any) => {
      if (get(sessionStatus) !== 'active') return;
      for (let i = event.resultIndex; i < event.results.length; ++i) {
        const item = event.results[i];
        const text = item[0]?.transcript?.trim();
        if (!text) continue;

        if (item.isFinal) {
          const startTime = get(sessionStartTime) ?? Date.now();
          const elapsedSec = (Date.now() - startTime) / 1000;
          appendSegment({
            id: `speech-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
            text,
            start_time: Math.max(0, elapsedSec - 2.5),
            end_time: elapsedSec,
            speaker: 'Live Speaker',
            language: 'en',
          });
        }
      }
    };

    recognition.onerror = (e: any) => {
      console.warn('Live speech recognition warning:', e?.error || e);
    };

    recognition.onend = () => {
      if (get(sessionStatus) === 'active') {
        try {
          recognition.start();
        } catch {}
      }
    };

    recognition.start();
    _speechRecognition = recognition;
  } catch (err) {
    console.warn('SpeechRecognition could not start:', err);
  }
}

function _stopSpeechRecognition(): void {
  if (_speechRecognition) {
    try {
      _speechRecognition.stop();
    } catch {}
    _speechRecognition = null;
  }
}

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

    // Start live speech transcription
    _startSpeechRecognition();

    // Only run mock feed if explicitly requested via URL parameter (?mock=true) for browser UI testing
    const isExplicitMock = typeof window !== 'undefined' && window.location.search.includes('mock=true');
    if (!isTauri() && isExplicitMock) {
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
    _stopSpeechRecognition();
  } catch (err) {
    errorMessage.set(String(err));
  }
}

export async function handleResume(): Promise<void> {
  if (!_sessionId) return;
  try {
    await resumeSession(_sessionId);
    sessionStatus.set('active');
    _startSpeechRecognition();
    const isExplicitMock = typeof window !== 'undefined' && window.location.search.includes('mock=true');
    if (!isTauri() && isExplicitMock) {
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
    _stopSpeechRecognition();
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
