<script lang="ts">
  import {
    sessionStore,
    sessionState,
    isRecording,
    isPaused,
    canStart,
    canPause,
    canStop,
  } from "../stores/session";
  import TranscriptPanel from "./TranscriptPanel.svelte";
  import AudioMeter from "./AudioMeter.svelte";
  import type { SessionConfig } from "$lib/types";

  let configuring = false;

  const defaultConfig: SessionConfig = {
    audio_source: "Loopback",
    language_hint: "auto",
    enable_diarization: false,
    show_partial_results: true,
  };

  let config: SessionConfig = { ...defaultConfig };

  async function handleInit() {
    await sessionStore.init(config);
    configuring = false;
  }

  async function handleStart() {
    if (!$sessionStore.session) {
      await sessionStore.init(config);
    }
    await sessionStore.start();
  }

  async function handlePause() {
    await sessionStore.pause();
  }

  async function handleResume() {
    await sessionStore.resume();
  }

  async function handleStop() {
    await sessionStore.stop();
  }

  function handleNewSession() {
    sessionStore.reset();
  }
</script>

<div class="session-view">
  <!-- Controls -->
  <div class="controls-bar">
    {#if $sessionState === "Idle"}
      <button class="btn btn-primary" on:click={handleInit}>
        New Session
      </button>
      <button
        class="btn btn-ghost"
        on:click={() => (configuring = !configuring)}
      >
        ⚙ Configure
      </button>
    {:else if $sessionState === "Initializing"}
      <button
        class="btn btn-success"
        on:click={sessionStore.start}
        disabled={$sessionStore.loading}
      >
        ▶ Start Recording
      </button>
      <button
        class="btn btn-ghost"
        on:click={handleNewSession}
        disabled={$sessionStore.loading}
      >
        Cancel
      </button>
    {:else if $sessionState === "Active"}
      <button class="btn btn-warning" on:click={handlePause}>
        ⏸ Pause
      </button>
      <button class="btn btn-danger" on:click={handleStop}>
        ⏹ Stop
      </button>
    {:else if $sessionState === "Paused"}
      <button class="btn btn-success" on:click={handleResume}>
        ▶ Resume
      </button>
      <button class="btn btn-danger" on:click={handleStop}>
        ⏹ Stop
      </button>
    {:else if $sessionState === "Stopped"}
      <button class="btn btn-primary" on:click={handleNewSession}>
        + New Session
      </button>
    {:else if $sessionState === "Error"}
      <button class="btn btn-primary" on:click={handleNewSession}>
        Retry
      </button>
    {/if}

    {#if $isRecording || $isPaused}
      <AudioMeter />
    {/if}
  </div>

  <!-- Configuration panel -->
  {#if configuring && $sessionState === "Idle"}
    <div class="config-panel">
      <h3>Session Configuration</h3>
      <label class="field">
        <span>Audio Source</span>
        <select bind:value={config.audio_source}>
          <option value="Loopback">System Audio (Loopback)</option>
          <option value="Microphone">Microphone</option>
          <option value="Both">Both</option>
        </select>
      </label>
      <label class="field">
        <span>Language</span>
        <select bind:value={config.language_hint}>
          <option value="auto">Auto-detect</option>
          <option value="si">Sinhala</option>
          <option value="ta">Tamil</option>
          <option value="en">English</option>
        </select>
      </label>
      <label class="field checkbox">
        <input type="checkbox" bind:checked={config.enable_diarization} />
        <span>Enable Speaker Diarization (experimental)</span>
      </label>
      <label class="field checkbox">
        <input type="checkbox" bind:checked={config.show_partial_results} />
        <span>Show partial results</span>
      </label>
    </div>
  {/if}

  <!-- Error banner -->
  {#if $sessionStore.error}
    <div class="error-banner" role="alert">
      <span>⚠ {$sessionStore.error}</span>
      <button class="btn-close" on:click={sessionStore.clearError}>✕</button>
    </div>
  {/if}

  <!-- Transcript area -->
  <div class="transcript-area">
    <TranscriptPanel />
  </div>
</div>

<style>
  .session-view {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .controls-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 20px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .btn {
    padding: 8px 18px;
    border-radius: var(--radius-md);
    font-size: 13px;
    font-weight: 600;
    border: none;
    transition: background 0.15s, opacity 0.15s;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-primary {
    background: var(--color-accent);
    color: #fff;
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--color-accent-hover);
  }

  .btn-success {
    background: var(--color-success);
    color: #fff;
  }

  .btn-warning {
    background: var(--color-warning);
    color: #000;
  }

  .btn-danger {
    background: var(--color-error);
    color: #fff;
  }

  .btn-ghost {
    background: transparent;
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
  }

  .btn-ghost:hover {
    color: var(--color-text-primary);
    border-color: var(--color-accent);
  }

  .config-panel {
    padding: 16px 20px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .config-panel h3 {
    font-size: 13px;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--color-text-primary);
  }

  .field span:first-child {
    min-width: 120px;
    color: var(--color-text-secondary);
  }

  .field select {
    background: var(--color-bg);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    font-size: 13px;
    font-family: inherit;
  }

  .field.checkbox {
    gap: 8px;
  }

  .error-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    background: rgba(239, 83, 80, 0.15);
    border-bottom: 1px solid var(--color-error);
    color: var(--color-error);
    font-size: 13px;
  }

  .btn-close {
    background: none;
    border: none;
    color: inherit;
    font-size: 14px;
    cursor: pointer;
    padding: 2px 6px;
  }

  .transcript-area {
    flex: 1;
    overflow: hidden;
  }
</style>
