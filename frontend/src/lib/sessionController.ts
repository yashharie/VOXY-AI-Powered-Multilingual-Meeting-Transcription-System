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
let _mediaStream: MediaStream | null = null;
let _interimId: string | null = null;
let _initialized = false;

/**
 * Detect the dominant language of a text string using Unicode script ranges.
 *
 * Rules (in priority order):
 *   1. Count Tamil script chars (U+0B80–U+0BFF).
 *   2. Count Sinhala script chars (U+0D80–U+0DFF).
 *   3. Count ASCII Latin chars (a-z / A-Z).
 *   4. The language with the most characters wins.
 *   5. Tie-break: Tamil > Sinhala > English.
 *   6. If no recognised characters, return 'und'.
 *
 * This correctly handles code-switched text such as
 * "Yes tauri use karannē ඇyī?" (Sinhala-dominant) or
 * "Performance-ஐப் பார்த்தால் நல்லது" (Tamil-dominant).
 */
export function detectLanguage(text: string): 'ta' | 'si' | 'en' | 'und' {
  let ta = 0, si = 0, en = 0;
  for (const ch of text) {
    const cp = ch.codePointAt(0) ?? 0;
    if (cp >= 0x0B80 && cp <= 0x0BFF) ta++;
    else if (cp >= 0x0D80 && cp <= 0x0DFF) si++;
    else if ((cp >= 0x41 && cp <= 0x5A) || (cp >= 0x61 && cp <= 0x7A)) en++;
  }
  if (ta === 0 && si === 0 && en === 0) return 'und';
  // Code-switching recognition:
  // If text contains ANY native Tamil script (even 1 character in a code-switched sentence),
  // identify as Tamil to preserve Tamil font rendering and proper tags.
  // Similarly for Sinhala.
  if (ta > 0 && ta >= si) return 'ta';
  if (si > 0 && si > ta) return 'si';
  return en > 0 ? 'en' : 'und';
}

export async function translateSegment(
  text: string,
  targetLang: string,
): Promise<{ text: string; language: 'ta' | 'si' | 'en' | 'und'; detectedLang: 'ta' | 'si' | 'en' | 'und' }> {
  const localDetected = detectLanguage(text);
  if (!targetLang || targetLang === 'auto' || targetLang === localDetected) {
    return {
      text,
      // `language` reflects the SOURCE language so the font and label
      // in the transcript are always correct regardless of translation.
      language: localDetected,
      detectedLang: localDetected,
    };
  }

  try {
    const url = `https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl=${encodeURIComponent(targetLang)}&dt=t&q=${encodeURIComponent(text)}`;
    const res = await fetch(url);
    if (!res.ok) throw new Error('Translation request failed');
    const data = await res.json();
    const translated =
      data[0]
        ?.map((chunk: any) => chunk[0])
        .filter(Boolean)
        .join('')
        ?.trim() || text;

    // Prefer the Unicode-based local detection over the API's language code
    // because the API sometimes returns 'en' for code-switched Sinhala/Tamil
    // text that contains Latin words.
    let apiDetected = data[2] || localDetected;
    if (typeof apiDetected === 'string') {
      if (apiDetected.startsWith('ta')) apiDetected = 'ta';
      else if (apiDetected.startsWith('si')) apiDetected = 'si';
      else if (apiDetected.startsWith('en')) apiDetected = 'en';
      else apiDetected = localDetected;
    } else {
      apiDetected = localDetected;
    }

    // Reconcile: if our character-count detection is unambiguous (ta/si),
    // trust it over the Google API (which sometimes mis-labels Sinhala as 'zh').
    const resolvedDetected: 'ta' | 'si' | 'en' | 'und' =
      (localDetected === 'ta' || localDetected === 'si')
        ? localDetected
        : (apiDetected as 'ta' | 'si' | 'en' | 'und');

    return {
      text: translated,
      // Keep the SOURCE (detected) language on the segment, not the target.
      // The translated text is stored but the original language tag is preserved
      // so the transcript font, label, and language breakdown remain accurate.
      language: resolvedDetected,
      detectedLang: resolvedDetected,
    };
  } catch (err) {
    console.warn('Translation fallback to original:', err);
    return {
      text,
      language: localDetected,
      detectedLang: localDetected,
    };
  }
}

