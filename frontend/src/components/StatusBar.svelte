<script lang="ts">
  import { sessionStatus, segments, errorMessage } from '../lib/sessionStore';

  const descriptions: Record<string, string> = {
    idle:     'No active session. Select a device and press Start.',
    starting: 'Initialising audio capture and transcription engine…',
    active:   'Transcription is running. Speak clearly into the meeting.',
    paused:   'Session paused. Press Resume to continue.',
    stopping: 'Stopping session and releasing resources…',
    stopped:  'Session complete. You can review, copy, or export the transcript.',
  };
</script>

<footer class="status-bar" aria-label="Application status">
  <span class="status-desc">{descriptions[$sessionStatus] ?? ''}</span>

  {#if $errorMessage}
    <span class="status-error" role="alert">⚠ {$errorMessage}</span>
  {:else}
    <span class="status-count">
      {$segments.length} segment{$segments.length !== 1 ? 's' : ''}
    </span>
  {/if}
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.3rem 1rem;
    font-size: 0.75rem;
    color: var(--color-text-muted);
    background: var(--color-surface);
    border-top: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .status-error {
    color: var(--color-danger);
    font-weight: 600;
  }

  .status-count {
    font-variant-numeric: tabular-nums;
  }
</style>
