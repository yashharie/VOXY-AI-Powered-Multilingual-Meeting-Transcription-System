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

export function detectLanguage(text: string): 'ta' | 'si' | 'en' | 'und' {
  if (/[\u0B80-\u0BFF]/.test(text)) return 'ta';
  if (/[\u0D80-\u0DFF]/.test(text)) return 'si';
  if (/[a-zA-Z]/.test(text)) return 'en';
  return 'und';
}

export async function translateSegment(
  text: string,
  targetLang: string,
): Promise<{ text: string; language: 'ta' | 'si' | 'en' | 'und'; detectedLang: 'ta' | 'si' | 'en' | 'und' }> {
  const localDetected = detectLanguage(text);
  if (!targetLang || targetLang === 'auto' || targetLang === localDetected) {
    return {
      text,
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

    let apiDetected = data[2] || localDetected;
    if (typeof apiDetected === 'string') {
      if (apiDetected.startsWith('en')) apiDetected = 'en';
      else if (apiDetected.startsWith('ta')) apiDetected = 'ta';
      else if (apiDetected.startsWith('si')) apiDetected = 'si';
      else apiDetected = localDetected;
    } else {
      apiDetected = localDetected;
    }

    return {
      text: translated,
      language: (targetLang as 'ta' | 'si' | 'en') || 'und',
      detectedLang: apiDetected as 'ta' | 'si' | 'en' | 'und',
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

    // Set speech recognition language based on explicit spoken language setting.
    // When "Auto-Detect" is selected, fall back to target language so the
    // recognizer outputs native script (Tamil/Sinhala Unicode) rather than
    // romanised English phonemes (enna nadakuthu → என்ன நடக்கிறது).
    const spoken = get(spokenLanguage);
    const target = get(targetLanguage);

    if (spoken === 'ta') {
      recognition.lang = 'ta-LK';
    } else if (spoken === 'si') {
      recognition.lang = 'si-LK';
    } else if (spoken === 'en') {
      recognition.lang = 'en-US';
    } else {
      // spoken === 'auto' — mirror the target language so native script is captured
      if (target === 'ta') {
        recognition.lang = 'ta-LK';
      } else if (target === 'si') {
        recognition.lang = 'si-LK';
      } else {
        recognition.lang = 'en-US';
      }
    }

    recognition.onresult = async (event: any) => {
      if (get(sessionStatus) !== 'active') return;

      let interimText = '';

      for (let i = event.resultIndex; i < event.results.length; ++i) {
        const item = event.results[i];
        const rawText = item[0]?.transcript?.trim();
        if (!rawText) continue;

        if (item.isFinal) {
          // Sentence finalized
          const target = get(targetLanguage);
          const { text, language, detectedLang } = await translateSegment(rawText, target);
          const startTime = get(sessionStartTime) ?? Date.now();
          const elapsedSec = (Date.now() - startTime) / 1000;

          let speakerLabel = 'Speaker';
          if (detectedLang !== 'und' && target !== 'auto' && detectedLang !== target) {
            speakerLabel = `Speaker (${detectedLang.toUpperCase()} → ${target.toUpperCase()})`;
          }

          if (_interimId) {
            // Replace the interim segment with the finalized translated segment
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

      // Real-time instant display for words being spoken right now (0s delay!)
      if (interimText.trim()) {
        if (!_interimId) {
          _interimId = `interim-${Date.now()}`;
        }
        const startTime = get(sessionStartTime) ?? Date.now();
        const elapsedSec = (Date.now() - startTime) / 1000;
        upsertSegment({
          id: _interimId,
          text: interimText.trim() + ' …',
          start_time: Math.max(0, elapsedSec - 1),
          end_time: elapsedSec,
          speaker: 'Live Speaker',
          language: get(targetLanguage) === 'auto' ? 'en' : (get(targetLanguage) as any),
        });
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
    // Determine audio source based on selected device name
    const devs = get(audioDevices);
    const chosen = devs.find((d) => d.id === _deviceId);
    let audioSource: 'Loopback' | 'Microphone' | 'Both' = 'Both';
    if (chosen?.name.toLowerCase().includes('mic')) {
      audioSource = 'Microphone';
    } else if (chosen?.name.toLowerCase().includes('speaker') || chosen?.name.toLowerCase().includes('loopback')) {
      audioSource = 'Loopback';
    }

    // Step 1: Create the session on the backend
    const sessionInfo = await initSession({
      audio_source: audioSource,
      language_hint: get(spokenLanguage) || 'auto',
      enable_diarization: false,
      show_partial_results: true,
    });

    _sessionId = sessionInfo.session_id;

    // Step 2: Start the session
    await startSession(_sessionId);

    // Bind media capture stream to chosen device
    if (typeof navigator !== 'undefined' && navigator.mediaDevices?.getUserMedia) {
      try {
        const constraints: MediaStreamConstraints = {
          audio: _deviceId && _deviceId !== 'default'
            ? { deviceId: { exact: _deviceId } }
            : true,
        };
        _mediaStream = await navigator.mediaDevices.getUserMedia(constraints);
      } catch (e) {
        console.warn('Could not bind chosen audio device stream directly:', e);
      }
    }

    clearSession();
    resetMockSegments();

    _unlistenTranscript = await onTranscriptSegment(async (segment) => {
      const target = get(targetLanguage);
      if (target && target !== 'auto') {
        const { text, language, detectedLang } = await translateSegment(segment.text, target);
        let speakerLabel = segment.speaker || 'Speaker';
        if (detectedLang !== 'und' && detectedLang !== target) {
          speakerLabel = `${speakerLabel} (${detectedLang.toUpperCase()} → ${target.toUpperCase()})`;
        }
        appendSegment({
          ...segment,
          text,
          language,
          speaker: speakerLabel,
        });
      } else {
        appendSegment(segment);
      }
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
