import { beforeEach, expect, test, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as ipc from './ipc';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());
test('tagged IPC success and explicit errors match Rust serialization', () => {
  expect(ipc.unwrap({ Ok: null })).toBeNull();
  expect(() => ipc.unwrap({ Err: { code: 'INVALID_TRANSCRIPT', message: 'Invalid ID' } })).toThrow('[INVALID_TRANSCRIPT] Invalid ID');
});
test('lifecycle and transcript calls send exact camelCase Tauri arguments', async () => {
  vi.mocked(invoke).mockResolvedValue({ Ok: [] });
  await ipc.startSession('s1'); expect(invoke).toHaveBeenLastCalledWith('start_session', { sessionId: 's1' });
  await ipc.pauseSession('s1'); expect(invoke).toHaveBeenLastCalledWith('pause_session', { sessionId: 's1' });
  await ipc.resumeSession('s1'); expect(invoke).toHaveBeenLastCalledWith('resume_session', { sessionId: 's1' });
  await ipc.stopSession('s1'); expect(invoke).toHaveBeenLastCalledWith('stop_session', { sessionId: 's1' });
  await ipc.editTranscriptSegment('s1', 7, 'தமிழ் සිංහල English');
  expect(invoke).toHaveBeenLastCalledWith('edit_transcript_segment', { sessionId: 's1', segmentId: 7, text: 'தமிழ் සිංහල English' });
  await ipc.deleteTranscriptSegment('s1', 7); expect(invoke).toHaveBeenLastCalledWith('delete_transcript_segment', { sessionId: 's1', segmentId: 7 });
});
