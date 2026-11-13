<script lang="ts">
  import { afterUpdate } from 'svelte';
  import { filteredSegments, searchQuery, segments, updateSegmentText, appSettings } from '../lib/sessionStore';
  import { formatTime, languageClass, languageLabel } from '../lib/utils';

  let panelEl: HTMLElement;
  let editingId: string | null = null;
  let editText = '';

  // Auto-scroll to bottom when new segments arrive
  afterUpdate(() => {
    if ($appSettings.autoScroll && panelEl) {
      panelEl.scrollTop = panelEl.scrollHeight;
    }
  });

  function handleScroll() {
    if (!panelEl) return;
    const atBottom = panelEl.scrollHeight - panelEl.scrollTop - panelEl.clientHeight < 40;
    if (!atBottom) {
      // User scrolled up — disable auto-scroll until they scroll back down
      appSettings.update((s) => ({ ...s, autoScroll: false }));
    }
  }

  function startEdit(id: string, text: string) {
    editingId = id;
    editText = text;
  }

  function commitEdit(id: string) {
    if (editText.trim()) updateSegmentText(id, editText.trim());
    editingId = null;
  }

  function cancelEdit() {
    editingId = null;
  }

  function handleEditKeydown(event: KeyboardEvent, id: string) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      commitEdit(id);
    } else if (event.key === 'Escape') {
      cancelEdit();
    }
  }

  /**
   * Split text into alternating plain/match parts for safe rendering.
   * No HTML is injected — each part is rendered as a plain text node.
   */
  function splitHighlight(text: string, query: string): Array<{ part: string; match: boolean }> {
    if (!query.trim()) return [{ part: text, match: false }];
    const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const re = new RegExp(`(${escaped})`, 'gi');
    const parts: Array<{ part: string; match: boolean }> = [];
    let last = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text)) !== null) {
      if (m.index > last) parts.push({ part: text.slice(last, m.index), match: false });
      parts.push({ part: m[1], match: true });
      last = m.index + m[1].length;
    }
    if (last < text.length) parts.push({ part: text.slice(last), match: false });
    return parts;
  }
</script>

<div class="transcript-toolbar">
  <input
    class="search-input"
    type="search"
    placeholder="Search transcript…"
    bind:value={$searchQuery}
    aria-label="Search transcript"
  />
  <span class="segment-count" aria-live="polite">
    {$segments.length} segment{$segments.length !== 1 ? 's' : ''}
  </span>
</div>

<div
  class="transcript-panel"
  bind:this={panelEl}
  on:scroll={handleScroll}
  role="log"
  aria-label="Live transcript"
  aria-live="polite"
  aria-relevant="additions"
>
  {#if $filteredSegments.length === 0}
    <p class="empty-state">
      {$searchQuery ? 'No segments match your search.' : 'Transcript will appear here once the session starts.'}
    </p>
  {/if}

  {#each $filteredSegments as segment (segment.id)}
    <div class="segment" data-speaker={segment.speaker}>

      <div class="segment-meta">
        {#if $appSettings.showSpeakerLabels}
          <span class="segment-speaker">{segment.speaker}</span>
        {/if}
        <span class="segment-time">{formatTime(segment.start_time)} – {formatTime(segment.end_time)}</span>
        {#if $appSettings.showLanguageLabels}
          <span class="segment-lang lang-pill">{languageLabel(segment.language)}</span>
        {/if}
      </div>

      {#if editingId === segment.id}
        <textarea
          class="segment-edit {languageClass(segment.language)}"
          bind:value={editText}
          on:keydown={(e) => handleEditKeydown(e, segment.id)}
          aria-label="Edit transcript segment"
          rows={3}
        ></textarea>
        <div class="edit-actions">
          <button class="btn-sm btn-save" on:click={() => commitEdit(segment.id)}>Save</button>
          <button class="btn-sm btn-cancel" on:click={cancelEdit}>Cancel</button>
        </div>
      {:else}
        <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
        <p
          class="segment-text {languageClass(segment.language)}"
          title="Double-click to edit"
          on:dblclick={() => startEdit(segment.id, segment.text)}
        >{#each splitHighlight(segment.text, $searchQuery) as chunk}<mark class:is-match={chunk.match}>{chunk.part}</mark>{/each}</p>
        <div class="segment-footer">
          <span class="edit-hint">Double-click to edit</span>
          <button
            class="btn-edit"
            on:click={() => startEdit(segment.id, segment.text)}
            aria-label="Edit this segment"
            title="Edit segment"
          >✎ Edit</button>
        </div>
      {/if}

    </div>
  {/each}
</div>

<style>
  .transcript-toolbar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 1rem;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }

  .search-input {
    flex: 1;
    padding: 0.35rem 0.7rem;
    background: var(--color-input-bg);
    color: var(--color-text);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    font-size: 0.85rem;
  }

  .segment-count {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  .transcript-panel {
    flex: 1;
    overflow-y: auto;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .empty-state {
    text-align: center;
    color: var(--color-text-muted);
    font-size: 0.9rem;
    margin-top: 2rem;
  }

  .segment {
    background: var(--color-card);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 0.65rem 0.9rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .segment-meta {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .segment-speaker {
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--color-primary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .segment-time {
    font-size: 0.7rem;
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .lang-pill {
    font-size: 0.65rem;
    padding: 0.1rem 0.4rem;
    border-radius: 999px;
    background: var(--color-idle-bg);
    color: var(--color-text-muted);
  }

  .segment-text {
    margin: 0;
    font-size: 0.95rem;
    line-height: 1.6;
    color: var(--color-text);
    cursor: text;
  }

  /* Safe highlight — uses a class, not injected HTML */
  :global(mark) {
    background: transparent;
    color: inherit;
    padding: 0;
  }
  :global(mark.is-match) {
    background: #7c5c00;
    color: #fefcbf;
    border-radius: 2px;
    padding: 0 1px;
  }

  .segment-edit {
    width: 100%;
    font-size: 0.95rem;
    line-height: 1.6;
    padding: 0.4rem 0.6rem;
    background: var(--color-input-bg);
    color: var(--color-text);
    border: 1px solid var(--color-primary);
    border-radius: 6px;
    resize: vertical;
    box-sizing: border-box;
  }

  .edit-actions {
    display: flex;
    gap: 0.4rem;
  }

  .btn-sm {
    padding: 0.25rem 0.65rem;
    font-size: 0.75rem;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-weight: 600;
  }

  .btn-save   { background: var(--color-primary); color: #fff; }
  .btn-cancel { background: var(--color-idle-bg); color: var(--color-text-muted); }

  .edit-hint {
    font-size: 0.65rem;
    color: var(--color-text-muted);
    opacity: 0;
    transition: opacity 0.15s;
  }

  .segment-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .segment:hover .edit-hint { opacity: 1; }

  .btn-edit {
    font-size: 0.68rem;
    padding: 0.15rem 0.5rem;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s;
  }

  .segment:hover .btn-edit { opacity: 1; }

  .btn-edit:hover {
    background: var(--color-idle-bg);
    color: var(--color-text);
  }

  /* Multilingual font targeting */
  :global(.lang-si) { font-family: 'Noto Sans Sinhala', 'Iskoola Pota', sans-serif; }
  :global(.lang-ta) { font-family: 'Noto Sans Tamil', 'Latha', sans-serif; }
  :global(.lang-en) { font-family: inherit; }
  :global(.lang-und){ font-family: inherit; color: var(--color-text-muted); }
</style>
