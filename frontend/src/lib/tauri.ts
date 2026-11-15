/**
 * Tauri IPC bridge.
 *
 * Calls real Tauri commands when running inside the Tauri shell.
 * Falls back to mock implementations during browser-based development.
 */

import type { AudioDevice, SessionStatus, TranscriptSegment } from '../types';
import { isTauri as coreIsTauri, invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// ── Tauri availability check ──────────────────────────────────────────────────

export function isTauri(): boolean {
  if (typeof window === 'undefined') return false;
  return (
    coreIsTauri() ||
    '__TAURI_INTERNALS__' in window ||
    '__TAURI__' in window ||
    Boolean((window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__)
  );
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(command, args);
}

// ── Session info type (mirrors Rust SessionInfo) ──────────────────────────────

export interface SessionInfo {
  session_id: string;
  state: string;
  created_at: number;
  started_at: number | null;
  ended_at: number | null;
  display_name: string;
}

type IpcResult<T> =
  | { Ok: T }
  | { Err: { code: string; message: string } }
  | T
  | { code: string; message: string };

function unwrap<T>(result: any): T {
  if (result == null) return result as T;
  if (typeof result === 'object') {
    if ('Ok' in result) {
      return result.Ok as T;
    }
    if ('Err' in result) {
      const err = result.Err;
      const code = err?.code ?? 'ERROR';
      const msg = err?.message ?? (typeof err === 'string' ? err : 'Unknown error');
      throw new Error(`[${code}] ${msg}`);
    }
    if ('code' in result && 'message' in result) {
      throw new Error(`[${result.code}] ${result.message}`);
    }
  }
  return result as T;
}

// ── Session lifecycle commands ────────────────────────────────────────────────

export interface SessionConfig {
  audio_source: 'Loopback' | 'Microphone' | 'Both';
  language_hint: string;
  enable_diarization: boolean;
  show_partial_results: boolean;
}

export async function initSession(config: SessionConfig): Promise<SessionInfo> {
  if (isTauri()) {
    const result = await invoke<IpcResult<SessionInfo>>('init_session', { config });
    return unwrap(result);
  }
  // Mock for browser dev
  return {
    session_id: `mock-${Date.now()}`,
    state: 'Initializing',
    created_at: Date.now() / 1000,
    started_at: null,
    ended_at: null,
    display_name: 'Mock Session',
  };
}

export async function startSession(sessionId: string): Promise<SessionInfo> {
  if (isTauri()) {
    const result = await invoke<IpcResult<SessionInfo>>('start_session', { sessionId });
    return unwrap(result);
  }
  return { session_id: sessionId, state: 'Active', created_at: 0, started_at: Date.now() / 1000, ended_at: null, display_name: '' };
}

export async function pauseSession(sessionId: string): Promise<SessionInfo> {
  if (isTauri()) {
    const result = await invoke<IpcResult<SessionInfo>>('pause_session', { sessionId });
    return unwrap(result);
  }
  return { session_id: sessionId, state: 'Paused', created_at: 0, started_at: null, ended_at: null, display_name: '' };
}

export async function resumeSession(sessionId: string): Promise<SessionInfo> {
  if (isTauri()) {
    const result = await invoke<IpcResult<SessionInfo>>('resume_session', { sessionId });
    return unwrap(result);
  }
  return { session_id: sessionId, state: 'Active', created_at: 0, started_at: null, ended_at: null, display_name: '' };
}

export async function stopSession(sessionId: string): Promise<SessionInfo> {
  if (isTauri()) {
    const result = await invoke<IpcResult<SessionInfo>>('stop_session', { sessionId });
    return unwrap(result);
  }
  return { session_id: sessionId, state: 'Stopped', created_at: 0, started_at: null, ended_at: null, display_name: '' };
}

// ── Audio device commands ─────────────────────────────────────────────────────

export async function listAudioDevices(): Promise<AudioDevice[]> {
  try {
    if (typeof navigator !== 'undefined' && navigator.mediaDevices) {
      // First try to request audio stream briefly so Windows/browser populates real hardware device labels
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        stream.getTracks().forEach((t) => t.stop());
      } catch {
        // User may have already allowed or prompt handled
      }

      const devs = await navigator.mediaDevices.enumerateDevices();
      const audioInputs = devs.filter((d) => d.kind === 'audioinput');
      const audioOutputs = devs.filter((d) => d.kind === 'audiooutput');

      const result: AudioDevice[] = [];

      // Always include Default option
      result.push({
        id: 'default',
        name: 'Default Audio (System Loopback / Mic)',
        is_default: true,
      });

      // Add actual input devices (Microphones, Stereo Mix, Headsets)
      audioInputs.forEach((d, i) => {
        if (d.deviceId && d.deviceId !== 'default') {
          result.push({
            id: d.deviceId,
            name: d.label || `Microphone ${i + 1}`,
            is_default: false,
          });
        }
      });

      // Add output devices (Speakers, Virtual Audio Cables, Headphones)
      audioOutputs.forEach((d, i) => {
        if (d.deviceId && d.deviceId !== 'default') {
          result.push({
            id: d.deviceId,
            name: d.label ? `Output: ${d.label}` : `Speaker ${i + 1}`,
            is_default: false,
          });
        }
      });

      if (result.length > 1) {
        return result;
      }
    }
  } catch (err) {
    console.warn('listAudioDevices error:', err);
  }

  return [
    { id: 'default', name: 'Default Audio (System Loopback & Mic)', is_default: true },
    { id: 'dev-speakers', name: 'Speakers / Headset (Meeting Audio)', is_default: false },
    { id: 'dev-mic', name: 'Microphone (Voice Input)', is_default: false },
    { id: 'dev-stereomix', name: 'Stereo Mix (System Loopback)', is_default: false },
  ];
}

// ── Tauri event listeners ─────────────────────────────────────────────────────

export async function onTranscriptSegment(
  callback: (segment: TranscriptSegment) => void,
): Promise<() => void> {
  if (isTauri()) {
    const unlisten = await listen<Record<string, unknown>>('voxy://transcript-segment', (event) => {
      const payload = event.payload;
      const normalized: TranscriptSegment = {
        id: (payload.id as string) || `seg-${payload.index ?? Date.now()}`,
        text: (payload.text as string) ?? '',
        start_time: (payload.start_time as number) ?? 0,
        end_time: (payload.end_time as number) ?? 0,
        speaker: (payload.speaker as string) || 'Speaker',
        language: (payload.language as 'si' | 'ta' | 'en' | 'und') || 'und',
      };
      callback(normalized);
    });
    return unlisten;
  }
  return () => {};
}

export async function onSessionStatusChange(
  callback: (status: SessionStatus) => void,
): Promise<() => void> {
  if (isTauri()) {
    const unlisten = await listen<{ new_state: string }>('voxy://session-state-changed', (event) => {
      // Map Rust SessionState to our frontend SessionStatus
      const stateMap: Record<string, SessionStatus> = {
        Idle: 'idle', Initializing: 'starting', Active: 'active',
        Paused: 'paused', Stopping: 'stopping', Stopped: 'stopped', Error: 'idle',
      };
      const mapped = stateMap[event.payload.new_state] ?? 'idle';
      callback(mapped as SessionStatus);
    });
    return unlisten;
  }
  return () => {};
}

// ── Diagnostics ───────────────────────────────────────────────────────────────

export interface DiagnosticsPayload {
  latency_ms: number | null;
  cpu_percent: number | null;
  ram_mb: number | null;
}

export async function onDiagnosticsUpdate(
  callback: (payload: DiagnosticsPayload) => void,
): Promise<() => void> {
  if (isTauri()) {
    const unlisten = await listen<DiagnosticsPayload>('diagnostics_update', (event) => {
      callback(event.payload);
    });
    return unlisten;
  }
  return () => {};
}

export async function exportTranscriptViaBackend(content: string): Promise<void> {
  if (isTauri()) {
    await tauriInvoke('export_transcript', { content });
  }
}
