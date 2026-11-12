<script lang="ts">
  import Header from './components/Header.svelte';
  import SessionControls from './components/SessionControls.svelte';
  import TranscriptPanel from './components/TranscriptPanel.svelte';
  import TranscriptActions from './components/TranscriptActions.svelte';
  import StatusBar from './components/StatusBar.svelte';
</script>

<div class="app-shell">
  <Header />

  <main class="app-main">
    <div class="left-panel">
      <SessionControls />
    </div>

    <div class="transcript-column">
      <TranscriptPanel />
      <TranscriptActions />
    </div>
  </main>

  <StatusBar />
</div>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  .app-main {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .left-panel {
    width: 280px;
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
