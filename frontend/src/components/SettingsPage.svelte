<script lang="ts">
  import {
    audioDevices,
    selectedDeviceId,
    sessionStatus,
  } from '../lib/sessionStore';
  import { listAudioDevices } from '../lib/tauri';
  import { onMount } from 'svelte';

  // ── Local settings state ─────────────────────────────────────────────────

  // Microphone (populated from device list; no capture yet — backend pending)
  let micDeviceId = 'none';
  let micDevices: Array<{ id: string; name: string }> = [
    { id: 'none', name: 'None (not configured)' },
  ];

  // Language preferences
  let preferredLanguages: string[] = ['en', 'si', 'ta'];
  const LANGUAGE_OPTIONS = [
    { code: 'en', label: 'English' },
    { code: 'si', label: 'Sinhala' },
    { code: 'ta', label: 'Tamil' },
  ];

  // Transcription preferences
  let autoScrollEnabled = true;
  let showLanguageLabels = true;
  let showSpeakerLabels  = true;
  let segmentIntervalMs  = 2500;

  $: isSessionActive = $sessionStatus === 'active' || $sessionStatus === 'paused';

  onMount(async () => {
    const devices = await listAudioDevices();
    audioDevices.set(devices);
  });

  function toggleLanguage(code: string) {
    if (preferredLanguages.includes(code)) {
      if (preferredLanguages.length > 1) {
        preferredLanguages = preferredLanguages.filter((c) => c !== code);
      }
    } else {
      preferredLanguages = [...preferredLanguages, code];
    }
  }
</script>

<div class="settings-page" aria-label="Application settings">

  <!-- ── Audio device ──────────────────────────────────────────────── -->
  <section class="settings-section">
    <h2 class="section-heading">Audio</h2>

    <div class="field">
      <label for="settings-device" class="field-label">
        Output device (system audio capture)
      </label>
      <select
        id="settings-device"
        class="field-select"
        bind:value={$selectedDeviceId}
        disabled={isSessionActive}
        aria-describedby="device-hint"
      >
        {#each $audioDevices as device (device.id)}
          <option value={device.id}>
            {device.name}{device.is_default ? ' (default)' : ''}
          </option>
        {/each}
      </select>
      {#if isSessionActive}
        <span id="device-hint" class="field-hint">Stop the session to change the audio device.</span>
      {:else}
        <span id="device-hint" class="field-hint">WASAPI loopback capture. Changes take effect at the next session start.</span>
      {/if}
    </div>

    <div class="field">
      <label for="settings-mic" class="field-label">
        Microphone (optional)
      </label>
      <select
        id="settings-mic"
        class="field-select"
        bind:value={micDeviceId}
        disabled={isSessionActive}
        aria-describedby="mic-hint"
      >
        {#each micDevices as d (d.id)}
          <option value={d.id}>{d.name}</option>
        {/each}
      </select>
      <span id="mic-hint" class="field-hint">
        Microphone capture requires backend support. Not available until the audio engine is integrated.
      </span>
    </div>
  </section>

  <!-- ── Language preferences ─────────────────────────────────────── -->
  <section class="settings-section">
    <h2 class="section-heading">Languages</h2>
    <p class="section-desc">Select the languages expected in this session. VOXY will transcribe all supported languages regardless of this setting, but this helps inform future model selection.</p>

    <div class="checkbox-group" role="group" aria-label="Language selection">
      {#each LANGUAGE_OPTIONS as lang}
        <label class="checkbox-label">
          <input
            type="checkbox"
            checked={preferredLanguages.includes(lang.code)}
            on:change={() => toggleLanguage(lang.code)}
          />
          {lang.label}
        </label>
      {/each}
    </div>
  </section>

  <!-- ── Transcription preferences ────────────────────────────────── -->
  <section class="settings-section">
    <h2 class="section-heading">Transcription</h2>

    <div class="field">
      <label class="toggle-label">
        <input type="checkbox" bind:checked={autoScrollEnabled} />
        Auto-scroll transcript to latest segment
      </label>
    </div>

    <div class="field">
      <label class="toggle-label">
        <input type="checkbox" bind:checked={showSpeakerLabels} />
        Show speaker labels
      </label>
    </div>

    <div class="field">
      <label class="toggle-label">
        <input type="checkbox" bind:checked={showLanguageLabels} />
        Show language labels on segments
      </label>
    </div>

    <div class="field">
      <label for="segment-interval" class="field-label">
        Mock segment interval (ms) — development only
      </label>
      <input
        id="segment-interval"
        class="field-input"
        type="number"
        min={500}
        max={10000}
        step={500}
        bind:value={segmentIntervalMs}
        aria-describedby="interval-hint"
      />
      <span id="interval-hint" class="field-hint">
        Controls how often mock segments appear during browser development. Has no effect when connected to the Tauri backend.
      </span>
    </div>
  </section>

  <!-- ── Application information ───────────────────────────────────── -->
  <section class="settings-section">
    <h2 class="section-heading">About</h2>
    <dl class="about-list">
      <div class="about-row"><dt>Application</dt><dd>VOXY</dd></div>
      <div class="about-row"><dt>Full name</dt><dd>AI-Powered Multilingual Meeting Transcription System</dd></div>
      <div class="about-row"><dt>Team</dt><dd>Tesseract</dd></div>
      <div class="about-row"><dt>Frontend</dt><dd>Svelte + TypeScript + Vite</dd></div>
      <div class="about-row"><dt>Desktop shell</dt><dd>Tauri 2</dd></div>
      <div class="about-row"><dt>Status</dt><dd>Development — backend not yet integrated</dd></div>
    </dl>
  </section>

</div>

<style>
  .settings-page {
    flex: 1;
    overflow-y: auto;
    padding: 1.25rem 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .section-heading {
    font-size: 0.75rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-muted);
    padding-bottom: 0.4rem;
    border-bottom: 1px solid var(--color-border);
    margin: 0;
  }

  .section-desc {
    font-size: 0.8rem;
    color: var(--color-text-muted);
    line-height: 1.5;
    margin: 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .field-label {
    font-size: 0.8rem;
    color: var(--color-text);
  }

  .field-hint {
    font-size: 0.72rem;
    color: var(--color-text-muted);
    line-height: 1.4;
  }

  .field-select,
  .field-input {
    padding: 0.4rem 0.65rem;
    background: var(--color-input-bg);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    font-size: 0.85rem;
    max-width: 400px;
  }

  .field-select:disabled,
  .field-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .checkbox-group {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .checkbox-label,
  .toggle-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.85rem;
    color: var(--color-text);
    cursor: pointer;
    user-select: none;
  }

  input[type='checkbox'] {
    width: 15px;
    height: 15px;
    accent-color: var(--color-primary);
    cursor: pointer;
  }

  .about-list {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .about-row {
    display: flex;
    gap: 1rem;
    font-size: 0.82rem;
  }

  dt {
    color: var(--color-text-muted);
    min-width: 120px;
    flex-shrink: 0;
  }

  dd {
    color: var(--color-text);
  }
</style>
