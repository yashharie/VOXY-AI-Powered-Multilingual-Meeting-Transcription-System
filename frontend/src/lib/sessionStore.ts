import { writable, derived } from 'svelte/store';
import type { SessionStatus, TranscriptSegment, AudioDevice } from '../types';

// ── Session state ─────────────────────────────────────────────────────────────

export const sessionStatus = writable<SessionStatus>('idle');
export const selectedDeviceId = writable<string>('default');
export const audioDevices = writable<AudioDevice[]>([]);
export const errorMessage = writable<string | null>(null);

// ── Transcript state ──────────────────────────────────────────────────────────

export const segments = writable<TranscriptSegment[]>([]);
export const searchQuery = writable<string>('');

/** Filtered segments based on search query */
export const filteredSegments = derived(
  [segments, searchQuery],
  ([$segments, $query]) => {
    if (!$query.trim()) return $segments;
    const q = $query.toLowerCase();
    return $segments.filter(
      (s) =>
        s.text.toLowerCase().includes(q) ||
        s.speaker.toLowerCase().includes(q),
    );
  },
);

// ── Helpers ───────────────────────────────────────────────────────────────────

export function appendSegment(segment: TranscriptSegment): void {
  segments.update((all) => [...all, segment]);
}

export function updateSegmentText(id: string, text: string): void {
  segments.update((all) =>
    all.map((s) => (s.id === id ? { ...s, text } : s)),
  );
}

export function clearSession(): void {
  segments.set([]);
  errorMessage.set(null);
  searchQuery.set('');
}
