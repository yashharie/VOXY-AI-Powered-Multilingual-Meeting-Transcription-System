<script lang="ts">
  import {
    sessionStatus,
    selectedDeviceId,
    audioDevices,
    errorMessage,
    clearSession,
  } from '../lib/sessionStore';
  import {
    startSession,
    pauseSession,
    resumeSession,
    stopSession,
    listAudioDevices,
    onTranscriptSegment,
    onSessionStatusChange,
  } from '../lib/tauri';
  import { appendSegment } from '../lib/sessionStore';
  import { nextMockSegment, resetMockSegments } from '../lib/mockData';
  import type { SessionStatus } from '../types';
  import { onMount, onDestroy } from 'svelte';

  let unlistenTranscript: (() => void) | null = null;
  let unlistenStatus: (() => void) | null = null;
  let mockTimer: ReturnType<typeof setInterval> | null = null;

  onMount(async () => {
    const devices = await listAudioDevices();
    audioDevices.set(devices);
    if (devices.length > 0) {
      selectedDeviceId.set(devices.find((d) => d.is_default)?.id ?? devices[0].id);
    }

    // Listen for real Tauri status events
    unlistenStatus = await onSessionStatusChange((status: SessionStatus) => {
      sessionStatus.set(status);
    });
  });

  onDestroy(() => {
    unlistenTranscript?.();
    unlistenStatus?.();
    if (mockTimer) clearInterval(mockTimer);
  });

  async function handleStart() {
    errorMessage.set(null);
    clearSession();
    resetMockSegments();
    sessionStatus.set('starting');

    try {
      await startSession($selectedDeviceId);

      // Subscribe to real transcript events
      unlistenTranscript = await onTranscriptSegment((segment) => {
        appendSegment(segment);
      });

      sessionStatus.set('active');

      // Start mock feed when not in Tauri (browser dev mode)
      if (!('__TAURI__' in window)) {
        mockTimer = setInterval(() => {
          if ($sessionStatus === 'active') {
            appendSegment(nextMockSegment());
          }
        }, 2500);
      }
    } catch (err) {
      errorMessage.set(String(err));
      sessionStatus.set('idle');
    }
  }

  async function handlePause() {
    try {
      await pauseSession();
      sessionStatus.set('paused');
      if (mockTimer) { clearInterval(mockTimer); mockTimer = null; }
    } catch (err) {
      errorMessage.set(String(err));
    }
  }

  async function handleResume() {
    try {
      await resumeSession();
      sessionStatus.set('active');
      if (!('__TAURI__' in window)) {
        mockTimer = setInterval(() => {
          if ($sessionStatus === 'active') appendSegment(nextMockSegment());
        }, 2500);
      }
    } catch (err) {
      errorMessage.set(String(err));
    }
  }

  async function handleStop() {
    sessionStatus.set('stopping');
    try {
      await stopSession();
      if (mockTimer) { clearInterval(mockTimer); mockTimer = null; }
      unlistenTranscript?.();
      unlistenTranscript = null;
      sessionStatus.set('stopped');
    } catch (err) {
      errorMessage.set(String(err));
      sessionStatus.set('stopped');
    }
  }

  $: isIdle    = $sessionStatus === 'idle'    || $sessionStatus === 'stopped';
  $: isActive  = $sessionStatus === 'active';
  $: isPaused  = $sessionStatus === 'paused';
  $: isBusy    = $sessionStatus === 'starting' || $sessionStatus === 'stopping';
</script>

<section class="session-controls" aria-label="Transcription session controls">
  <div class="device-row">
    <label for="device-select" class="device-label">Audio device</label>
    <select
      id="device-select"
      class="device-select"
      bind:value={$selectedDeviceId}
      disabled={!isIdle}
      aria-label="Select audio output device"
    >
      {#each $audioDevices as device (device.id)}
        <option value={device.id}>
          {device.name}{device.is_default ? ' (default)' : ''}
        </option>
      {/each}
    </select>
  </div>

  <div class="button-row">
    <button
      class="btn btn-start"
      on:click={handleStart}
      disabled={!isIdle || isBusy}
      aria-label="Start transcription session"
    >
      ▶ Start
    </button>

    {#if isActive}
      <button
        class="btn btn-pause"
        on:click={handlePause}
        disabled={isBusy}
        aria-label="Pause transcription session"
      >
        ⏸ Pause
      </button>
    {/if}

    {#if isPaused}
      <button
        class="btn btn-resume"
        on:click={handleResume}
        disabled={isBusy}
        aria-label="Resume transcription session"
      >
        ▶ Resume
      </button>
    {/if}

    <button
      class="btn btn-stop"
      on:click={handleStop}
      disabled={isIdle || isBusy}
      aria-label="Stop transcription session"
    >
      ■ Stop
    </button>
  </div>

  {#if $errorMessage}
    <p class="error-msg" role="alert">{$errorMessage}</p>
  {/if}
</section>

<style>
  .session-controls {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 1rem 1.25rem;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
  }

  .device-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .device-label {
    font-size: 0.8rem;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .device-select {
    flex: 1;
    max-width: 320px;
    padding: 0.35rem 0.6rem;
    background: var(--color-input-bg);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .device-select:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .button-row {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .btn {
    padding: 0.45rem 1.1rem;
    border: none;
    border-radius: 6px;
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s, background 0.15s;
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .btn-start  { background: var(--color-primary); color: #fff; }
  .btn-pause  { background: var(--color-warn-bg); color: var(--color-warn-fg); }
  .btn-resume { background: var(--color-primary); color: #fff; }
  .btn-stop   { background: var(--color-danger);  color: #fff; }

  .error-msg {
    font-size: 0.8rem;
    color: var(--color-danger);
    margin: 0;
  }
</style>
