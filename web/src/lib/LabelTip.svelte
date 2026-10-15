<script lang="ts">
  // The label of an icon button, shown on a long-press or right-click (lib/labels): a small tip above or below it,
  // kept on the screen.
  import { tip } from './labels.svelte';

  let width = $state(0);
  const left = $derived(Math.max(8, Math.min(tip.x - width / 2, innerWidth - width - 8)));
</script>

{#if tip.shown}
  <div class="tip" class:below={tip.below} role="tooltip" bind:clientWidth={width} style:left="{left}px" style:top="{tip.y}px">{tip.text}</div>
{/if}

<style>
  .tip {
    position: fixed;
    z-index: 1000;
    max-width: calc(100vw - 16px);
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--text);
    color: var(--bg);
    font-size: 13px;
    white-space: nowrap;
    pointer-events: none;
    transform: translateY(calc(-100% - 8px));
  }

  .tip.below {
    transform: translateY(8px);
  }
</style>