/**
 * Map spoken/target language settings to the best BCP-47 locale for
 * the Web SpeechRecognition API.
 *
 * The Web SpeechRecognition API accepts only ONE locale per session, so we
 * cannot do true auto-detection.  Instead we pick the best single locale that
 * maximises the chance of capturing native-script output:
 *
 *  - Explicit spoken language → use that locale directly.
 *  - Auto-detect + explicit target language → use the target locale so the
 *    recogniser outputs native Unicode script instead of romanised phonemes.
 *  - Auto-detect + target is 'auto' (fully automatic mode) → start THREE
 *    recognisers in parallel (en-US, ta-LK, si-LK) and pick the result whose
 *    detected script matches the recognised text, discarding the other two.
 *    This is the only way to handle a speaker who can switch among all three
 *    languages mid-session.
 */
function _pickRecognitionLocale(spoken: string, target: string): string {
  if (spoken === 'ta' || spoken === 'ta-IN') return 'ta-IN';
  if (spoken === 'ta-LK') return 'ta-LK';
  if (spoken === 'si') return 'si-LK';
  if (spoken === 'en') return 'en-US';
  // spoken === 'auto'
  if (target === 'ta') return 'ta-IN';
  if (target === 'si') return 'si-LK';
  // target === 'en' or 'auto'
  return 'en-US';
}

/**
 * When both spoken and target are 'auto' the user expects full three-language
 * detection (English / Tamil / Sinhala).  We run a recogniser per locale and
 * route each final result through detectLanguage() to pick the winning script.
 */
