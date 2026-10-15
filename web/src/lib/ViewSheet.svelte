<script lang="ts">
  // The page's View (lib/TopPill's view button, decided 2026-10-15): how its list shows. Its order and direction (the
  // same as its list header's), then for playlists the size, for albums rows or covers and the size.
  import MenuSheet from './MenuSheet.svelte';
  import { ALBUM_LAYOUTS, ALBUM_ORDERS, ALBUM_WIDTHS, CRATE_ORDERS, DIRECTIONS, PLAYLIST_ORDERS, PLAYLIST_WIDTHS, pageView } from './pageView.svelte';
  import { prefs } from './prefs.svelte';
  import Seg from './Seg.svelte';
  import SizeControl from './SizeControl.svelte';

  let { page, onclose }: { page: string; onclose: () => void } = $props();
</script>

<MenuSheet label="View" icon="sliders" title="View" {onclose}>
  <div class="parts">
    {#if page === 'playlists'}
      <div class="part"><span>Order</span><Seg label="Order" options={PLAYLIST_ORDERS} bind:value={prefs.playlistOrder} /></div>
      <div class="part"><span>Direction</span><Seg label="Direction" options={DIRECTIONS} bind:value={prefs.playlistDown} /></div>
      <div class="part"><span>Size</span><SizeControl widths={PLAYLIST_WIDTHS} width={pageView.width} bind:value={prefs.zoom} /></div>
    {:else if page === 'crates'}
      <div class="part"><span>Order</span><Seg label="Order" options={CRATE_ORDERS} bind:value={prefs.crateOrder} /></div>
      <div class="part"><span>Direction</span><Seg label="Direction" options={DIRECTIONS} bind:value={prefs.crateDown} /></div>
    {:else if page === 'collection'}
      <div class="part"><span>Order</span><Seg label="Order" options={ALBUM_ORDERS} bind:value={prefs.collectionOrder} /></div>
      <div class="part"><span>Direction</span><Seg label="Direction" options={DIRECTIONS} bind:value={prefs.collectionDown} /></div>
      {@render albums()}
    {:else}
      <div class="part"><span>Order</span><Seg label="Order" options={ALBUM_ORDERS} bind:value={prefs.incomingOrder} /></div>
      <div class="part"><span>Direction</span><Seg label="Direction" options={DIRECTIONS} bind:value={prefs.incomingDown} /></div>
      {@render albums()}
    {/if}
  </div>
</MenuSheet>

{#snippet albums()}
  <div class="part"><span>Layout</span><Seg label="Layout" options={ALBUM_LAYOUTS} bind:value={prefs.albumLayout} /></div>
  <div class="part">
    <span>Size</span>
    <SizeControl widths={ALBUM_WIDTHS[prefs.albumLayout]} width={pageView.width} bind:value={prefs.albumZoom} />
  </div>
{/snippet}

<style>
  /* Each setting a row: its name, the control at the end (the filter sheet's cards). */
  .parts {
    display: grid;
    gap: 4px;
    padding: 4px 16px 16px;
  }

  .part {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 56px;
    padding: 0 12px 0 16px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .part > span {
    font-size: 15px;
  }
</style>
