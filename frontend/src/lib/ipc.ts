import { invoke } from '@tauri-apps/api/core';
import type { AppVersion, SessionConfig, SessionInfo, TranscriptSegment, TranscriptDeletion, AudioDevice, VoxyResult } from './types';
export function unwrap<T>(result: VoxyResult<T>): T {
  if ('Ok' in result) return result.Ok;
  throw new Error(`[${result.Err.code}] ${result.Err.message}`);
}
async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> { return unwrap(await invoke<VoxyResult<T>>(name, args)); }
export const ping = () => invoke<string>('ping');
export const getAppVersion = () => invoke<AppVersion>('get_app_version');
export const listAudioDevices = () => command<AudioDevice[]>('list_audio_devices');
export const initSession = (config: SessionConfig) => command<SessionInfo>('init_session', { config });
export const startSession = (sessionId: string) => command<SessionInfo>('start_session', { sessionId });
export const pauseSession = (sessionId: string) => command<SessionInfo>('pause_session', { sessionId });
export const resumeSession = (sessionId: string) => command<SessionInfo>('resume_session', { sessionId });
export const stopSession = (sessionId: string) => command<SessionInfo>('stop_session', { sessionId });
export const getSessionInfo = (sessionId: string) => command<SessionInfo>('get_session_info', { sessionId });
export const getTranscript = (sessionId: string) => command<TranscriptSegment[]>('get_transcript', { sessionId });
export const getTranscriptSegment = (sessionId: string, segmentId: number) => command<TranscriptSegment>('get_transcript_segment', { sessionId, segmentId });
export const editTranscriptSegment = (sessionId: string, segmentId: number, text: string) => command<TranscriptSegment>('edit_transcript_segment', { sessionId, segmentId, text });
export const deleteTranscriptSegment = (sessionId: string, segmentId: number) => command<TranscriptDeletion>('delete_transcript_segment', { sessionId, segmentId });
export const searchTranscript = (sessionId: string, query: string) => command<TranscriptSegment[]>('search_transcript', { sessionId, query });
export const getTranscriptText = (sessionId: string) => command<string>('get_transcript_text', { sessionId });
export const saveTranscript = (sessionId: string, filePath: string) => command<string>('save_transcript', { sessionId, format: 'txt', filePath });