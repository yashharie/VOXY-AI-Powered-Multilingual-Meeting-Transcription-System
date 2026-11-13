import { writable, derived } from 'svelte/store';
import type { SessionStatus, TranscriptSegment, AudioDevice } from '../types';

// ── Session state ─────────────────────────────────────────────────────────────

export const sessionStatus = writable<SessionStatus>('idle');
export const selectedDeviceId = writable<string>('default');
export const audioDevices = writable<AudioDevice[]>([]);
export const errorMessage = writable<string | null>(null);
export const meetingPlatform = writable<string>('None');
export const sessionName = writable<string>('');
export const sessionStartTime = writable<number | null>(null);

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

/** Language segment counts derived from current transcript */
export const languageBreakdown = derived(segments, ($segments) => {
  const counts: Record<string, number> = { en: 0, si: 0, ta: 0, und: 0 };
  for (const s of $segments) {
    counts[s.language] = (counts[s.language] ?? 0) + 1;
  }
  return counts;
});

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

// ── Settings store (persisted to localStorage) ────────────────────────────────

export interface AppSettings {
  autoScroll: boolean;
  showSpeakerLabels: boolean;
  showLanguageLabels: boolean;
  preferredLanguages: string[];
}

const SETTINGS_KEY = 'voxy_settings';

function loadSettings(): AppSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) return { ...defaultSettings(), ...JSON.parse(raw) };
  } catch { /* ignore */ }
  return defaultSettings();
}

function defaultSettings(): AppSettings {
  return {
    autoScroll: true,
    showSpeakerLabels: true,
    showLanguageLabels: true,
    preferredLanguages: ['en', 'si', 'ta'],
  };
}

function createSettingsStore() {
  const { subscribe, set, update } = writable<AppSettings>(loadSettings());
  return {
    subscribe,
    set(value: AppSettings) {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(value));
      set(value);
    },
    update(fn: (s: AppSettings) => AppSettings) {
      update((current) => {
        const next = fn(current);
        localStorage.setItem(SETTINGS_KEY, JSON.stringify(next));
        return next;
      });
    },
  };
}

export const appSettings = createSettingsStore();
