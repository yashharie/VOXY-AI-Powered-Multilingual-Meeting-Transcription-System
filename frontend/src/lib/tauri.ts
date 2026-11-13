/**
 * Tauri IPC bridge.
 *
 * Calls real Tauri commands when running inside the Tauri shell.
 * Falls back to mock implementations during browser-based development
 * so the UI can be developed and demoed without the Rust backend.
 */

import type { AudioDevice, SessionStatus, TranscriptSegment } from '../types';

// ── Tauri availability check ─────────────────────────────────────────────────

function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window;
}

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(command, args);
  }
  throw new Error(`Tauri not available: cannot invoke "${command}"`);
}

// ── Session commands ─────────────────────────────────────────────────────────

export async function startSession(deviceId: string): Promise<void> {
  if (isTauri()) {
    await invokeCommand('start_session', { deviceId });
  }
  // mock: no-op — session state is managed in the store
}

export async function pauseSession(): Promise<void> {
  if (isTauri()) {
    await invokeCommand('pause_session');
  }
}

export async function resumeSession(): Promise<void> {
  if (isTauri()) {
    await invokeCommand('resume_session');
  }
}

export async function stopSession(): Promise<void> {
  if (isTauri()) {
    await invokeCommand('stop_session');
  }
}

// ── Audio device commands ────────────────────────────────────────────────────

export async function listAudioDevices(): Promise<AudioDevice[]> {
  if (isTauri()) {
    return invokeCommand<AudioDevice[]>('list_audio_devices');
  }
  // Mock devices for browser development
  return [
    { id: 'default', name: 'Default Output Device', is_default: true },
    { id: 'dev-1', name: 'Speakers (Realtek Audio)', is_default: false },
    { id: 'dev-2', name: 'Headphones (USB Audio)', is_default: false },
  ];
}

// ── Transcript event listener ────────────────────────────────────────────────

export async function onTranscriptSegment(
  callback: (segment: TranscriptSegment) => void,
): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<TranscriptSegment>('transcript_segment', (event) => {
      callback(event.payload);
    });
    return unlisten;
  }
  // Return a no-op unsubscribe for browser development
  return () => {};
}

export async function onSessionStatusChange(
  callback: (status: SessionStatus) => void,
): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<SessionStatus>('session_status', (event) => {
      callback(event.payload);
    });
    return unlisten;
  }
  return () => {};
}

// ── Transcript export command ────────────────────────────────────────────────

/**
 * Ask the Rust backend to save the transcript to a file chosen by the user.
 * Falls back to browser download when running outside Tauri.
 */
export async function exportTranscriptViaBackend(content: string): Promise<void> {
  if (isTauri()) {
    await invokeCommand('export_transcript', { content });
  }
  // Browser fallback is handled directly in TranscriptActions via downloadTextFile
}

// ── Diagnostics event listener ────────────────────────────────────────────────

export interface DiagnosticsPayload {
  latency_ms: number | null;
  cpu_percent: number | null;
  ram_mb: number | null;
}

export async function onDiagnosticsUpdate(
  callback: (payload: DiagnosticsPayload) => void,
): Promise<() => void> {
  if (isTauri()) {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<DiagnosticsPayload>('diagnostics_update', (event) => {
      callback(event.payload);
    });
    return unlisten;
  }
  return () => {};
}
