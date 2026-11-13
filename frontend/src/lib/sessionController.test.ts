import { beforeEach, afterEach, expect, test, vi } from 'vitest';
import { get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
let controller: typeof import('./sessionController');
let store: typeof import('./sessionStore');
const info = (id: string, state: string) => ({ session_id: id, state, created_at: 0, started_at: 0, ended_at: null, display_name: id });
const row = { id: 1, startMs: 0, endMs: 1000, speaker: null, language: 'si', text: 'ආයුබෝවන් தமிழ்', status: 'final' as const };
beforeEach(async () => {
  vi.resetModules(); vi.resetAllMocks(); vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
  vi.mocked(listen).mockResolvedValue(() => {});
  vi.mocked(invoke).mockImplementation(async (name: string) => {
    if (name === 'list_audio_devices') return { Ok: [{ id: 'mic', name: 'Microphone', source: 'Microphone', is_default: true }] };
    if (name === 'init_session') return { Ok: info('new', 'Initializing') };
    if (name === 'get_transcript') return { Ok: [row] };
    if (name === 'search_transcript') return { Ok: [row] };
    return { Ok: info('new', name === 'stop_session' ? 'Stopped' : 'Active') };
  });
  store = await import('./sessionStore'); controller = await import('./sessionController');
});
afterEach(() => { controller.destroySessionController(); vi.unstubAllGlobals(); });
test('failed replacement keeps the previous review and cleans the candidate', async () => {
  store.sessionInfo.set(info('old', 'Stopped') as never); store.sessionStatus.set('stopped'); store.upsertSegments([row]);
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (name, args) => name === 'start_session' ? { Err: { code: 'MODEL_NOT_FOUND', message: 'Missing model' } } : original(name, args));
  await controller.handleStart('mic');
  expect(get(store.sessionInfo)?.session_id).toBe('old'); expect(get(store.segments)).toEqual([row]);
  expect(invoke).toHaveBeenCalledWith('stop_session', { sessionId: 'new' }); expect(get(store.errorMessage)).toContain('Missing model');
});
test('initialization, lifecycle, retrieval and session-scoped events use canonical contracts', async () => {
  await controller.handleStart('mic');
  expect(invoke).toHaveBeenCalledWith('init_session', { config: expect.objectContaining({ audio_source: 'Microphone', device_id: 'mic' }) });
  expect(get(store.sessionInfo)?.session_id).toBe('new'); expect(get(store.segments)).toEqual([row]);
  const subscription = vi.mocked(listen).mock.calls.find(([event]) => event === 'transcript:update')!;
  const receive = subscription[1] as (event: unknown) => void;
  receive({ payload: { sessionId: 'old', segments: [{ ...row, id: 2 }] } }); expect(get(store.segments)).toHaveLength(1);
  receive({ payload: { sessionId: 'new', segments: [{ ...row, speaker: 'Speaker 2' }] } }); expect(get(store.segments)[0].speaker).toBe('Speaker 2');
  await controller.handlePause(); expect(invoke).toHaveBeenCalledWith('pause_session', { sessionId: 'new' });
  await controller.handleResume(); expect(invoke).toHaveBeenCalledWith('resume_session', { sessionId: 'new' });
  await controller.handleStop(); expect(get(store.sessionStatus)).toBe('stopped'); expect(get(store.segments)).toHaveLength(1);
});
