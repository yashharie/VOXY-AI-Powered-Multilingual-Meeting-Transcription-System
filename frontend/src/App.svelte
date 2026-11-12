<script lang="ts">
  import { onMount } from "svelte";
  import { sessionStore } from "./stores/session";
  import SessionView from "./components/SessionView.svelte";
  import StatusBar from "./components/StatusBar.svelte";

  let ready = false;

  onMount(async () => {
    await sessionStore.initialize();
    ready = true;
  });
</script>

<main class="app-shell">
  <header class="app-header">
    <div class="app-brand">
      <span class="brand-icon">🎙</span>
      <span class="brand-name">VOXY</span>
      <span class="brand-tagline">AI Live Transcription</span>
    </div>
    {#if ready}
      <StatusBar />
    {/if}
  </header>

  <div class="app-content">
    {#if ready}
      <SessionView />
    {:else}
      <div class="loading">
        <div class="loading-spinner"></div>
        <p>Initialising VOXY…</p>
      </div>
    {/if}
  </div>
</main>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--color-bg);
  }

  .app-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    background: var(--color-surface);
    border-bottom: 1px solid var(--color-border);
    flex-shrink: 0;
  }

  .app-brand {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .brand-icon {
    font-size: 20px;
  }

  .brand-name {
    font-size: 18px;
    font-weight: 700;
    color: var(--color-accent);
    letter-spacing: 2px;
  }

  .brand-tagline {
    font-size: 11px;
    color: var(--color-text-secondary);
    letter-spacing: 1px;
    text-transform: uppercase;
  }

  .app-content {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 16px;
    color: var(--color-text-secondary);
  }

  .loading-spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--color-border);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
