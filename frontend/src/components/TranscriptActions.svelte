<script lang="ts">
  import { onDestroy } from 'svelte';
  import { segments, sessionInfo, errorMessage } from '../lib/sessionStore';
  import { copyTranscript, exportTranscript } from '../lib/transcriptActions';
  let copyFeedback = '';
  let exporting = false;
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  onDestroy(() => { if (copyTimer) clearTimeout(copyTimer); });
  async function handleCopy() {
    if (!$sessionInfo || !$segments.length) return;
    try { await copyTranscript($sessionInfo.session_id); copyFeedback = 'Copied!'; }
    catch (error) { copyFeedback = 'Copy failed'; errorMessage.set(String(error)); }
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copyFeedback = ''; }, 2000);
  }
  async function handleExport() {
    if (!$sessionInfo || !$segments.length || exporting) return;
    exporting = true;
    try { await exportTranscript($sessionInfo.session_id); }
    catch (error) { errorMessage.set(String(error)); }
    finally { exporting = false; }
  }
</script>

<div class="transcript-actions" role="toolbar" aria-label="Transcript actions">
  <button
    class="action-btn"
    on:click={handleCopy}
    disabled={$segments.length === 0}
    aria-label="Copy full transcript to clipboard"
    title="Copy transcript"
  >
    📋 Copy
    {#if copyFeedback}
      <span class="feedback" aria-live="polite">{copyFeedback}</span>
    {/if}
  </button>

  <button
    class="action-btn"
    on:click={handleExport}
    disabled={$segments.length === 0}
    aria-label="Export transcript as text file"
    title="Export transcript as .txt"
  >
    💾 Export .txt
  </button>
</div>

<style>
  .transcript-actions {
    display: flex;
    gap: 0.5rem;
    padding: 0.6rem 1rem;
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
    flex-shrink: 0;
  }

  .action-btn {
    padding: 0.4rem 0.9rem;
    font-size: 0.82rem;
    font-weight: 600;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-card);
    color: var(--color-text);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    transition: background 0.15s;
  }

  .action-btn:hover:not(:disabled) {
    background: var(--color-idle-bg);
  }

  .action-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .feedback {
    font-size: 0.7rem;
    color: var(--color-primary);
    font-weight: 400;
  }
</style>
