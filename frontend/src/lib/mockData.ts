/**
 * Mock transcript data for frontend development.
 *
 * Provides sample transcript segments covering:
 *   - English-only speech
 *   - Sinhala-only speech
 *   - Tamil-only speech
 *   - Sinhala-English code-switching
 *   - Tamil-English code-switching
 *   - Sinhala-Tamil-English mixed conversation
 *   - Multiple speakers taking turns
 *
 * This data is used exclusively during browser-based development and
 * UI demonstration before the Tauri backend and AI modules are integrated.
 * It does not represent real transcription output.
 */

import type { TranscriptSegment } from '../types';

export const MOCK_SEGMENTS: TranscriptSegment[] = [
  // ── English-only ─────────────────────────────────────────────────────────
  {
    id: 1,
    text: 'Good morning everyone. Let us get started with today\'s meeting.',
    startMs: 0,
    endMs: 3800,
    speaker: 'Speaker 1',
    language: 'en', status: 'final',
  },
  {
    id: 2,
    text: 'The agenda for today covers the project timeline and the technical prototype.',
    startMs: 4000,
    endMs: 7600,
    speaker: 'Speaker 1',
    language: 'en', status: 'final',
  },

  // ── Sinhala-only ─────────────────────────────────────────────────────────
  {
    id: 3,
    text: 'ආයුබෝවන්. අද අපි නව ව්‍යාපෘතිය ගැන කතා කරමු.',
    startMs: 8000,
    endMs: 11200,
    speaker: 'Speaker 2',
    language: 'si', status: 'final',
  },
  {
    id: 4,
    text: 'මම මේ ගැන ටිකක් කලින් ඉඳලා සොයා බැලුවා. ප්‍රශ්නයක් නෑ.',
    startMs: 11500,
    endMs: 15000,
    speaker: 'Speaker 2',
    language: 'si', status: 'final',
  },

  // ── Tamil-only ───────────────────────────────────────────────────────────
  {
    id: 5,
    text: 'வணக்கம். இன்றைய கூட்டத்தில் நாம் திட்டத்தை விவாதிப்போம்.',
    startMs: 15500,
    endMs: 19100,
    speaker: 'Speaker 3',
    language: 'ta', status: 'final',
  },
  {
    id: 6,
    text: 'தொழில்நுட்ப அம்சங்களை இன்று விளக்குவேன்.',
    startMs: 19400,
    endMs: 22300,
    speaker: 'Speaker 3',
    language: 'ta', status: 'final',
  },

  // ── Sinhala-English code-switching ──────────────────────────────────────
  {
    id: 7,
    text: 'Yes, the system architecture looks good. අපි Tauri use කරන්නේ ඇයි?',
    startMs: 23000,
    endMs: 26600,
    speaker: 'Speaker 1',
    language: 'en', status: 'final',
  },
  {
    id: 8,
    text: 'Tauri ගොඩක් lightweight. It uses the system WebView instead of bundling Chromium.',
    startMs: 27000,
    endMs: 31400,
    speaker: 'Speaker 2',
    language: 'si', status: 'final',
  },
  {
    id: 9,
    text: 'Memory footprint එකත් Electron වලට වඩා හොඳයි. Performance tests run කරන්න ඕනේ.',
    startMs: 31800,
    endMs: 36200,
    speaker: 'Speaker 2',
    language: 'si', status: 'final',
  },

  // ── Tamil-English code-switching ─────────────────────────────────────────
  {
    id: 10,
    text: 'சரி. Performance-ஐப் பார்த்தால் இது நல்லது.',
    startMs: 37000,
    endMs: 40100,
    speaker: 'Speaker 3',
    language: 'ta', status: 'final',
  },
  {
    id: 11,
    text: 'Audio capture module-ஐ WASAPI மூலம் implement பண்ணலாம்.',
    startMs: 40500,
    endMs: 44300,
    speaker: 'Speaker 3',
    language: 'ta', status: 'final',
  },

  // ── Sinhala-Tamil-English mixed ───────────────────────────────────────────
  {
    id: 12,
    text: 'We also need to evaluate Whisper Base and Small on CPU-only hardware.',
    startMs: 45000,
    endMs: 49300,
    speaker: 'Speaker 1',
    language: 'en', status: 'final',
  },
  {
    id: 13,
    text: 'Whisper Large-v3 accuracy reference ලෙස use කළත්, 8 GB RAM machine එකේ slow වෙන්න පුළුවන්.',
    startMs: 49800,
    endMs: 54200,
    speaker: 'Speaker 2',
    language: 'si', status: 'final',
  },
  {
    id: 14,
    text: 'சிங்கள மொழிக்கான fine-tuned model பற்றி ஆராய வேண்டும். இது முக்கியம்.',
    startMs: 54600,
    endMs: 58900,
    speaker: 'Speaker 3',
    language: 'ta', status: 'final',
  },

  // ── Multiple speakers taking turns ───────────────────────────────────────
  {
    id: 15,
    text: 'What is the testing plan for the diarization module?',
    startMs: 59500,
    endMs: 62400,
    speaker: 'Speaker 1',
    language: 'en', status: 'final',
  },
  {
    id: 16,
    text: 'WER සහ CER measure කරනවා. DER diarization එකට.',
    startMs: 62800,
    endMs: 66100,
    speaker: 'Speaker 2',
    language: 'si', status: 'final',
  },
  {
    id: 17,
    text: 'Real-time factor-ஐயும் latency-ஐயும் அளவிட வேண்டும்.',
    startMs: 66500,
    endMs: 70000,
    speaker: 'Speaker 3',
    language: 'ta', status: 'final',
  },
  {
    id: 18,
    text: 'Agreed. We will document all results in the evaluation report.',
    startMs: 70400,
    endMs: 73800,
    speaker: 'Speaker 1',
    language: 'en', status: 'final',
  },
];

// ── Stateful mock feed ────────────────────────────────────────────────────────

let _segmentIndex = 0;

/**
 * Returns the next mock segment in sequence.
 * Loops back to the beginning once all segments have been delivered.
 * Each loop assigns fresh IDs and shifted timestamps so the UI
 * treats them as new segments.
 */
export function nextMockSegment(): TranscriptSegment {
  const loopCount = Math.floor(_segmentIndex / MOCK_SEGMENTS.length);
  const segment   = MOCK_SEGMENTS[_segmentIndex % MOCK_SEGMENTS.length];
  _segmentIndex++;

  const offset = loopCount * 80_000; // shift by ~80 s per loop
  return {
    ...segment,
    id: _segmentIndex,
    startMs: segment.startMs + offset,
    endMs: segment.endMs + offset,
  };
}

/** Reset the mock feed to the beginning (called on session start). */
export function resetMockSegments(): void {
  _segmentIndex = 0;
}
