<script lang="ts">
  import {
    sessionStatus,
    selectedDeviceId,
    audioDevices,
    errorMessage,
    meetingPlatform,
    sessionName,
    selectedLanguages,
  } from '../lib/sessionStore';
  import {
    handleStart,
    handlePause,
    handleResume,
    handleStop,
  } from '../lib/sessionController';

  const PLATFORMS = ['None', 'Google Meet', 'Zoom', 'Microsoft Teams', 'Other'];
  const LANGUAGE_OPTIONS = [
    { code: 'ta', label: 'Tamil (தமிழ்)' },
    { code: 'si', label: 'Sinhala (සිංහල)' },
    { code: 'en', label: 'English' },
  ] as const;

  $: isIdle   = $sessionStatus === 'idle' || $sessionStatus === 'stopped';
  $: isActive = $sessionStatus === 'active';
  $: isPaused = $sessionStatus === 'paused';
  $: isBusy   = $sessionStatus === 'starting' || $sessionStatus === 'stopping';
</script>

<section class="session-controls" aria-label="Transcription session controls">

  <div class="section-title">Meeting Setup</div>

  <div class="field-row">
    <label for="session-name" class="field-label">Session name</label>
    <input
      id="session-name"
      class="field-input"
      type="text"
      placeholder="e.g. Sprint Planning"
      bind:value={$sessionName}
      disabled={!isIdle}
      maxlength={80}
      aria-label="Session name"
    />
  </div>

  <div class="field-row">
    <label for="platform-select" class="field-label">Platform</label>
    <select
      id="platform-select"
      class="field-select"
      bind:value={$meetingPlatform}
      disabled={!isIdle}
      aria-label="Select meeting platform"
    >
      {#each PLATFORMS as p}
        <option value={p}>{p}</option>
      {/each}
    </select>
  </div>

  <div class="field-row">
    <label for="device-select" class="field-label">Audio device</label>
    <select
      id="device-select"
      class="field-select"
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

  <div class="field-row">
    <span class="field-label">Languages to transcribe</span>
    <div class="language-options" role="group" aria-label="Languages to transcribe">
      {#each LANGUAGE_OPTIONS as lang}
        <label class="language-option">
          <input
            type="checkbox"
            checked={$selectedLanguages.includes(lang.code)}
            disabled={!isIdle}
            on:change={(event) => {
              const checked = (event.currentTarget as HTMLInputElement).checked;
              selectedLanguages.update((current) =>
                checked
                  ? (current.includes(lang.code) ? current : [...current, lang.code])
                  : current.filter((code) => code !== lang.code)
              );
            }}
          />
          <span>{lang.label}</span>
        </label>
      {/each}
    </div>
    <p class="field-help">Select every language that may be spoken. VOXY keeps the original spoken text.</p>
  </div>

  <div class="divider"></div>
  <div class="section-title">Controls</div>

  <div class="button-row">
    <button
      class="btn btn-start"
      on:click={() => handleStart($selectedDeviceId)}
      disabled={!isIdle || isBusy || $selectedLanguages.length === 0}
      aria-label="Start transcription session"
    >▶ Start</button>

    {#if isActive}
      <button
        class="btn btn-pause"
        on:click={handlePause}
        disabled={isBusy}
        aria-label="Pause transcription session"
      >⏸ Pause</button>
    {/if}

    {#if isPaused}
      <button
        class="btn btn-resume"
        on:click={handleResume}
        disabled={isBusy}
        aria-label="Resume transcription session"
      >▶ Resume</button>
    {/if}

    <button
      class="btn btn-stop"
      on:click={handleStop}
      disabled={isIdle || isBusy}
      aria-label="Stop transcription session"
    >■ Stop</button>
  </div>

  {#if $errorMessage}
    <p class="error-msg" role="alert">{$errorMessage}</p>
  {/if}

</section>

<style>
  .session-controls {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    padding: 1rem;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
  }

  .section-title {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-muted);
    margin-bottom: 0.1rem;
  }

  .divider {
    height: 1px;
    background: var(--color-border);
    margin: 0.25rem 0;
  }

  .field-row {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .field-label {
    font-size: 0.75rem;
    color: var(--color-text-muted);
  }

  .field-input,
  .field-select {
    width: 100%;
    padding: 0.35rem 0.6rem;
    background: var(--color-input-bg);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .language-options {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem 0.9rem;
  }

  .language-option {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.82rem;
    color: var(--color-text);
    cursor: pointer;
  }

  .language-option input { accent-color: var(--color-primary); }

  .field-help {
    margin: 0;
    font-size: 0.7rem;
    color: var(--color-text-muted);
    line-height: 1.4;
  }

  .field-input::placeholder { color: var(--color-text-muted); }
  .field-input:disabled,
  .field-select:disabled { opacity: 0.5; cursor: not-allowed; }

  .button-row {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .btn {
    padding: 0.45rem 1rem;
    border: none;
    border-radius: 6px;
    font-size: 0.85rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s, background 0.15s;
  }

  .btn:disabled { opacity: 0.4; cursor: not-allowed; }
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
