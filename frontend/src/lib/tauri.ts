/** Compatibility entry point. IPC/events have one implementation. */
export * from './ipc';
export { onTranscriptSegment, onSessionStateChanged as onSessionStatusChange } from './events';
export function isTauri(): boolean { return typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI__' in window); }