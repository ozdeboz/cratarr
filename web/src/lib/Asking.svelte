<script lang="ts">
  // A list's place while its answer is on its way (lib/loader: the one rule): the mark in motion (lib/MarkLoader; the
  // owner, 2026-10-31: a spinner while the page swaps its content for the search's). `now`: what the list showed was
  // kept until the loader was due (lib/keepWhile), so it shows at once; else nothing was there to keep, and it comes
  // LOADER_AFTER_MS after the wait began, as it would have. Its room is kept from the start, so its coming moves nothing.
  import MarkLoader from './MarkLoader.svelte';
  import { LOADER_AFTER_MS } from './loader';

  let { size = 48, now = false }: { size?: number; now?: boolean } = $props();

  let due = $state(false);
  $effect(() => {
    const t = setTimeout(() => (due = true), LOADER_AFTER_MS);
    return () => clearTimeout(t);
  });
</script>

<div class="asking" aria-busy="true">
  {#if now || due}<MarkLoader {size} />{/if}
</div>

<style>
  .asking {
    display: grid;
    place-items: center;
    min-height: 96px;
    padding-top: 24px;
  }
</style>
