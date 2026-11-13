/**
 * Format a time value in seconds to MM:SS display format.
 */
export function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

/**
 * Map a BCP-47 language code to a human-readable label.
 */
export function languageLabel(code: string): string {
  switch (code) {
    case 'si': return 'Sinhala';
    case 'ta': return 'Tamil';
    case 'en': return 'English';
    default:   return 'Unknown';
  }
}

/**
 * Map a language code to a CSS class used for font targeting.
 */
export function languageClass(code: string): string {
  switch (code) {
    case 'si': return 'lang-si';
    case 'ta': return 'lang-ta';
    case 'en': return 'lang-en';
    default:   return 'lang-und';
  }
}

/**
 * Build a plain-text export string from transcript segments.
 */
export function buildExportText(
  segments: Array<{ start_time: number; speaker: string; text: string }>,
): string {
  return segments
    .map((s) => `[${formatTime(s.start_time)}] ${s.speaker}: ${s.text}`)
    .join('\n');
}

/**
 * Trigger a local file download in the browser / Tauri WebView.
 */
export function downloadTextFile(filename: string, content: string): void {
  const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
