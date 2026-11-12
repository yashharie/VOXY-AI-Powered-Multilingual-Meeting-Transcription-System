import type { TranscriptSegment } from '../types';

/**
 * Mock transcript segments covering Sinhala, Tamil, English,
 * and code-switching scenarios for UI development and demo purposes.
 */
export const MOCK_SEGMENTS: TranscriptSegment[] = [
  {
    id: '1',
    text: 'Good morning everyone. Let us get started with today\'s meeting.',
    start_time: 0.0,
    end_time: 3.8,
    speaker: 'Speaker 1',
    language: 'en',
  },
  {
    id: '2',
    text: 'ආයුබෝවන්. අද අපි නව ව්‍යාපෘතිය ගැන කතා කරමු.',
    start_time: 4.1,
    end_time: 7.5,
    speaker: 'Speaker 2',
    language: 'si',
  },
  {
    id: '3',
    text: 'வணக்கம். இன்றைய கூட்டத்தில் நாம் திட்டத்தை விவாதிப்போம்.',
    start_time: 8.0,
    end_time: 11.2,
    speaker: 'Speaker 3',
    language: 'ta',
  },
  {
    id: '4',
    text: 'Yes, the system architecture looks good. අපි Tauri use කරන්නේ ඇයි?',
    start_time: 12.0,
    end_time: 15.6,
    speaker: 'Speaker 1',
    language: 'en',
  },
  {
    id: '5',
    text: 'Tauri ගොඩක් lightweight. It uses the system WebView instead of bundling Chromium.',
    start_time: 16.0,
    end_time: 20.4,
    speaker: 'Speaker 2',
    language: 'si',
  },
  {
    id: '6',
    text: 'சரி. Performance-ஐப் பார்த்தால் இது நல்லது.',
    start_time: 21.0,
    end_time: 24.1,
    speaker: 'Speaker 3',
    language: 'ta',
  },
  {
    id: '7',
    text: 'We also need to evaluate Whisper Base and Small on CPU-only hardware.',
    start_time: 25.0,
    end_time: 29.3,
    speaker: 'Speaker 1',
    language: 'en',
  },
];

let _segmentIndex = 0;

/**
 * Returns the next mock segment in sequence.
 * Loops back to the start when all segments have been used.
 */
export function nextMockSegment(): TranscriptSegment {
  const segment = MOCK_SEGMENTS[_segmentIndex % MOCK_SEGMENTS.length];
  _segmentIndex++;

  // Give each replayed segment a unique id and shifted timestamps
  const offset = Math.floor(_segmentIndex / MOCK_SEGMENTS.length) * 30;
  return {
    ...segment,
    id: `${Date.now()}-${_segmentIndex}`,
    start_time: segment.start_time + offset,
    end_time: segment.end_time + offset,
  };
}

export function resetMockSegments(): void {
  _segmentIndex = 0;
}
