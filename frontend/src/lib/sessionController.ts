/** One app-level controller and one frontend transcript cache. Backend owns data. */
import { get } from 'svelte/store';
import * as ipc from './ipc';
import * as events from './events';
import { isTauri } from './tauri';
import { sessionStatus, sessionInfo, busy, audioLevel, audioDevices, selectedDeviceId, errorMessage, sessionStartTime, segments, searchQuery, searchMatches, clearSession, upsertSegments, removeSegment, acknowledgeEdit, applySnapshot, transcriptRevision } from './sessionStore';
import type { SessionConfig, SessionInfo, SessionState } from './types';
let initialized: Promise<void> | null = null;
let unlisteners: (() => void)[] = [];
let subscriptions: (() => void)[] = [];
let searchGeneration = 0;
let configured: SessionConfig | null = null;
let pendingId: string | null = null;
const currentId = () => get(sessionInfo)?.session_id ?? null;
function status(state: SessionState) { return state === 'Initializing' ? 'starting' : state.toLowerCase() as 'idle' | 'active' | 'paused' | 'stopping' | 'stopped' | 'error'; }
function acceptInfo(info: SessionInfo) { sessionInfo.set(info); sessionStatus.set(status(info.state)); }
export function configureSession(config: SessionConfig) { configured = config; }
export async function initSessionController(): Promise<void> {
  if (initialized) return initialized;
  if (!isTauri()) { errorMessage.set('Open VOXY in the desktop app to transcribe and export.'); return; }
  initialized = (async () => {
    try {
      unlisteners.push(await events.onTranscriptSegment(payload => { if (payload.sessionId === currentId()) upsertSegments(payload.segments); }));
      unlisteners.push(await events.onTranscriptDeleted(payload => { if (payload.sessionId === currentId()) removeSegment(payload.segmentId); }));
      unlisteners.push(await events.onSessionStateChanged(payload => {
        const info = get(sessionInfo);
        if (info?.session_id === payload.session_id) acceptInfo({ ...info, state: payload.new_state });
      }));
      unlisteners.push(await events.onError(payload => {
        if (!payload.session_id || payload.session_id === currentId() || payload.session_id === pendingId) errorMessage.set(`[${payload.code}] ${payload.message}`);
      }));
      unlisteners.push(await events.onAudioLevel(payload => { if (payload.session_id === currentId()) audioLevel.set(payload.level); }));
      unlisteners.push(await events.onSessionCleanupDone(id => { if (id === currentId()) audioLevel.set(0); }));
      const devices = await ipc.listAudioDevices(); audioDevices.set(devices);
      if (!devices.some(d => d.id === get(selectedDeviceId))) selectedDeviceId.set(devices.find(d => d.is_default)?.id ?? devices[0]?.id ?? 'default');
      subscriptions.push(searchQuery.subscribe(() => { searchMatches.set(null); void refreshSearch(); }));
      subscriptions.push(segments.subscribe(() => { void refreshSearch(); }));
    } catch (error) { destroySessionController(); throw error; }
  })();
  return initialized;
}
export function destroySessionController() {
  for (const stop of unlisteners) stop(); unlisteners = [];
  for (const stop of subscriptions) stop(); subscriptions = [];
  initialized = null; searchGeneration++;
}
export async function refreshTranscript(): Promise<void> {
  const id = currentId(); if (!id) return;
  const requestedAt = transcriptRevision(); const snapshot = await ipc.getTranscript(id);
  if (id === currentId()) applySnapshot(snapshot, requestedAt);
}
export async function refreshSearch(): Promise<void> {
  const version = ++searchGeneration; const query = get(searchQuery); const id = currentId();
  if (!query.trim() || !id) { searchMatches.set(null); return; }
  try { const found = await ipc.searchTranscript(id, query); if (version === searchGeneration && id === currentId()) searchMatches.set(found.map(s => s.id)); }
  catch (error) { if (version === searchGeneration) errorMessage.set(String(error)); }
}
export async function handleStart(deviceId = get(selectedDeviceId)): Promise<void> {
  if (get(busy)) return; busy.set(true); errorMessage.set(null);
  const previous = get(sessionInfo); const previousStatus = get(sessionStatus);
  sessionStatus.set('starting');
  try {
    if (!isTauri()) throw new Error('Transcription requires the VOXY desktop app.');
    await initSessionController();
    if (previous?.state === 'Error') { try { await ipc.stopSession(previous.session_id); } catch { /* backend stop can report a final inference error after cleanup */ } }
    const device = get(audioDevices).find(d => d.id === deviceId);
    const config = configured ?? { audio_source: device?.source ?? 'Loopback', device_id: device?.id ?? null, language_hint: 'auto', enable_diarization: false, show_partial_results: true };
    const candidate = await ipc.initSession(config); pendingId = candidate.session_id;
    const started = await ipc.startSession(candidate.session_id);
    // No previous review data is removed before backend start succeeds.
    clearSession(); acceptInfo(started); pendingId = null; sessionStartTime.set(Date.now());
    await refreshTranscript(); acceptInfo(await ipc.getSessionInfo(started.session_id));
  } catch (error) {
    if (pendingId) { try { await ipc.stopSession(pendingId); } catch { /* preserve original startup error */ } pendingId = null; }
    if (currentId() === previous?.session_id || (!previous && !currentId())) { sessionStatus.set(previousStatus); }
    errorMessage.set(String(error));
  } finally { busy.set(false); }
}
async function lifecycle(action: 'pause' | 'resume' | 'stop') {
  const id = currentId(); if (!id || get(busy)) return;
  busy.set(true); errorMessage.set(null);
  try {
    const info = await ({ pause: ipc.pauseSession, resume: ipc.resumeSession, stop: ipc.stopSession }[action])(id);
    acceptInfo(info); await refreshTranscript();
    if (action === 'stop') sessionStartTime.set(null);
  } catch (error) {
    errorMessage.set(String(error));
    try { acceptInfo(await ipc.getSessionInfo(id)); await refreshTranscript(); } catch { /* keep the operation error */ }
  } finally { busy.set(false); }
}
export const handlePause = () => lifecycle('pause');
export const handleResume = () => lifecycle('resume');
export const handleStop = () => lifecycle('stop');
export async function editSegment(id: number, text: string) {
  const session = currentId(); if (!session) throw new Error('No current session');
  acknowledgeEdit(await ipc.editTranscriptSegment(session, id, text)); await refreshSearch();
}
export async function deleteSegment(id: number) {
  const session = currentId(); if (!session) throw new Error('No current session');
  await ipc.deleteTranscriptSegment(session, id); removeSegment(id); await refreshSearch();
}
export async function transcriptText() { const id = currentId(); if (!id) throw new Error('No current session'); return ipc.getTranscriptText(id); }