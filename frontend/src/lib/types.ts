/** Wire contracts mirrored from Rust. Transcript offsets are milliseconds. */
export type SessionState = 'Idle' | 'Initializing' | 'Active' | 'Paused' | 'Stopping' | 'Stopped' | 'Error';
export interface SessionInfo { session_id: string; state: SessionState; created_at: number; started_at: number | null; ended_at: number | null; display_name: string; }
export interface SessionConfig { audio_source: 'Loopback' | 'Microphone' | 'Both'; device_id?: string | null; language_hint: string; enable_diarization: boolean; show_partial_results: boolean; }
export interface TranscriptSegment { id: number; startMs: number; endMs: number; speaker: string | null; text: string; language: string | null; status: 'partial' | 'final'; }
export interface TranscriptUpdate { sessionId: string; segments: TranscriptSegment[]; }
export interface TranscriptDeletion { sessionId: string; segmentId: number; }
export type TranscriptSegmentPayload = TranscriptUpdate;
export interface SessionStateChangedPayload { session_id: string; previous_state: SessionState; new_state: SessionState; reason: string | null; }
export interface ErrorEventPayload { session_id: string | null; code: string; message: string; recoverable: boolean; }
export interface AudioLevelPayload { session_id: string; level: number; }
export interface AudioDevice { id: string; name: string; is_default: boolean; source: 'Loopback' | 'Microphone'; }
export type VoxyResult<T> = { Ok: T } | { Err: { code: string; message: string } };
export interface AppVersion { version: string; tauri_version: string; build_target: string; }