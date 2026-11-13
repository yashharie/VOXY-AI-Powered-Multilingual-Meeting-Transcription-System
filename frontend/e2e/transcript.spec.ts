import { test, expect } from '@playwright/test';
import { fileURLToPath } from 'node:url';

// Serve the production bundle through Playwright routing: no external server
// or Windows child-process cleanup is required. Native IPC remains test-only.

test('existing UI lifecycle, stable updates, Unicode edit/search/delete, copy and save/cancel (mocked IPC)', async ({ page }) => {
  await page.route('http://voxy.test/**', async route => {
    const pathname = new URL(route.request().url()).pathname;
    const asset = new URL(pathname === '/' ? 'index.html' : pathname.slice(1), new URL('../../dist/', import.meta.url));
    await route.fulfill({ path: fileURLToPath(asset) });
  });
  await page.addInitScript(() => {
    const w = window as any;
    const callbacks = new Map<number, (event: unknown) => void>(); const listeners = new Map<string, number>(); let next = 0;
    let sessionId = 's1'; let state = 'Initializing'; let failStart = false; let savePath: string | null = null;
    const rows = [
      { id: 1, startMs: 0, endMs: 1000, speaker: 'Speaker 1', language: 'ta', text: 'வணக்கம்', status: 'final' },
      { id: 2, startMs: 1000, endMs: 2000, speaker: 'Speaker 2', language: 'si', text: 'ආයුබෝවන්', status: 'final' },
      { id: 3, startMs: 2000, endMs: 3000, speaker: null, language: 'en', text: 'Hello everyone', status: 'final' },
    ];
    const info = () => ({ session_id: sessionId, state, created_at: 0, started_at: 0, ended_at: null, display_name: 'Meeting' });
    const text = () => 'VOXY Meeting Transcript\n=======================\n\n' + rows.map(s => `[00:0${Math.floor(s.startMs / 1000)}] ${s.speaker ?? 'Unknown speaker'}: ${s.text}\n`).join('');
    w.__VOXY_TEST__ = {
      copied: '', saved: null, fail: () => { failStart = true; }, dialog: (path: string | null) => { savePath = path; },
      update: () => { const row = rows[0]; row.speaker = 'Late speaker'; callbacks.get(listeners.get('transcript:update')!)?.({ payload: { sessionId, segments: [row] } }); },
    };
    w.__TAURI_INTERNALS__ = {
      transformCallback: (callback: (event: unknown) => void) => { callbacks.set(++next, callback); return next; },
      unregisterCallback: (id: number) => callbacks.delete(id),
      invoke: async (command: string, args: any = {}) => {
        if (command === 'plugin:event|listen') { listeners.set(args.event, args.handler); return args.handler; }
        if (command === 'plugin:event|unlisten') return;
        if (command === 'plugin:dialog|save') return savePath;
        if (command === 'plugin:clipboard-manager|write_text') { w.__VOXY_TEST__.copied = args.text; return; }
        switch (command) {
          case 'list_audio_devices': return { Ok: [{ id: 'output', name: 'System audio', source: 'Loopback', is_default: true }] };
          case 'init_session': state = 'Initializing'; return { Ok: info() };
          case 'start_session': if (failStart) return { Err: { code: 'MODEL_NOT_FOUND', message: 'Missing model' } }; state = 'Active'; return { Ok: info() };
          case 'pause_session': state = 'Paused'; return { Ok: info() };
          case 'resume_session': state = 'Active'; return { Ok: info() };
          case 'stop_session': state = 'Stopped'; return { Ok: info() };
          case 'get_session_info': return { Ok: info() };
          case 'get_transcript': return { Ok: [...rows] };
          case 'get_transcript_text': return { Ok: text() };
          case 'search_transcript': return { Ok: rows.filter(s => `${s.text} ${s.speaker ?? ''}`.toLowerCase().includes(args.query.toLowerCase())) };
          case 'edit_transcript_segment': { const row = rows.find(s => s.id === args.segmentId)!; row.text = args.text; return { Ok: row }; }
          case 'delete_transcript_segment': rows.splice(rows.findIndex(s => s.id === args.segmentId), 1); return { Ok: { sessionId, segmentId: args.segmentId } };
          case 'save_transcript': w.__VOXY_TEST__.saved = { ...args, text: text() }; return { Ok: args.filePath };
          default: throw new Error(`Unexpected IPC: ${command}`);
        }
      },
    };
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Start transcription session' }).click();
  await expect(page.locator('.segment')).toHaveCount(3);
  await page.evaluate(() => (window as any).__VOXY_TEST__.update());
  await expect(page.locator('.segment-speaker').first()).toHaveText('Late speaker');
  await page.getByRole('button', { name: 'Edit this segment' }).first().click();
  await page.getByRole('textbox', { name: 'Edit transcript segment' }).fill('Edited தமிழ் සිංහල English');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.segment-text').first()).toHaveText('Edited தமிழ் සිංහල English');
  await page.getByRole('searchbox').fill('Speaker 2'); await expect(page.locator('.segment')).toHaveCount(1);
  await page.getByRole('searchbox').fill(''); await expect(page.locator('.segment')).toHaveCount(3);
  await page.getByRole('button', { name: 'Delete this segment' }).last().click(); await expect(page.locator('.segment')).toHaveCount(2);
  await page.getByRole('button', { name: 'Copy full transcript to clipboard' }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__VOXY_TEST__.copied)).toContain('Edited தமிழ் සිංහල English');
  await page.getByRole('button', { name: 'Export transcript as text file' }).click();
  expect(await page.evaluate(() => (window as any).__VOXY_TEST__.saved)).toBeNull();
  await page.evaluate(() => (window as any).__VOXY_TEST__.dialog('C:\\exports\\தமிழ்-සිංහල.txt'));
  await page.getByRole('button', { name: 'Export transcript as text file' }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__VOXY_TEST__.saved?.format)).toBe('txt');
  await page.getByRole('button', { name: 'Pause transcription session' }).click();
  await page.getByRole('button', { name: 'Resume transcription session' }).click();
  await page.getByRole('button', { name: 'Stop transcription session' }).click(); await expect(page.locator('.segment')).toHaveCount(2);
  await page.evaluate(() => (window as any).__VOXY_TEST__.fail());
  await page.getByRole('button', { name: 'Start transcription session' }).click();
  await expect(page.locator('.error-msg')).toContainText('Missing model'); await expect(page.locator('.segment')).toHaveCount(2);
});
