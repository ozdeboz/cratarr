<script lang="ts">
  // A page's or a panel's whole content loading: the mark in motion (lib/MarkLoader.svelte), in the middle of where the
  // content will be.
  import MarkLoader from './MarkLoader.svelte';

  let { size = 96 }: { size?: number } = $props();
</script>

<div class="page-loading"><MarkLoader {size} /></div>

<style>
  .page-loading {
    display: grid;
    place-items: center;
    min-height: 40vh;
  }
</style>
