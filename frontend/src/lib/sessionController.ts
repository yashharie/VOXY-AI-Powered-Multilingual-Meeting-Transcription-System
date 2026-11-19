/**
 * VOXY Session Controller
 *
 * This is the single source of truth for session lifecycle management.
 *
 * Live transcription path (inside Tauri desktop app):
 *   initSession() → subscribe to native events → startSession() → Rust pipeline:
 *     WASAPI capture → preprocessing → VAD → Whisper → events
 *   Frontend listens on voxy://transcript-segment Tauri events.
 *
 * Dev/browser fallback (npm run dev, no Tauri shell):
 *   Mock segments are injected every 2.5 s for UI development.
 *   Browser SpeechRecognition is NOT used — it was removed because it
 *   bypassed the native pipeline and produced unreliable multilingual output.
 */

import {
  sessionStatus,
  selectedOutputDeviceId,
  selectedMicrophoneDeviceId,
  errorMessage,
  sessionStartTime,
  segments,
  appendSegment,
  upsertSegment,
  clearSession,
  audioDevices,
  selectedTargetLanguage,
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
  onSessionError,
  isTauri,
} from './tauri';
import { nextMockSegment, resetMockSegments } from './mockData';
import type { SessionStatus, TranscriptSegment } from '../types';
import { get } from 'svelte/store';

// ── Internal state ────────────────────────────────────────────────────────────

let _sessionId: string | null = null;
let _unlistenTranscript: (() => void) | null = null;
let _unlistenStatus: (() => void) | null = null;
let _unlistenError: (() => void) | null = null;
let _mockTimer: ReturnType<typeof setInterval> | null = null;
let _initialized = false;

// ── Init once at app startup ──────────────────────────────────────────────────

export async function initSessionController(): Promise<void> {
  if (_initialized) return;
  _initialized = true;

  // Load audio device list
  const devices = await listAudioDevices();
  audioDevices.set(devices);
  const outputDevices = devices.filter((device) => device.kind === 'output');
  const inputDevices = devices.filter((device) => device.kind === 'input');

  if (!outputDevices.some((device) => device.id === get(selectedOutputDeviceId))) {
    selectedOutputDeviceId.set(
      outputDevices.find((device) => device.is_default)?.id ?? 'default-output',
    );
  }
  if (!inputDevices.some((device) => device.id === get(selectedMicrophoneDeviceId))) {
    selectedMicrophoneDeviceId.set(
      inputDevices.find((device) => device.is_default)?.id ?? 'default-input',
    );
  }

  // Keep device list fresh when hardware changes
  if (typeof navigator !== 'undefined' && navigator.mediaDevices?.addEventListener) {
    navigator.mediaDevices.addEventListener('devicechange', async () => {
      audioDevices.set(await listAudioDevices());
    });
  }

  // Listen for backend session-state events
  _unlistenStatus = await onSessionStatusChange((status: SessionStatus) => {
    sessionStatus.set(status);
    if (status === 'stopped') {
      _stopMockTimer();
      _unlistenTranscript?.();
      _unlistenTranscript = null;
      _unlistenError?.();
      _unlistenError = null;
      sessionStartTime.set(null);
      _sessionId = null;
    }
  });
}

// ── Session commands ──────────────────────────────────────────────────────────