function _startMultilingualRecognition(SpeechRec: any): void {
  // Use optimal BCP-47 regional locales for English, Tamil (India), and Sinhala (Sri Lanka)
  const LOCALES: Array<{ locale: string; lang: 'en' | 'ta' | 'si' }> = [
    { locale: 'en-US', lang: 'en' },
    { locale: 'ta-IN', lang: 'ta' },
    { locale: 'si-LK', lang: 'si' },
  ];

  const recognisers: any[] = [];

  for (const { locale, lang: localeLang } of LOCALES) {
    const rec = new SpeechRec();
    rec.continuous = true;
    rec.interimResults = true;
    rec.maxAlternatives = 3;
    rec.lang = locale;

    rec.onresult = async (event: any) => {
      if (get(sessionStatus) !== 'active') return;

      let interimText = '';
      for (let i = event.resultIndex; i < event.results.length; ++i) {
          const item = event.results[i];
          // Candidate alternative selection:
          let rawText = item[0]?.transcript?.trim();
          if (item.length > 1) {
            for (let a = 0; a < item.length; a++) {
              const alt = item[a]?.transcript?.trim();
              if (alt) {
                const detected = detectLanguage(alt);
                if (detected === 'ta' || detected === 'si') {
                  rawText = alt;
                  break;
                }
              }
            }
          }
          if (!rawText) continue;

          if (item.isFinal) {
            const detectedLang = detectLanguage(rawText);

            // Cross-locale routing & disambiguation:
            // 1. If this is the Tamil or Sinhala recognizer, but the output has NO Indic script
            //    (pure English letters), discard it and let the en-US recognizer handle it.
            //    This stops non-English recognizers from polluting with mangled phonemes.
            if ((localeLang === 'ta' || localeLang === 'si') && detectedLang === 'en') {
              continue;
            }

            // 2. If the detected script belongs to a different non-English language, discard it.
            if ((detectedLang === 'ta' || detectedLang === 'si') && detectedLang !== localeLang) {
              continue;
            }

          const tgt = get(targetLanguage);
          const { text, language, detectedLang: finalDetected } = await translateSegment(rawText, tgt);
          const startTime = get(sessionStartTime) ?? Date.now();
          const elapsedSec = (Date.now() - startTime) / 1000;

          let speakerLabel = 'Speaker';
          if (finalDetected !== 'und' && tgt !== 'auto' && finalDetected !== tgt) {
            speakerLabel = `Speaker (${finalDetected.toUpperCase()} → ${tgt.toUpperCase()})`;
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

      // Show interim only from the locale matching the detected script.
      if (interimText.trim()) {
        const detectedInterim = detectLanguage(interimText);
        if (detectedInterim === localeLang || detectedInterim === 'und') {
          if (!_interimId) _interimId = `interim-${Date.now()}`;
          const startTime = get(sessionStartTime) ?? Date.now();
          const elapsedSec = (Date.now() - startTime) / 1000;
          upsertSegment({
            id: _interimId,
            text: interimText.trim() + ' …',
            start_time: Math.max(0, elapsedSec - 1),
            end_time: elapsedSec,
            speaker: 'Live Speaker',
            language: localeLang,
          });
        }
      }
    };

    rec.onerror = (e: any) => {
      console.warn(`Speech recognition error (${locale}):`, e?.error || e);
    };

    rec.onend = () => {
      if (get(sessionStatus) === 'active') {
        try { rec.start(); } catch { /* ignore */ }
      }
    };

    try { rec.start(); } catch (e) {
      console.warn(`Could not start recogniser for ${locale}:`, e);
    }
    recognisers.push(rec);
  }

  // Store all three recognisers so they can be stopped together.
  _speechRecognition = recognisers;
}

function _startSpeechRecognition(): void {
  if (typeof window === 'undefined') return;
  const SpeechRec =
    (window as any).SpeechRecognition ||
    (window as any).webkitSpeechRecognition;
  if (!SpeechRec) {
    console.warn('SpeechRecognition API not available in this WebView');
    errorMessage.set('Speech recognition is not supported on this system. Enable it in Windows Settings > Time & Language > Speech.');
    return;
  }

  (async () => {
    // Request microphone permission first — WebView2 requires this before SpeechRecognition works.
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
      stream.getTracks().forEach((t) => t.stop());
    } catch (permErr) {
      console.warn('Microphone permission denied:', permErr);
      errorMessage.set('Microphone permission denied. Allow microphone access in Windows Settings > Privacy > Microphone.');
      sessionStatus.set('idle');
      return;
    }

    // Always run all 3 languages in parallel — English, Tamil, Sinhala.
    _startMultilingualRecognition(SpeechRec);
  })();
}
function _stopSpeechRecognition(): void {
  if (_speechRecognition) {
    // In multilingual (fully-auto) mode _speechRecognition is an array of
    // three recognisers; in single-locale mode it is one object.
    const recs = Array.isArray(_speechRecognition)
      ? _speechRecognition
      : [_speechRecognition];
    for (const rec of recs) {
      try { rec.stop(); } catch { /* ignore */ }
    }
    _speechRecognition = null;
  }
  _interimId = null;
}

function _stopMediaStream(): void {
  if (_mediaStream) {
    try {
      _mediaStream.getTracks().forEach((t) => t.stop());
    } catch {}
    _mediaStream = null;
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

  // Also update devices when hardware devices change (headphones, mic, Zoom audio)
  if (typeof navigator !== 'undefined' && navigator.mediaDevices?.addEventListener) {
    navigator.mediaDevices.addEventListener('devicechange', async () => {
      const updated = await listAudioDevices();
      audioDevices.set(updated);
    });
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
    // Try to create and start a backend session — if this fails (e.g. backend
    // not fully integrated yet), fall through gracefully to speech-only mode.
    try {
      const devs = get(audioDevices);
      const chosen = devs.find((d) => d.id === _deviceId);
      let audioSource: 'Loopback' | 'Microphone' | 'Both' = 'Both';
      if (chosen?.name.toLowerCase().includes('mic')) {
        audioSource = 'Microphone';
      } else if (chosen?.name.toLowerCase().includes('speaker') || chosen?.name.toLowerCase().includes('loopback')) {
        audioSource = 'Loopback';
      }

      const sessionInfo = await initSession({
        audio_source: audioSource,
        language_hint: 'auto',
        enable_diarization: false,
        show_partial_results: true,
      });
      _sessionId = sessionInfo.session_id;
      await startSession(_sessionId);
    } catch (backendErr) {
      // Backend not ready — continue in speech-recognition-only mode
      console.warn('Backend session init failed, continuing in speech-only mode:', backendErr);
      _sessionId = null;
    }

    // Bind media stream to chosen device
    if (typeof navigator !== 'undefined' && navigator.mediaDevices?.getUserMedia) {
      try {
        const constraints: MediaStreamConstraints = {
          audio: _deviceId && _deviceId !== 'default'
            ? { deviceId: { exact: _deviceId }, echoCancellation: true, noiseSuppression: true, autoGainControl: true }
            : { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
        };
        _mediaStream = await navigator.mediaDevices.getUserMedia(constraints);
      } catch (e) {
        console.warn('Could not bind audio device stream:', e);
      }
    }

    clearSession();
    resetMockSegments();

    if (_sessionId) {
      _unlistenTranscript = await onTranscriptSegment(async (segment) => {
        appendSegment(segment);
      });
    }

    sessionStatus.set('active');
    sessionStartTime.set(Date.now());

    // Always start multilingual speech recognition (en + ta + si in parallel)
    _startSpeechRecognition();

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
    _stopMediaStream();
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
    _stopMediaStream();
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
