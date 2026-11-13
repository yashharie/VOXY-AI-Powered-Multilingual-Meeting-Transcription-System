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
export function languageLabel(code: string | null): string {
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
export function languageClass(code: string | null): string {
  switch (code) {
    case 'si': return 'lang-si';
    case 'ta': return 'lang-ta';
    case 'en': return 'lang-en';
    default:   return 'lang-und';
  }
}
