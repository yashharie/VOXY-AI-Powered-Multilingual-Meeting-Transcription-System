<script lang="ts">
  import { afterUpdate } from "svelte";
  import type { SessionState, TranscriptSegment } from "$lib/types";

  export let segments: TranscriptSegment[] = [];
  export let partial: TranscriptSegment | null = null;
  export let state: SessionState = "Idle";

  let scrollEl: HTMLDivElement;
  let autoScroll = true;

  afterUpdate(() => {
    if (autoScroll && scrollEl) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  });

  function formatTime(seconds: number): string {
    const m = Math.floor(seconds / 60)
      .toString()
      .padStart(2, "0");
    const s = Math.floor(seconds % 60)
      .toString()
      .padStart(2, "0");
    return `${m}:${s}`;
  }
</script>

<div class="transcript-panel">
  {#if segments.length === 0 && !partial}
    <div class="empty-state">
      {#if state === "Idle" || state === "Stopped"}
        <p class="empty-icon">📝</p>
        <p>No transcript yet. Start a session to begin.</p>
      {:else if state === "Initializing"}
        <p class="empty-icon">⏳</p>
        <p>Session ready. Press Start Recording to begin.</p>
      {:else if state === "Active"}
        <p class="empty-icon">🎙</p>
        <p>Listening… transcript will appear here.</p>
      {:else if state === "Paused"}
        <p class="empty-icon">⏸</p>
        <p>Recording paused.</p>
      {/if}
    </div>
  {:else}
    <div
      class="segments-container"
      bind:this={scrollEl}
      on:scroll={() => {
        const atBottom =
          scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight <
          40;
        autoScroll = atBottom;
      }}
    >
      {#each segments as seg (seg.index)}
        <div class="segment" class:partial={seg.is_partial}>
          <div class="segment-meta">
            <span class="time">{formatTime(seg.start_time)}</span>
            {#if seg.speaker}
              <span class="speaker">{seg.speaker}</span>
            {/if}
            <span class="lang">{seg.language}</span>
          </div>
          <p class="segment-text">{seg.text}</p>
        </div>
      {/each}

      {#if partial}
        <div class="segment partial live">
          <div class="segment-meta">
            <span class="time">{formatTime(partial.start_time)}</span>
            {#if partial.speaker}
              <span class="speaker">{partial.speaker}</span>
            {/if}
            <span class="lang">{partial.language}</span>
            <span class="live-badge">LIVE</span>
          </div>
          <p class="segment-text">{partial.text}<span class="cursor">|</span></p>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .transcript-panel {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 10px;
    color: var(--color-text-secondary);
    font-size: 14px;
  }

  .empty-icon {
    font-size: 36px;
  }

  .segments-container {
    flex: 1;
    overflow-y: auto;
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .segment {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: 10px 14px;
    transition: border-color 0.2s;
  }

  .segment:hover {
    border-color: var(--color-accent);
  }

  .segment.partial {
    border-color: var(--color-warning);
    opacity: 0.85;
  }

  .segment.live {
    animation: fadein 0.3s ease-in;
  }

  @keyframes fadein {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 0.85;
    }
  }

  .segment-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .time {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-text-secondary);
  }

  .speaker {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-accent);
    background: rgba(108, 99, 255, 0.12);
    padding: 1px 6px;
    border-radius: 10px;
  }

  .lang {
    font-size: 10px;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .live-badge {
    font-size: 9px;
    font-weight: 700;
    color: var(--color-recording);
    background: rgba(239, 83, 80, 0.15);
    padding: 1px 5px;
    border-radius: 3px;
    letter-spacing: 1px;
    animation: blink 1s step-start infinite;
  }

  @keyframes blink {
    50% {
      opacity: 0.4;
    }
  }

  .segment-text {
    font-size: 14px;
    line-height: 1.6;
    color: var(--color-text-primary);
    user-select: text;
  }

  .cursor {
    animation: blink 0.8s step-start infinite;
    color: var(--color-accent);
    font-weight: 100;
  }
</style>