export async function handleStart(outputDeviceId: string, microphoneDeviceId: string): Promise<void> {
  errorMessage.set(null);
  const previousSegments = get(segments);
  sessionStatus.set('starting');

  // ── Tauri desktop path ────────────────────────────────────────────────────
  if (isTauri()) {
    try {
      // Translation mode: recognise all supported source languages automatically.
      const targetLanguage = get(selectedTargetLanguage);

      // Clear the previous session before starting so early native events
      // cannot be erased by a later clearSession() call.
      clearSession();

      // 1. Create the session using the language selected for spoken audio.
      const sessionInfo = await initSession({
        audio_source: 'Both',          // loopback + microphone
        language_hint: 'auto',
        allowed_languages: ['ta', 'si', 'en'],
        translation_target_language: targetLanguage,
        enable_diarization: false,
        loopback_device_id: outputDeviceId !== 'default-output' ? outputDeviceId : undefined,
        microphone_device_id: microphoneDeviceId !== 'default-input' ? microphoneDeviceId : undefined,
      });
      _sessionId = sessionInfo.session_id;

      // 2. Subscribe BEFORE starting the native pipeline. Whisper can finish
      //    its first window quickly enough that subscribing afterwards loses it.
      _unlistenTranscript = await onTranscriptSegment((segment: TranscriptSegment) => {
        // Backend emits the same stable segment ID again when translation arrives.
        // Upsert updates that row instead of creating a duplicate transcript entry.
        upsertSegment(segment);
        segments.update(all => all.sort((a, b) =>
          a.start_time - b.start_time || a.id.localeCompare(b.id)
        ));
      });

      // Subscribe to errors before start_session for the same reason.
      _unlistenError = await onSessionError(({ code, message }) => {
        errorMessage.set(`[${code}] ${message}`);
      });

      // 3. Start audio capture and transcription only after listeners are ready.
      await startSession(_sessionId);

      resetMockSegments();
      sessionStatus.set('active');
      sessionStartTime.set(Date.now());

      return;
    } catch (err) {
      // Remove any listeners installed for a session that failed to start.
      _unlistenTranscript?.();
      _unlistenTranscript = null;
      _unlistenError?.();
      _unlistenError = null;

      const msg = String(err);
      errorMessage.set(
        msg.includes('model not found') || msg.includes('main.exe')
          ? 'Whisper model or CLI not found. Place ggml-base.bin and main.exe in the models/ folder next to the app.'
          : `Failed to start session: ${msg}`
      );
      sessionStatus.set('idle');
      segments.set(previousSegments);
      _sessionId = null;
      return;
    }
  }

  // ── Browser dev path (no Tauri shell) ────────────────────────────────────
  // Used only when running `npm run dev` in a browser for UI development.
  // Injects mock transcript segments to simulate the live transcript panel.
  clearSession();
  resetMockSegments();
  sessionStatus.set('active');
  sessionStartTime.set(Date.now());
  _mockTimer = setInterval(() => {
    if (get(sessionStatus) === 'active') {
      appendSegment(nextMockSegment());
    }
  }, 2500);
}

export async function handlePause(): Promise<void> {
  if (isTauri() && !_sessionId) return;
  try {
    if (isTauri() && _sessionId) {
      await pauseSession(_sessionId);
    }
    sessionStatus.set('paused');
    _stopMockTimer();
  } catch (err) {
    errorMessage.set(String(err));
  }
}

export async function handleResume(): Promise<void> {
  if (isTauri() && !_sessionId) return;
  try {
    if (isTauri() && _sessionId) {
      await resumeSession(_sessionId);
    }
    sessionStatus.set('active');
    if (!isTauri()) {
      _mockTimer = setInterval(() => {
        if (get(sessionStatus) === 'active') appendSegment(nextMockSegment());
      }, 2500);
    }
  } catch (err) {
    errorMessage.set(String(err));
  }
}

export async function handleStop(): Promise<void> {
  const previousStatus = get(sessionStatus);
  sessionStatus.set('stopping');

  try {
    if (isTauri() && _sessionId) {
      const info = await stopSession(_sessionId);
      // Native Stop returns once capture has stopped. Keep transcript listeners
      // active until the background Whisper queue emits the final Stopped state.
      if (info.state === 'Stopping') {
        if (get(sessionStatus) !== 'stopped') sessionStatus.set('stopping');
        return;
      }
    }
    _stopMockTimer();
    _unlistenTranscript?.();
    _unlistenTranscript = null;
    _unlistenError?.();
    _unlistenError = null;
    sessionStatus.set('stopped');
    sessionStartTime.set(null);
    _sessionId = null;
  } catch (err) {
    errorMessage.set(String(err));
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
