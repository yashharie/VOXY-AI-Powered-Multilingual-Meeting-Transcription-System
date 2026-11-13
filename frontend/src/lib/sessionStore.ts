import { writable, derived, get } from 'svelte/store';
import type { SessionStatus, TranscriptSegment, AudioDevice } from '../types';
import type { SessionInfo } from './types';
export const sessionStatus = writable<SessionStatus>('idle');
export const sessionInfo = writable<SessionInfo | null>(null);
export const busy = writable(false);
export const audioLevel = writable(0);
export const selectedDeviceId = writable('default');
export const audioDevices = writable<AudioDevice[]>([]);
export const errorMessage = writable<string | null>(null);
export const meetingPlatform = writable('None');
export const sessionName = writable('');
export const sessionStartTime = writable<number | null>(null);
export const segments = writable<TranscriptSegment[]>([]);
export const searchQuery = writable('');
export const searchMatches = writable<number[] | null>(null);
let revision = 0;
const touched = new Map<number, number>();
const edited = new Set<number>();
const deleted = new Set<number>();
export const transcriptRevision = () => revision;
const ordered = (all: TranscriptSegment[]) => all.sort((a, b) => a.startMs - b.startMs || a.endMs - b.endMs || a.id - b.id);
export function appendSegment(segment: TranscriptSegment): void { upsertSegments([segment]); }
export function upsertSegments(incoming: TranscriptSegment[]): void {
  segments.update(all => {
    const byId = new Map(all.map(segment => [segment.id, segment]));
    for (const segment of incoming) {
      if (deleted.has(segment.id)) continue;
      const old = byId.get(segment.id);
      if (old?.status === 'final' && segment.status === 'partial') continue;
      byId.set(segment.id, edited.has(segment.id) && old ? { ...segment, text: old.text } : segment);
      touched.set(segment.id, ++revision);
    }
    return ordered([...byId.values()]);
  });
}
export function acknowledgeEdit(segment: TranscriptSegment): void {
  edited.delete(segment.id); upsertSegments([segment]); edited.add(segment.id);
}
export function removeSegment(id: number): void { touched.set(id, ++revision); deleted.add(id); edited.delete(id); segments.update(all => all.filter(s => s.id !== id)); }
export function applySnapshot(snapshot: TranscriptSegment[], requestedAt: number): void {
  const all = get(segments);
  const byId = new Map(snapshot.filter(s => !deleted.has(s.id) && (touched.get(s.id) ?? 0) <= requestedAt).map(s => [s.id, s]));
  for (const segment of all) if ((touched.get(segment.id) ?? 0) > requestedAt) byId.set(segment.id, segment);
  segments.set(ordered([...byId.values()]));
}
export function clearSession(): void { segments.set([]); touched.clear(); edited.clear(); deleted.clear(); revision++; searchQuery.set(''); searchMatches.set(null); errorMessage.set(null); }
export const filteredSegments = derived([segments, searchQuery, searchMatches], ([$segments, $query, $matches]) => {
  if (!$query.trim()) return $segments;
  if ($matches) { const ids = new Set($matches); return $segments.filter(s => ids.has(s.id)); }
  const q = $query.toLowerCase();
  return $segments.filter(s => s.text.toLowerCase().includes(q) || (s.speaker ?? '').toLowerCase().includes(q));
});
export const languageBreakdown = derived(segments, all => {
  const counts: Record<string, number> = { en: 0, si: 0, ta: 0, und: 0 };
  for (const segment of all) { const language = segment.language ?? 'und'; counts[language] = (counts[language] ?? 0) + 1; }
  return counts;
});
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
