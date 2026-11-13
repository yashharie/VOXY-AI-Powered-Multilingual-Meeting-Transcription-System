<script lang="ts">
  import Header from './components/Header.svelte';
  import SessionControls from './components/SessionControls.svelte';
  import SessionInfo from './components/SessionInfo.svelte';
  import TranscriptPanel from './components/TranscriptPanel.svelte';
  import TranscriptActions from './components/TranscriptActions.svelte';
  import SettingsPage from './components/SettingsPage.svelte';
  import DiagnosticsPage from './components/DiagnosticsPage.svelte';
  import StatusBar from './components/StatusBar.svelte';

  type View = 'session' | 'settings' | 'diagnostics';
  let activeView: View = 'session';
</script>

<div class="app-shell">
  <Header />

  <div class="app-body">

    <!-- ΓöÇΓöÇ Sidebar navigation ΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇ -->
    <nav class="sidebar" aria-label="Main navigation">
      <button
        class="nav-btn"
        class:active={activeView === 'session'}
        on:click={() => activeView = 'session'}
        aria-current={activeView === 'session' ? 'page' : undefined}
      >
        ≡ƒÄÖ Session
      </button>
      <button
        class="nav-btn"
        class:active={activeView === 'settings'}
        on:click={() => activeView = 'settings'}
        aria-current={activeView === 'settings' ? 'page' : undefined}
      >
        ΓÜÖ Settings
      </button>
      <button
        class="nav-btn"
        class:active={activeView === 'diagnostics'}
        on:click={() => activeView = 'diagnostics'}
        aria-current={activeView === 'diagnostics' ? 'page' : undefined}
      >
        ≡ƒöì Diagnostics
      </button>
    </nav>

    <!-- ΓöÇΓöÇ Main content area ΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇ -->
    <main class="app-main">

      {#if activeView === 'session'}
        <div class="left-panel">
          <SessionControls />
          <SessionInfo />
        </div>
        <div class="transcript-column">
          <TranscriptPanel />
          <TranscriptActions />
        </div>

      {:else if activeView === 'settings'}
        <SettingsPage />

      {:else if activeView === 'diagnostics'}
        <DiagnosticsPage />
      {/if}

    </main>
  </div>

  <StatusBar />
</div>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  .app-body {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  /* ΓöÇΓöÇ Sidebar ΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇ */
  .sidebar {
    width: 130px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.75rem 0.5rem;
    background: var(--color-surface);
    border-right: 1px solid var(--color-border);
  }

  .nav-btn {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.55rem 0.75rem;
    background: transparent;
    color: var(--color-text-muted);
    border: none;
    border-radius: 6px;
    font-size: 0.82rem;
    cursor: pointer;
    text-align: left;
    transition: background 0.15s, color 0.15s;
    width: 100%;
  }

  .nav-btn:hover {
    background: var(--color-idle-bg);
    color: var(--color-text);
  }

  .nav-btn.active {
    background: var(--color-idle-bg);
    color: var(--color-primary);
    font-weight: 600;
  }

  /* ΓöÇΓöÇ Main content ΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇΓöÇ */
  .app-main {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .left-panel {
    width: 260px;
    flex-shrink: 0;
    border-right: 1px solid var(--color-border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .transcript-column {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
</style>
