import { beforeEach, expect, test } from 'vitest';
import { get } from 'svelte/store';
import { clearSession, segments, upsertSegments, removeSegment, acknowledgeEdit, applySnapshot, transcriptRevision } from './sessionStore';
import type { TranscriptSegment } from './types';
const row = (id: number, status: 'partial' | 'final' = 'final'): TranscriptSegment => ({ id, startMs: id * 1000, endMs: id * 1000 + 999, speaker: null, language: 'ta', text: 'தமிழ் සිංහල English', status });
beforeEach(clearSession);
test('stable IDs update partial/final text and late speaker metadata without duplicates', () => {
  upsertSegments([row(2), row(1, 'partial')]); upsertSegments([{ ...row(1), speaker: 'Speaker 2' }]);
  expect(get(segments).map(s => s.id)).toEqual([1, 2]); expect(get(segments)[0].speaker).toBe('Speaker 2');
  upsertSegments([row(1, 'partial')]); expect(get(segments)[0].status).toBe('final');
});
test('old snapshots cannot resurrect deletions or overwrite later edits', () => {
  upsertSegments([row(1), row(2)]); const requestedAt = transcriptRevision();
  removeSegment(2); acknowledgeEdit({ ...row(1), text: 'Edited தமிழ்' });
  applySnapshot([row(1), row(2)], requestedAt); expect(get(segments)).toHaveLength(1); expect(get(segments)[0].text).toBe('Edited தமிழ்');
});
test('late metadata keeps local acknowledged edits and replacement clears edit flags', () => {
  acknowledgeEdit({ ...row(1), text: 'Edited' }); upsertSegments([{ ...row(1), speaker: 'Late speaker' }]);
  expect(get(segments)[0]).toMatchObject({ text: 'Edited', speaker: 'Late speaker' });
  clearSession(); upsertSegments([row(1)]); expect(get(segments)[0].text).toBe(row(1).text);
});

test('queued events cannot resurrect a deleted segment; a new session can reuse its ID', () => {
  upsertSegments([row(1)]); removeSegment(1); upsertSegments([{ ...row(1), speaker: 'Late speaker' }]);
  applySnapshot([row(1)], transcriptRevision()); expect(get(segments)).toEqual([]);
  clearSession(); upsertSegments([row(1)]); expect(get(segments)).toEqual([row(1)]);
});
