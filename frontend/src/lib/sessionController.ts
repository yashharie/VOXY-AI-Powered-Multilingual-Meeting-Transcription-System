/**
 * Session controller — persistent application-level session lifecycle manager.
 *
 * Lives at the App root level and is never destroyed by navigation.
 * Follows the real Tauri IPC contract:
 *   init_session(config) → session_id → start_session(session_id)
 */

import {
  sessionStatus,
  errorMessage,
  sessionStartTime,
  segments,
  appendSegment,
  upsertSegment,
  clearSession,
  audioDevices,
  targetLanguage,
  spokenLanguage,
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
} from './tauri';
import { resetMockSegments } from './mockData';
import type { SessionStatus } from '../types';
import { get } from 'svelte/store';

// ── Internal state ────────────────────────────────────────────────────────────

let _sessionId: string | null = null;
let _unlistenTranscript: (() => void) | null = null;
let _unlistenStatus: (() => void) | null = null;
let _speechRecognition: any = null;   // single SpeechRecognition instance
let _mediaStream: MediaStream | null = null;
let _interimId: string | null = null;
let _initialized = false;
let _micPermissionGranted = false;    // cached so we don't double-prompt

// ── Language detection ────────────────────────────────────────────────────────

export function detectLanguage(text: string): 'ta' | 'si' | 'en' | 'und' {
  let ta = 0, si = 0, en = 0;
  for (const ch of text) {
    const cp = ch.codePointAt(0) ?? 0;
    if (cp >= 0x0B80 && cp <= 0x0BFF) ta++;
    else if (cp >= 0x0D80 && cp <= 0x0DFF) si++;
    else if ((cp >= 0x41 && cp <= 0x5A) || (cp >= 0x61 && cp <= 0x7A)) en++;
  }
  if (ta === 0 && si === 0 && en === 0) return 'und';
  if (ta > 0 && ta >= si) return 'ta';
  if (si > 0 && si > ta) return 'si';
  return en > 0 ? 'en' : 'und';
}

// ── Translation ───────────────────────────────────────────────────────────────

export async function translateSegment(
  text: string,
  targetLang: string,
): Promise<{ text: string; language: 'ta' | 'si' | 'en' | 'und'; detectedLang: 'ta' | 'si' | 'en' | 'und' }> {
  const localDetected = detectLanguage(text);
  if (!targetLang || targetLang === 'auto' || targetLang === localDetected) {
    return { text, language: localDetected, detectedLang: localDetected };
  }

  try {
    const url = `https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl=${encodeURIComponent(targetLang)}&dt=t&q=${encodeURIComponent(text)}`;
    const res = await fetch(url);
    if (!res.ok) throw new Error('Translation request failed');
    const data = await res.json();
    const translated =
      data[0]?.map((chunk: any) => chunk[0]).filter(Boolean).join('')?.trim() || text;

    let apiDetected = data[2] || localDetected;
    if (typeof apiDetected === 'string') {
      if (apiDetected.startsWith('ta')) apiDetected = 'ta';
      else if (apiDetected.startsWith('si')) apiDetected = 'si';
      else if (apiDetected.startsWith('en')) apiDetected = 'en';
      else apiDetected = localDetected;
    } else {
      apiDetected = localDetected;
    }

    const resolvedDetected: 'ta' | 'si' | 'en' | 'und' =
      (localDetected === 'ta' || localDetected === 'si') ? localDetected : (apiDetected as any);

    return { text: translated, language: resolvedDetected, detectedLang: resolvedDetected };
  } catch (err) {
    console.warn('Translation fallback to original:', err);
    return { text, language: localDetected, detectedLang: localDetected };
  }
}

// ── Microphone permission ─────────────────────────────────────────────────────

async function _requestMicPermission(): Promise<boolean> {
  if (_micPermissionGranted) return true;
  if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) return false;
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
    stream.getTracks().forEach((t) => t.stop());
    _micPermissionGranted = true;
    return true;
  } catch (e) {
    console.warn('Microphone permission denied:', e);
    return false;
  }
}

// ── Speech recognition (single instance, one language at a time) ──────────────

function _pickLocale(spoken: string, target: string): string {
  if (spoken === 'ta') return 'ta-IN';
  if (spoken === 'si') return 'si-LK';
  if (spoken === 'en') return 'en-US';
  // spoken === 'auto' — mirror target so native script is captured
  if (target === 'ta') return 'ta-IN';
  if (target === 'si') return 'si-LK';
  return 'en-US';
}

