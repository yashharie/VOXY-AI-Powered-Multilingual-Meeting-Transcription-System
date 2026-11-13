/**
 * VOXY — Session Svelte store.
 * Manages frontend session state, keeps it in sync with backend events.
 */

import { writable, derived, get } from "svelte/store";
import type {
  SessionConfig,
  SessionInfo,
  SessionState,
  TranscriptSegment,
} from "$lib/types";
import {
  initSession,
  startSession,
  pauseSession,
  resumeSession,
  stopSession,
  getSessionInfo,
} from "$lib/ipc";
import {
  onSessionStateChanged,
  onTranscriptSegment,
  onError,
  onAudioLevel,
  onSessionCleanupDone,
} from "$lib/events";
import type { UnlistenFn } from "@tauri-apps/api/event";

// ─── State shape ─────────────────────────────────────────────────────────────

interface SessionStoreState {
  session: SessionInfo | null;
  transcript: TranscriptSegment[];
  partialSegment: TranscriptSegment | null;
  audioLevel: number;
  error: string | null;
  loading: boolean;
}

const INITIAL_STATE: SessionStoreState = {
  session: null,
  transcript: [],
  partialSegment: null,
  audioLevel: 0,
  error: null,
  loading: false,
};

// ─── Store ───────────────────────────────────────────────────────────────────

function createSessionStore() {
  const { subscribe, set, update } = writable<SessionStoreState>(INITIAL_STATE);

  let unlisteners: UnlistenFn[] = [];

  /** Register all Tauri event listeners. Called once on app mount. */
  async function initialize(): Promise<void> {
    const unlisten1 = await onSessionStateChanged((payload) => {
      update((s) => {
        if (!s.session || s.session.session_id !== payload.session_id)
          return s;
        return {
          ...s,
          session: { ...s.session, state: payload.new_state },
          error: null,
        };
      });
    });

    const unlisten2 = await onTranscriptSegment((payload) => {
      update((s) => {
        if (!s.session || s.session.session_id !== payload.session_id)
          return s;
        const seg = payload.segment;
        if (seg.is_partial) {
          return { ...s, partialSegment: seg };
        }
        return {
          ...s,
          partialSegment: null,
          transcript: [...s.transcript, seg],
        };
      });
    });

    const unlisten3 = await onError((payload) => {
      update((s) => ({
        ...s,
        error: `[${payload.code}] ${payload.message}`,
        loading: false,
      }));
    });

    const unlisten4 = await onAudioLevel((payload) => {
      update((s) => {
        if (!s.session || s.session.session_id !== payload.session_id)
          return s;
        return { ...s, audioLevel: payload.level };
      });
    });

    const unlisten5 = await onSessionCleanupDone((_sessionId) => {
      update((s) => ({ ...s, audioLevel: 0, partialSegment: null }));
    });

    unlisteners = [unlisten1, unlisten2, unlisten3, unlisten4, unlisten5];
  }

  /** Tear down all event listeners. */
  function destroy(): void {
    for (const fn of unlisteners) fn();
    unlisteners = [];
  }

  // ─── Session lifecycle actions ────────────────────────────────────────────

  const DEFAULT_CONFIG: SessionConfig = {
    audio_source: "Loopback",
    language_hint: "auto",
    enable_diarization: false,
    show_partial_results: true,
  };

  async function init(config: SessionConfig = DEFAULT_CONFIG): Promise<void> {
    update((s) => ({ ...s, loading: true, error: null }));
    try {
      const info = await initSession(config);
      update((s) => ({
        ...s,
        session: info,
        transcript: [],
        partialSegment: null,
        loading: false,
      }));
    } catch (err) {
      update((s) => ({ ...s, error: String(err), loading: false }));
    }
  }

  async function start(): Promise<void> {
    const { session } = get(sessionStore);
    if (!session) return;
    update((s) => ({ ...s, loading: true, error: null }));
    try {
      const info = await startSession(session.session_id);
      update((s) => ({ ...s, session: info, loading: false }));
    } catch (err) {
      update((s) => ({ ...s, error: String(err), loading: false }));
    }
  }

  async function pause(): Promise<void> {
    const { session } = get(sessionStore);
    if (!session) return;
    update((s) => ({ ...s, loading: true, error: null }));
    try {
      const info = await pauseSession(session.session_id);
      update((s) => ({ ...s, session: info, loading: false }));
    } catch (err) {
      update((s) => ({ ...s, error: String(err), loading: false }));
    }
  }

  async function resume(): Promise<void> {
    const { session } = get(sessionStore);
    if (!session) return;
    update((s) => ({ ...s, loading: true, error: null }));
    try {
      const info = await resumeSession(session.session_id);
      update((s) => ({ ...s, session: info, loading: false }));
    } catch (err) {
      update((s) => ({ ...s, error: String(err), loading: false }));
    }
  }

  async function stop(): Promise<void> {
    const { session } = get(sessionStore);
    if (!session) return;
    update((s) => ({ ...s, loading: true, error: null }));
    try {
      const info = await stopSession(session.session_id);
      update((s) => ({ ...s, session: info, loading: false }));
    } catch (err) {
      update((s) => ({ ...s, error: String(err), loading: false }));
    }
  }

  function clearError(): void {
    update((s) => ({ ...s, error: null }));
  }

  function reset(): void {
    set(INITIAL_STATE);
  }

  return {
    subscribe,
    initialize,
    destroy,
    init,
    start,
    pause,
    resume,
    stop,
    clearError,
    reset,
  };
}

export const sessionStore = createSessionStore();

// ─── Derived stores ───────────────────────────────────────────────────────────

export const sessionState = derived(
  sessionStore,
  ($s): SessionState => $s.session?.state ?? "Idle"
);

export const isRecording = derived(
  sessionStore,
  ($s): boolean => $s.session?.state === "Active"
);

export const isPaused = derived(
  sessionStore,
  ($s): boolean => $s.session?.state === "Paused"
);

export const canStart = derived(
  sessionStore,
  ($s): boolean =>
    $s.session?.state === "Initializing" || $s.session?.state === "Paused"
);

export const canPause = derived(
  sessionStore,
  ($s): boolean => $s.session?.state === "Active"
);

export const canStop = derived(
  sessionStore,
  ($s): boolean =>
    $s.session?.state === "Active" || $s.session?.state === "Paused"
);
