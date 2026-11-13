import { beforeEach, expect, test, vi } from 'vitest';
import { save } from '@tauri-apps/plugin-dialog';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import * as ipc from './ipc';
import { copyTranscript, exportTranscript } from './transcriptActions';
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn() }));
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({ writeText: vi.fn() }));
vi.mock('./ipc', () => ({ getTranscriptText: vi.fn(), saveTranscript: vi.fn() }));
beforeEach(() => vi.resetAllMocks());
test('dialog cancellation is successful and never writes a file', async () => {
  vi.mocked(save).mockResolvedValue(null);
  await expect(exportTranscript('s1')).resolves.toBeNull(); expect(ipc.saveTranscript).not.toHaveBeenCalled();
});
test('native copy uses the backend TXT formatter byte for byte', async () => {
  const text = 'VOXY Meeting Transcript\n[00:00] Speaker 1: தமிழ் සිංහල English\n';
  vi.mocked(ipc.getTranscriptText).mockResolvedValue(text);
  await copyTranscript('s1'); expect(writeText).toHaveBeenCalledWith(text);
});
test('native save forwards Unicode locations and surfaces write errors', async () => {
  const path = 'C:\\exports\\தமிழ்-සිංහල.txt';
  vi.mocked(save).mockResolvedValue(path); vi.mocked(ipc.saveTranscript).mockResolvedValue(path);
  await expect(exportTranscript('s1')).resolves.toBe(path); expect(ipc.saveTranscript).toHaveBeenCalledWith('s1', path);
  vi.mocked(ipc.saveTranscript).mockRejectedValue(new Error('Access denied'));
  await expect(exportTranscript('s1')).rejects.toThrow('Access denied');
});
