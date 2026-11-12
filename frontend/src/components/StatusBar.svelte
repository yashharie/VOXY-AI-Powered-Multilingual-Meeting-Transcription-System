<script lang="ts">
  import { sessionStore, sessionState, isRecording } from "../stores/session";
  import type { SessionState } from "$lib/types";

  const STATE_LABELS: Record<SessionState, string> = {
    Idle: "Idle",
    Initializing: "Initialising…",
    Active: "Recording",
    Paused: "Paused",
    Stopping: "Stopping…",
    Stopped: "Stopped",
    Error: "Error",
  };

  const STATE_COLORS: Record<SessionState, string> = {
    Idle: "var(--color-text-secondary)",
    Initializing: "var(--color-warning)",
    Active: "var(--color-recording)",
    Paused: "var(--color-warning)",
    Stopping: "var(--color-warning)",
    Stopped: "var(--color-success)",
    Error: "var(--color-error)",
  };
</script>

<div class="status-bar">
  <div class="indicator" style:background-color={STATE_COLORS[$sessionState]}>
    {#if $isRecording}
      <span class="pulse"></span>
    {/if}
  </div>
  <span class="label">{STATE_LABELS[$sessionState]}</span>
  {#if $sessionStore.session}
    <span class="session-id">
      #{$sessionStore.session.session_id.slice(0, 8)}
    </span>
  {/if}
</div>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .indicator {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    position: relative;
    flex-shrink: 0;
  }

  .pulse::before {
    content: "";
    position: absolute;
    inset: -4px;
    border-radius: 50%;
    background: var(--color-recording);
    opacity: 0.3;
    animation: pulse 1.5s ease-out infinite;
  }

  @keyframes pulse {
    0% {
      transform: scale(1);
      opacity: 0.4;
    }
    100% {
      transform: scale(2.5);
      opacity: 0;
    }
  }

  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-primary);
  }

  .session-id {
    font-size: 10px;
    color: var(--color-text-secondary);
    font-family: var(--font-mono);
  }
</style>
