<script lang="ts">
  import { onDestroy } from 'svelte';
  import {
    sessionStatus,
    sessionStartTime,
    segments,
    languageBreakdown,
    meetingPlatform,
    sessionName,
    audioDevices,
    selectedDeviceId,
  } from '../lib/sessionStore';
  import { formatTime } from '../lib/utils';

  // ── Elapsed time ticker ───────────────────────────────────────────────────

  let elapsed = 0;
  let ticker: ReturnType<typeof setInterval> | null = null;

  const unsubStatus = sessionStatus.subscribe((s) => {
    if (s === 'active') {
      ticker = setInterval(() => {
        const start = $sessionStartTime;
        elapsed = start ? Math.floor((Date.now() - start) / 1000) : 0;
      }, 1000);
    } else {
      if (ticker) { clearInterval(ticker); ticker = null; }
      if (s === 'idle' || s === 'stopped') elapsed = 0;
    }
  });

  onDestroy(() => {
    unsubStatus();
    if (ticker) clearInterval(ticker);
  });

  // ── Derived display values ────────────────────────────────────────────────

  $: deviceName = $audioDevices.find((d) => d.id === $selectedDeviceId)?.name ?? $selectedDeviceId;

  const langLabels: Record<string, string> = {
    en: 'English', si: 'Sinhala', ta: 'Tamil', und: 'Unknown',
  };
</script>

<div class="session-info" aria-label="Session information">
  <div class="info-title">Session Info</div>

  <dl class="info-list">

    {#if $sessionName}
      <div class="info-row">
        <dt>Name</dt>
        <dd>{$sessionName}</dd>
      </div>
    {/if}

    <div class="info-row">
      <dt>Platform</dt>
      <dd>{$meetingPlatform}</dd>
    </div>

    <div class="info-row">
      <dt>Device</dt>
      <dd class="truncate" title={deviceName}>{deviceName}</dd>
    </div>

    <div class="info-row">
      <dt>Status</dt>
      <dd class="status-{$sessionStatus}">{$sessionStatus}</dd>
    </div>

    <div class="info-row">
      <dt>Elapsed</dt>
      <dd>{formatTime(elapsed)}</dd>
    </div>

    <div class="info-row">
      <dt>Segments</dt>
      <dd>{$segments.length}</dd>
    </div>

  </dl>

  {#if $segments.length > 0}
    <div class="lang-breakdown" aria-label="Language breakdown">
      <span class="info-title">Languages</span>
      <div class="lang-pills">
        {#each Object.entries($languageBreakdown) as [code, count]}
          {#if count > 0}
            <span class="lang-chip lang-{code}">
              {langLabels[code] ?? code}: {count}
            </span>
          {/if}
        {/each}
      </div>
    </div>
  {/if}

</div>

<style>
  .session-info {
    padding: 0.9rem 1rem;
    border-bottom: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .info-title {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-muted);
  }

  .info-list {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .info-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.5rem;
  }

  dt {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  dd {
    font-size: 0.8rem;
    color: var(--color-text);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 140px;
  }

  /* Status colour coding */
  .status-active   { color: var(--color-active-fg);  font-weight: 600; }
  .status-paused   { color: var(--color-warn-fg);     font-weight: 600; }
  .status-starting,
  .status-stopping { color: var(--color-warn-fg); }
  .status-idle,
  .status-stopped  { color: var(--color-text-muted); }

  .lang-breakdown {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .lang-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }

  .lang-chip {
    font-size: 0.68rem;
    padding: 0.15rem 0.5rem;
    border-radius: 999px;
    background: var(--color-idle-bg);
    color: var(--color-text-muted);
  }

  .lang-chip.lang-si { background: #1a2f3a; color: #63b3ed; }
  .lang-chip.lang-ta { background: #2a1f3a; color: #b794f4; }
  .lang-chip.lang-en { background: #1a3a2a; color: #68d391; }
</style>
