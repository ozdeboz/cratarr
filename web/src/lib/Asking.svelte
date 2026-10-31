<script lang="ts">
  // A list still asking the server for the words as they are now (lib/search listShows): nothing for a moment, so a
  // quick answer shows no loader, then the mark in motion (lib/MarkLoader; the owner, 2026-10-31: a spinner while the
  // page swaps its content for the search's). Its room is kept from the start, so the loader's coming moves nothing.
  import MarkLoader from './MarkLoader.svelte';
  import { ASK_DELAY } from './search';

  let { size = 48 }: { size?: number } = $props();

  let shown = $state(false);
  $effect(() => {
    const t = setTimeout(() => (shown = true), ASK_DELAY);
    return () => clearTimeout(t);
  });
</script>

<div class="asking" aria-busy="true">
  {#if shown}<MarkLoader {size} />{/if}
</div>

<style>
  .asking {
    display: grid;
    place-items: center;
    min-height: 96px;
    padding-top: 24px;
  }
</style>
