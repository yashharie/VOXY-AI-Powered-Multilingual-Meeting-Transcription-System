<script lang="ts">
  import { segments, sessionName, meetingPlatform } from '../lib/sessionStore';
  import { buildExportText, downloadTextFile } from '../lib/utils';

  let copyFeedback = '';
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function handleCopy() {
    if ($segments.length === 0) return;
    const text = buildExportText($segments);
    try {
      await navigator.clipboard.writeText(text);
      copyFeedback = 'Copied!';
    } catch {
      copyFeedback = 'Copy failed';
    }
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copyFeedback = ''; }, 2000);
  }

  function handleExportTxt() {
    if ($segments.length === 0) return;
    const text = buildExportText($segments);
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
    downloadTextFile(`voxy-transcript-${timestamp}.txt`, text);
  }

  function handleExportPdf() {
    if ($segments.length === 0) return;
    const name = $sessionName || 'Meeting Session';
    const platform = $meetingPlatform || 'General';
    const dateStr = new Date().toLocaleString();
    const rows = $segments.map((s) => `
      <div style="margin-bottom: 12px; padding-bottom: 8px; border-bottom: 1px solid #e2e8f0;">
        <div style="font-size: 11px; color: #64748b; margin-bottom: 4px;">
          <strong style="color: #0284c7;">${s.speaker || 'Speaker'}</strong> • [${s.start_time.toFixed(1)}s - ${s.end_time.toFixed(1)}s] • <em>${(s.language || 'und').toUpperCase()}</em>
        </div>
        <div style="font-size: 13px; color: #0f172a; line-height: 1.5;">${s.text}</div>
      </div>
    `).join('');

    const html = `
      <!DOCTYPE html>
      <html>
        <head>
          <meta charset="utf-8">
          <title>${name} - VOXY Meeting Transcript</title>
          <style>
            body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; padding: 24px; color: #0f172a; background: #fff; }
            .header { border-bottom: 2px solid #0284c7; padding-bottom: 12px; margin-bottom: 20px; }
            h1 { margin: 0 0 6px 0; font-size: 20px; color: #0369a1; }
            .meta { font-size: 12px; color: #475569; }
            @media print { body { padding: 0; } }
          </style>
        </head>
        <body>
          <div class="header">
            <h1>VOXY — Meeting Transcript</h1>
            <div class="meta">
              <strong>Session:</strong> ${name} &nbsp;|&nbsp;
              <strong>Platform:</strong> ${platform} &nbsp;|&nbsp;
              <strong>Date:</strong> ${dateStr} &nbsp;|&nbsp;
              <strong>Total Segments:</strong> ${$segments.length}
            </div>
          </div>
          <div>${rows}</div>
        </body>
      </html>
    `;

    // Try hidden iframe print first (works inside Tauri without popups)
    try {
      const iframe = document.createElement('iframe');
      iframe.style.position = 'fixed';
      iframe.style.right = '0';
      iframe.style.bottom = '0';
      iframe.style.width = '0';
      iframe.style.height = '0';
      iframe.style.border = '0';
      document.body.appendChild(iframe);

      const frameDoc = iframe.contentWindow?.document;
      if (frameDoc) {
        frameDoc.open();
        frameDoc.write(html);
        frameDoc.close();
        setTimeout(() => {
          iframe.contentWindow?.focus();
          iframe.contentWindow?.print();
          setTimeout(() => {
            try { document.body.removeChild(iframe); } catch {}
          }, 3000);
        }, 300);
        return;
      }
    } catch {
      // Fallback to window.open if iframe is blocked
    }

    const printWin = window.open('', '_blank');
    if (printWin) {
      printWin.document.write(html);
      printWin.document.close();
      printWin.focus();
      setTimeout(() => {
        printWin.print();
      }, 250);
    }
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
    on:click={handleExportTxt}
    disabled={$segments.length === 0}
    aria-label="Export transcript as text file"
    title="Export transcript as .txt"
  >
    💾 Export .txt
  </button>

  <button
    class="action-btn"
    on:click={handleExportPdf}
    disabled={$segments.length === 0}
    aria-label="Export transcript as PDF file"
    title="Export transcript as .pdf"
  >
    📄 Export .pdf
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
