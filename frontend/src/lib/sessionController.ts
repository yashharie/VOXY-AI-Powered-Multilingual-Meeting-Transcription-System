/**
 * VOXY Session Controller
 *
 * This is the single source of truth for session lifecycle management.
 *
 * Live transcription path (inside Tauri desktop app):
 *   initSession() → startSession() → Rust pipeline starts:
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
  selectedDeviceId,
  errorMessage,
  sessionStartTime,
  segments,
  appendSegment,
  upsertSegment,
  clearSession,
  audioDevices,
  targetLanguage,
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
  if (devices.length > 0) {
    const current = get(selectedDeviceId);
    const known = devices.find((d) => d.id === current);
    if (!known) {
      selectedDeviceId.set(
        devices.find((d) => d.is_default)?.id ?? devices[0].id,
      );
    }
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
  });
}

// ── Session commands ──────────────────────────────────────────────────────────

export async function handleStart(deviceId: string): Promise<void> {
  errorMessage.set(null);
  const previousSegments = get(segments);
  sessionStatus.set('starting');

  // ── Tauri desktop path ────────────────────────────────────────────────────
  if (isTauri()) {
    try {
      // 1. Create the session on the Rust backend
      const sessionInfo = await initSession({
        audio_source: 'Both',          // loopback + microphone
        language_hint: 'auto',         // Whisper auto-detects Tamil/Sinhala/English
        enable_diarization: false,
        show_partial_results: true,
        loopback_device_id: deviceId !== 'default' ? deviceId : undefined,
      });
      _sessionId = sessionInfo.session_id;

      // 2. Start the native pipeline — WASAPI capture → Whisper
      await startSession(_sessionId);

      // 3. Subscribe to transcript segments from the native pipeline
      _unlistenTranscript = await onTranscriptSegment((segment: TranscriptSegment) => {
        const tgt = get(targetLanguage);
        if (tgt && tgt !== 'auto' && segment.language !== tgt) {
          // If a specific output language is selected, still show the segment
          // in its original language — translation is not done automatically.
          // The segment is shown as-is; the language tag reflects the detected script.
        }
        if (segment.is_partial) {
          // Update or insert partial segment in-place
          upsertSegment(segment);
        } else {
          // Remove any matching partial before appending the final
          segments.update(all => {
            const withoutPartial = all.filter(s => !(s.is_partial && s.id === segment.id));
            return [...withoutPartial, segment];
          });
        }
      });

      // 4. Subscribe to backend error events
      _unlistenError = await onSessionError(({ code, message }) => {
        errorMessage.set(`[${code}] ${message}`);
      });

      clearSession();
      resetMockSegments();
      sessionStatus.set('active');
      sessionStartTime.set(Date.now());

      return; // Native path done — do not fall through to mock
    } catch (err) {
      // Native pipeline failed — show the error clearly, do not silently fall back
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
      await stopSession(_sessionId);
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
