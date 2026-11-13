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
    id: 'mock-1',
    text: 'Good morning everyone. Let us get started with today\'s meeting.',
    start_time: 0.0,
    end_time: 3.8,
    speaker: 'Speaker 1',
    language: 'en',
  },
  {
    id: 'mock-2',
    text: 'The agenda for today covers the project timeline and the technical prototype.',
    start_time: 4.0,
    end_time: 7.6,
    speaker: 'Speaker 1',
    language: 'en',
  },

  // ── Sinhala-only ─────────────────────────────────────────────────────────
  {
    id: 'mock-3',
    text: 'ආයුබෝවන්. අද අපි නව ව්‍යාපෘතිය ගැන කතා කරමු.',
    start_time: 8.0,
    end_time: 11.2,
    speaker: 'Speaker 2',
    language: 'si',
  },
  {
    id: 'mock-4',
    text: 'මම මේ ගැන ටිකක් කලින් ඉඳලා සොයා බැලුවා. ප්‍රශ්නයක් නෑ.',
    start_time: 11.5,
    end_time: 15.0,
    speaker: 'Speaker 2',
    language: 'si',
  },

  // ── Tamil-only ───────────────────────────────────────────────────────────
  {
    id: 'mock-5',
    text: 'வணக்கம். இன்றைய கூட்டத்தில் நாம் திட்டத்தை விவாதிப்போம்.',
    start_time: 15.5,
    end_time: 19.1,
    speaker: 'Speaker 3',
    language: 'ta',
  },
  {
    id: 'mock-6',
    text: 'தொழில்நுட்ப அம்சங்களை இன்று விளக்குவேன்.',
    start_time: 19.4,
    end_time: 22.3,
    speaker: 'Speaker 3',
    language: 'ta',
  },

  // ── Sinhala-English code-switching ──────────────────────────────────────
  {
    id: 'mock-7',
    text: 'Yes, the system architecture looks good. අපි Tauri use කරන්නේ ඇයි?',
    start_time: 23.0,
    end_time: 26.6,
    speaker: 'Speaker 1',
    language: 'en',
  },
  {
    id: 'mock-8',
    text: 'Tauri ගොඩක් lightweight. It uses the system WebView instead of bundling Chromium.',
    start_time: 27.0,
    end_time: 31.4,
    speaker: 'Speaker 2',
    language: 'si',
  },
  {
    id: 'mock-9',
    text: 'Memory footprint එකත් Electron වලට වඩා හොඳයි. Performance tests run කරන්න ඕනේ.',
    start_time: 31.8,
    end_time: 36.2,
    speaker: 'Speaker 2',
    language: 'si',
  },

  // ── Tamil-English code-switching ─────────────────────────────────────────
  {
    id: 'mock-10',
    text: 'சரி. Performance-ஐப் பார்த்தால் இது நல்லது.',
    start_time: 37.0,
    end_time: 40.1,
    speaker: 'Speaker 3',
    language: 'ta',
  },
  {
    id: 'mock-11',
    text: 'Audio capture module-ஐ WASAPI மூலம் implement பண்ணலாம்.',
    start_time: 40.5,
    end_time: 44.3,
    speaker: 'Speaker 3',
    language: 'ta',
  },

  // ── Sinhala-Tamil-English mixed ───────────────────────────────────────────
  {
    id: 'mock-12',
    text: 'We also need to evaluate Whisper Base and Small on CPU-only hardware.',
    start_time: 45.0,
    end_time: 49.3,
    speaker: 'Speaker 1',
    language: 'en',
  },
  {
    id: 'mock-13',
    text: 'Whisper Large-v3 accuracy reference ලෙස use කළත්, 8 GB RAM machine එකේ slow වෙන්න පුළුවන්.',
    start_time: 49.8,
    end_time: 54.2,
    speaker: 'Speaker 2',
    language: 'si',
  },
  {
    id: 'mock-14',
    text: 'சிங்கள மொழிக்கான fine-tuned model பற்றி ஆராய வேண்டும். இது முக்கியம்.',
    start_time: 54.6,
    end_time: 58.9,
    speaker: 'Speaker 3',
    language: 'ta',
  },

  // ── Multiple speakers taking turns ───────────────────────────────────────
  {
    id: 'mock-15',
    text: 'What is the testing plan for the diarization module?',
    start_time: 59.5,
    end_time: 62.4,
    speaker: 'Speaker 1',
    language: 'en',
  },
  {
    id: 'mock-16',
    text: 'WER සහ CER measure කරනවා. DER diarization එකට.',
    start_time: 62.8,
    end_time: 66.1,
    speaker: 'Speaker 2',
    language: 'si',
  },
  {
    id: 'mock-17',
    text: 'Real-time factor-ஐயும் latency-ஐயும் அளவிட வேண்டும்.',
    start_time: 66.5,
    end_time: 70.0,
    speaker: 'Speaker 3',
    language: 'ta',
  },
  {
    id: 'mock-18',
    text: 'Agreed. We will document all results in the evaluation report.',
    start_time: 70.4,
    end_time: 73.8,
    speaker: 'Speaker 1',
    language: 'en',
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

  const offset = loopCount * 80; // shift by ~80 s per loop
  return {
    ...segment,
    id: `${Date.now()}-${_segmentIndex}`,
    start_time: segment.start_time + offset,
    end_time:   segment.end_time   + offset,
  };
}

/** Reset the mock feed to the beginning (called on session start). */
export function resetMockSegments(): void {
  _segmentIndex = 0;
}
