<script lang="ts">
  import {
    sessionStatus,
    audioDevices,
    selectedDeviceId,
    errorMessage,
  } from '../lib/sessionStore';

  $: deviceName = $audioDevices.find((d) => d.id === $selectedDeviceId)?.name ?? $selectedDeviceId;

  $: audioCapture   = $sessionStatus === 'active' || $sessionStatus === 'paused' ? 'Active' : 'Inactive';
  $: transcription  = $sessionStatus === 'active' ? 'Running' : $sessionStatus === 'paused' ? 'Paused' : 'Stopped';

  // These will be provided by the Tauri backend once integrated.
  // Until then they are explicitly marked as unavailable.
  const NOT_AVAILABLE = 'Not available';
</script>

<div class="diag-page" aria-label="Diagnostics">

  <section class="diag-section">
    <h2 class="section-heading">System Status</h2>

    <dl class="diag-list">

      <div class="diag-row">
        <dt>Audio capture</dt>
        <dd class:active={audioCapture === 'Active'} class:inactive={audioCapture === 'Inactive'}>
          {audioCapture}
        </dd>
      </div>

      <div class="diag-row">
        <dt>Selected audio device</dt>
        <dd class="truncate" title={deviceName}>{deviceName}</dd>
      </div>

      <div class="diag-row">
        <dt>Microphone</dt>
        <dd class="unavailable">Not configured</dd>
      </div>

      <div class="diag-row">
        <dt>AI model</dt>
        <dd class="unavailable">Not loaded — backend not yet integrated</dd>
      </div>

      <div class="diag-row">
        <dt>Transcription</dt>
        <dd class:active={transcription === 'Running'} class:paused={transcription === 'Paused'} class:inactive={transcription === 'Stopped'}>
          {transcription}
        </dd>
      </div>

    </dl>
  </section>

  <section class="diag-section">
    <h2 class="section-heading">Performance</h2>
    <p class="diag-note">
      Performance metrics are collected by the Rust backend during a live session.
      Values below will be populated once the backend is integrated.
      Simulated values are not shown here.
    </p>

    <dl class="diag-list">

      <div class="diag-row">
        <dt>Processing latency</dt>
        <dd class="unavailable">{NOT_AVAILABLE}</dd>
      </div>

      <div class="diag-row">
        <dt>CPU usage</dt>
        <dd class="unavailable">{NOT_AVAILABLE}</dd>
      </div>

      <div class="diag-row">
        <dt>RAM usage</dt>
        <dd class="unavailable">{NOT_AVAILABLE}</dd>
      </div>

    </dl>
  </section>

  <section class="diag-section">
    <h2 class="section-heading">Errors</h2>

    {#if $errorMessage}
      <div class="error-entry" role="alert">
        <span class="error-icon">⚠</span>
        <span class="error-text">{$errorMessage}</span>
      </div>
    {:else}
      <p class="no-errors">No errors recorded in this session.</p>
    {/if}
  </section>

</div>

<style>
  .diag-page {
    flex: 1;
    overflow-y: auto;
    padding: 1.25rem 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  .diag-section {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
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

  .diag-note {
    font-size: 0.78rem;
    color: var(--color-text-muted);
    line-height: 1.5;
    margin: 0;
  }

  .diag-list {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .diag-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 1rem;
    padding: 0.3rem 0;
    border-bottom: 1px solid var(--color-border);
  }

  dt {
    font-size: 0.8rem;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  dd {
    font-size: 0.82rem;
    color: var(--color-text);
    text-align: right;
  }

  dd.active   { color: var(--color-active-fg);  font-weight: 600; }
  dd.paused   { color: var(--color-warn-fg);     font-weight: 600; }
  dd.inactive { color: var(--color-text-muted); }
  dd.unavailable { color: var(--color-text-muted); font-style: italic; }

  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 240px;
  }

  .error-entry {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    padding: 0.65rem 0.9rem;
    background: #2d1a1a;
    border: 1px solid #5a2020;
    border-radius: 6px;
    font-size: 0.82rem;
  }

  .error-icon { color: var(--color-danger); font-size: 1rem; flex-shrink: 0; }
  .error-text { color: var(--color-danger); line-height: 1.5; }

  .no-errors {
    font-size: 0.82rem;
    color: var(--color-text-muted);
    margin: 0;
  }
</style>
