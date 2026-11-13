// Transcript segment as agreed in the data contract
export interface TranscriptSegment {
  id: string;
  text: string;
  start_time: number;
  end_time: number;
  speaker: string;
  language: 'si' | 'ta' | 'en' | 'und';
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
