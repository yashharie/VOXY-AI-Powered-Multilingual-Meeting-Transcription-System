import { save } from '@tauri-apps/plugin-dialog';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { getTranscriptText, saveTranscript } from './ipc';
export async function copyTranscript(sessionId: string): Promise<void> { await writeText(await getTranscriptText(sessionId)); }
export async function exportTranscript(sessionId: string): Promise<string | null> {
  const path = await save({ title: 'Export transcript', defaultPath: 'voxy-transcript.txt', filters: [{ name: 'Text transcript', extensions: ['txt'] }] });
  if (path === null) return null;
  return saveTranscript(sessionId, path);
}
