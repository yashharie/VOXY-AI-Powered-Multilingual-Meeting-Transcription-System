// Transcript segment as agreed in the data contract
export interface TranscriptSegment {
  id: string;
  text: string;
  start_time: number;
  end_time: number;
  /** Speaker label from diarization, or null/undefined if diarization is off */
  speaker: string | null;
  language: 'si' | 'ta' | 'en' | 'und';
  is_partial?: boolean;
}

// Session lifecycle states
export type SessionStatus =
  | 'idle'
  | 'starting'
  | 'active'
  | 'paused'
  | 'stopping'
  | 'stopped';

// Audio device info from the backend
export interface AudioDevice {
  id: string;
  name: string;
  is_default: boolean;
}
