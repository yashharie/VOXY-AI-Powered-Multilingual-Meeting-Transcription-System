<script lang="ts">
  import { sessionStore } from "../stores/session";

  // Show audio level as a visual VU meter bar
  $: level = $sessionStore.audioLevel;
  $: bars = Array.from({ length: 12 }, (_, i) => i / 12 < level);
</script>

<div class="audio-meter" aria-label="Audio input level">
  {#each bars as active, i}
    <div
      class="bar"
      class:active
      style:--hue={i < 8 ? "142" : i < 10 ? "38" : "0"}
    ></div>
  {/each}
</div>

<style>
  .audio-meter {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 20px;
  }

  .bar {
    width: 4px;
    height: 100%;
    background: var(--color-border);
    border-radius: 2px;
    transition: background 0.08s;
  }

  .bar.active {
    background: hsl(var(--hue), 70%, 55%);
  }
</style>
