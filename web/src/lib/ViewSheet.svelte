<script lang="ts">
  // The page's View (lib/TopPill's view button, decided 2026-10-15): how its list shows, as the page had it in its own
  // toolbar. Playlists: the order (A–Z, or those with the playing track first) and the size; Collection: the order, rows
  // or covers, and the size; Incoming: rows or covers, and the size.
  import MenuSheet from './MenuSheet.svelte';
  import { ALBUM_LAYOUTS, ALBUM_ORDERS, ALBUM_WIDTHS, PLAYLIST_WIDTHS, pageView } from './pageView.svelte';
  import { prefs } from './prefs.svelte';
  import Seg from './Seg.svelte';
  import SizeControl from './SizeControl.svelte';

  let { page, onclose }: { page: string; onclose: () => void } = $props();
</script>

<MenuSheet label="View" icon="sliders" title="View" {onclose}>
  <div class="parts">
    {#if page === 'playlists'}
      <div class="part">
        <span>Order</span>
        <Seg
          label="Order"
          options={[
            { value: false, label: 'A–Z' },
            { value: true, label: '✓ first', title: 'Playlists with the playing track first' },
          ]}
          bind:value={prefs.membersFirst}
        />
      </div>
      <div class="part">
        <span>Size</span>
        <SizeControl widths={PLAYLIST_WIDTHS} width={pageView.width} bind:value={prefs.zoom} />
      </div>
    {:else}
      {#if page === 'collection'}
        <div class="part">
          <span>Order</span>
          <Seg label="Order" options={ALBUM_ORDERS} bind:value={prefs.collectionOrder} />
        </div>
      {/if}
      <div class="part">
        <span>Layout</span>
        <Seg label="Layout" options={ALBUM_LAYOUTS} bind:value={prefs.albumLayout} />
      </div>
      <div class="part">
        <span>Size</span>
        <SizeControl widths={ALBUM_WIDTHS[prefs.albumLayout]} width={pageView.width} bind:value={prefs.albumZoom} />
      </div>
    {/if}
  </div>
</MenuSheet>

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
