<script lang="ts">
  import { sessionStatus } from '../lib/sessionStore';

  const statusLabels: Record<string, string> = {
    idle: 'Ready',
    starting: 'Starting…',
    active: 'Live',
    paused: 'Paused',
    stopping: 'Stopping…',
    stopped: 'Stopped',
  };
</script>

<header class="app-header">
  <div class="header-brand">
    <span class="brand-logo" aria-hidden="true">🎙</span>
    <span class="brand-name">VOXY</span>
    <span class="brand-tagline">Every voice matters.</span>
  </div>

  <div class="header-status">
    <span
      class="status-badge status-{$sessionStatus}"
      role="status"
      aria-live="polite"
      aria-label="Session status: {statusLabels[$sessionStatus]}"
    >
      {statusLabels[$sessionStatus] ?? $sessionStatus}
    </span>
  </div>
</header>

<style>
  .app-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1.5rem;
    height: 56px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .header-brand {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .brand-logo {
    font-size: 1.4rem;
    line-height: 1;
  }

  .brand-name {
    font-size: 1.2rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--color-primary);
  }

  .brand-tagline {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    padding-left: 0.25rem;
  }

  .status-badge {
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.25rem 0.75rem;
    border-radius: 999px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .status-idle    { background: var(--color-idle-bg);    color: var(--color-idle-fg); }
  .status-starting{ background: var(--color-warn-bg);    color: var(--color-warn-fg); }
  .status-active  { background: var(--color-active-bg);  color: var(--color-active-fg); }
  .status-paused  { background: var(--color-paused-bg);  color: var(--color-paused-fg); }
  .status-stopping{ background: var(--color-warn-bg);    color: var(--color-warn-fg); }
  .status-stopped { background: var(--color-idle-bg);    color: var(--color-idle-fg); }
</style>