function _startSpeechRecognition(): void {
  if (typeof window === 'undefined') return;

  const SpeechRec =
    (window as any).SpeechRecognition ||
    (window as any).webkitSpeechRecognition;

  if (!SpeechRec) {
    errorMessage.set(
      'Speech recognition is not available. Enable it in Windows Settings → Time & Language → Speech.',
    );
    return;
  }

  // Fire-and-forget async wrapper — errors are caught internally, never bubble up
  void (async () => {
    // Ensure mic permission before starting (required by WebView2)
    const hasPermission = await _requestMicPermission();
    if (!hasPermission) {
      errorMessage.set(
        'Microphone permission denied. Go to Windows Settings → Privacy → Microphone and allow access.',
      );
      sessionStatus.set('idle');
      return;
    }

    // Guard: session may have been stopped while we awaited permission
    if (get(sessionStatus) !== 'active') return;

    try {
      const recognition = new SpeechRec();
      recognition.continuous    = true;
      recognition.interimResults = true;
      recognition.maxAlternatives = 3;   // try top-3 alternatives for better accuracy
      recognition.lang = _pickLocale(get(spokenLanguage), get(targetLanguage));

      // Visual feedback: show when audio is actually being captured
      recognition.onsoundstart = () => {
        if (get(sessionStatus) === 'active') errorMessage.set(null);
      };
      recognition.onspeechend = () => {
        // speech ended — recognition will finalize and onend will restart
      };

      recognition.onresult = async (event: any) => {
        if (get(sessionStatus) !== 'active') return;

        let interimText = '';

        for (let i = event.resultIndex; i < event.results.length; ++i) {
          const item    = event.results[i];
          // Pick the best alternative: prefer any that contains native script (ta/si)
          let rawText = item[0]?.transcript?.trim();
          for (let a = 1; a < item.length; a++) {
            const alt = item[a]?.transcript?.trim();
            if (alt && detectLanguage(alt) !== 'en' && detectLanguage(rawText || '') === 'en') {
              rawText = alt; // prefer native-script alternative over romanised
            }
          }
          if (!rawText) continue;

          if (item.isFinal) {
            const tgt = get(targetLanguage);
            const { text, language, detectedLang } = await translateSegment(rawText, tgt);
            const startTime  = get(sessionStartTime) ?? Date.now();
            const elapsedSec = (Date.now() - startTime) / 1000;

            let speakerLabel = 'Speaker';
            if (detectedLang !== 'und' && tgt !== 'auto' && detectedLang !== tgt) {
              speakerLabel = `Speaker (${detectedLang.toUpperCase()} → ${tgt.toUpperCase()})`;
            }

            if (_interimId) {
              upsertSegment({
                id: _interimId,
                text,
                start_time: Math.max(0, elapsedSec - 2.5),
                end_time: elapsedSec,
                speaker: speakerLabel,
                language,
              });
              _interimId = null;
            } else {
              appendSegment({
                id: `speech-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
                text,
                start_time: Math.max(0, elapsedSec - 2.5),
                end_time: elapsedSec,
                speaker: speakerLabel,
                language,
              });
            }
          } else {
            interimText += rawText + ' ';
          }
        }

        // Real-time interim display
        if (interimText.trim()) {
          if (!_interimId) _interimId = `interim-${Date.now()}`;
          const startTime  = get(sessionStartTime) ?? Date.now();
          const elapsedSec = (Date.now() - startTime) / 1000;
          const tgt = get(targetLanguage);
          upsertSegment({
            id: _interimId,
            text: interimText.trim() + ' …',
            start_time: Math.max(0, elapsedSec - 1),
            end_time: elapsedSec,
            speaker: 'Live Speaker',
            language: tgt === 'auto' ? 'en' : (tgt as any),
          });
        }
      };

      recognition.onerror = (e: any) => {
        const err = e?.error || '';
        // 'no-speech' is normal (silence), 'aborted' happens on stop — ignore both
        if (err === 'no-speech' || err === 'aborted') return;

        // Show actionable errors to the user
        if (err === 'not-allowed' || err === 'service-not-allowed') {
          errorMessage.set('Microphone access denied. Go to Windows Settings → Privacy → Microphone and allow VOXY.');
        } else if (err === 'network') {
          errorMessage.set('Network error during transcription. Check your internet connection.');
        } else if (err === 'audio-capture') {
          errorMessage.set('Could not access microphone. Make sure no other app is using it exclusively.');
        } else {
          console.warn('Speech recognition error:', err, e);
        }
      };

      let _restartTimer: ReturnType<typeof setTimeout> | null = null;

      recognition.onend = () => {
        // Clear any pending restart timer
        if (_restartTimer) { clearTimeout(_restartTimer); _restartTimer = null; }

        // Auto-restart after a short delay to avoid tight loops and race conditions.
        // The delay also prevents WebView2 from throttling rapid restart attempts.
        if (get(sessionStatus) === 'active') {
          _restartTimer = setTimeout(() => {
            if (get(sessionStatus) === 'active' && _speechRecognition === recognition) {
              try { recognition.start(); } catch { /* recognition already restarting */ }
            }
          }, 300);
        }
      };

      recognition.start();
      _speechRecognition = recognition;
    } catch (err) {
      console.warn('Could not start SpeechRecognition:', err);
    }
  })();
}

function _stopSpeechRecognition(): void {
  if (_speechRecognition) {
    try { _speechRecognition.stop(); } catch { /* ignore */ }
    _speechRecognition = null;
  }
  _interimId = null;
}

function _stopMediaStream(): void {
  if (_mediaStream) {
    try { _mediaStream.getTracks().forEach((t) => t.stop()); } catch { /* ignore */ }
    _mediaStream = null;
  }
}

// ── Init once at app startup ──────────────────────────────────────────────────

export async function initSessionController(): Promise<void> {
  if (_initialized) return;
  _initialized = true;

  // Load audio devices (always returns at least 4 fallback entries)
  try {
    const devices = await listAudioDevices();
    audioDevices.set(devices);
  } catch (e) {
    console.warn('Could not enumerate audio devices:', e);
  }

  // Listen for backend session-state events
  try {
    _unlistenStatus = await onSessionStatusChange((status: SessionStatus) => {
      // Only update from backend if we have an active backend session
      if (_sessionId) sessionStatus.set(status);
    });
  } catch (e) {
    console.warn('Could not register session status listener:', e);
  }

  // Update device list when hardware changes (mic plugged in, Zoom virtual device, etc.)
  if (typeof navigator !== 'undefined' && navigator.mediaDevices?.addEventListener) {
    navigator.mediaDevices.addEventListener('devicechange', async () => {
      try {
        const updated = await listAudioDevices();
        audioDevices.set(updated);
      } catch { /* ignore */ }
    });
  }
}

// ── Session commands ──────────────────────────────────────────────────────────

export async function handleStart(_deviceId: string): Promise<void> {
  errorMessage.set(null);
  sessionStatus.set('starting');

  try {
    clearSession();
    resetMockSegments();

    // ── Step 1: Try to init backend session (non-fatal if it fails) ──
    try {
      const devs = get(audioDevices);
      const chosen = devs.find((d) => d.id === _deviceId);
      let audioSource: 'Loopback' | 'Microphone' | 'Both' = 'Both';
      if (chosen?.name.toLowerCase().includes('mic')) audioSource = 'Microphone';
      else if (chosen?.name.toLowerCase().includes('speaker') || chosen?.name.toLowerCase().includes('loopback'))
        audioSource = 'Loopback';

      const sessionInfo = await initSession({
        audio_source: audioSource,
        language_hint: get(spokenLanguage) || 'auto',
        enable_diarization: false,
        show_partial_results: true,
      });
      _sessionId = sessionInfo.session_id;
      await startSession(_sessionId);

      // Listen for Rust-emitted transcript segments
      _unlistenTranscript = await onTranscriptSegment(async (segment) => {
        appendSegment(segment);
      });
    } catch (backendErr) {
      // Backend init failed — that's OK, speech-only mode works fine
      console.warn('Backend session init failed, running in speech-only mode:', backendErr);
      _sessionId = null;
    }

    // ── Step 2: Get mic permission via getUserMedia, then RELEASE the stream ──
    // SpeechRecognition manages its own audio input internally.
    // Holding a getUserMedia stream open at the same time causes them to
    // compete for the microphone → degraded / broken recognition quality.
    if (typeof navigator !== 'undefined' && navigator.mediaDevices?.getUserMedia) {
      try {
        const probeStream = await navigator.mediaDevices.getUserMedia({
          audio: _deviceId && _deviceId !== 'default'
            ? { deviceId: { exact: _deviceId }, echoCancellation: true, noiseSuppression: true }
            : { echoCancellation: true, noiseSuppression: true },
        });
        // Release immediately — permission is now cached by the browser/WebView2
        probeStream.getTracks().forEach((t) => t.stop());
        _micPermissionGranted = true;
      } catch (e) {
        console.warn('Could not get microphone permission:', e);
        // Non-fatal — SpeechRecognition will try anyway and show its own error
      }
    }

    // ── Step 3: Mark active and start live transcription ──
    sessionStatus.set('active');
    sessionStartTime.set(Date.now());
    _startSpeechRecognition();

  } catch (err) {
    // Outer catch — something truly unexpected happened
    errorMessage.set(`Failed to start: ${String(err)}`);
    sessionStatus.set('idle');
    _sessionId = null;
  }
}

export async function handlePause(): Promise<void> {
  _stopSpeechRecognition();
  _stopMediaStream();

  if (_sessionId) {
    try {
      await pauseSession(_sessionId);
    } catch (e) {
      console.warn('Backend pause failed:', e);
    }
  }

  sessionStatus.set('paused');
}

export async function handleResume(): Promise<void> {
  if (_sessionId) {
    try {
      await resumeSession(_sessionId);
    } catch (e) {
      console.warn('Backend resume failed:', e);
    }
  }

  sessionStatus.set('active');
  _startSpeechRecognition();
}

export async function handleStop(): Promise<void> {
  // Stop everything locally first — this is instant and never crashes
  _stopSpeechRecognition();
  _stopMediaStream();

  _unlistenTranscript?.();
  _unlistenTranscript = null;

  sessionStatus.set('stopped');
  sessionStartTime.set(null);

  // Then tell the backend (non-fatal if it fails — session is already stopped locally)
  if (_sessionId) {
    const sid = _sessionId;
    _sessionId = null;
    try {
      await stopSession(sid);
    } catch (e) {
      console.warn('Backend stop session failed (session already cleaned up locally):', e);
    }
  }
}
