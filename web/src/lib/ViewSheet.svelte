<script lang="ts" module>
  /** A list's order shown in View when it is not a page's (the panel's track list). */
  export type ListOrder = {
    fields: { value: string; label: string }[];
    value: string;
    down: boolean;
    set: (value: string, down: boolean) => void;
  };
</script>

<script lang="ts">
  // The page's View (lib/TopPill's view button, decided 2026-10-15): how its list shows. Its order and direction (the
  // same as its list header's), then for playlists the size, for albums rows or covers and the size.
  import Icon from './Icon.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { ALBUM_LAYOUTS, ALBUM_ORDERS, ALBUM_WIDTHS, CRATE_ORDERS, DATES, DIRECTIONS, PLAYLIST_ORDERS, PLAYLIST_WIDTHS, pageView } from './pageView.svelte';
  import { prefs } from './prefs.svelte';
  import Seg from './Seg.svelte';
  import SizeControl from './SizeControl.svelte';

  // `order`: the panel list's own order instead of a page's (`page` null).
  let { page = null, order = null, onclose }: { page?: string | null; order?: ListOrder | null; onclose: () => void } = $props();
</script>

<MenuSheet label="View" icon="sliders" title="View" {onclose}>
  <div class="parts">
    {#if order}
      <!-- One under another (the list header's Order sheet's): a track list has too many to fit in a row. -->
      <div class="part choices" role="radiogroup" aria-label="Order">
        <span>Order</span>
        {#each order.fields as f (f.value)}
          {@const on = f.value === order.value}
          <button role="radio" aria-checked={on} class:on onclick={() => order.set(f.value, order.down)}
            ><Icon name={on ? 'circleCheck' : 'circle'} size={22} />{f.label}</button
          >
        {/each}
      </div>
      <div class="part">
        <span>Direction</span>
        <Seg label="Direction" options={DIRECTIONS} bind:value={() => order.down, (d) => order.set(order.value, d)} />
      </div>
    {:else if page === 'playlists'}
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
  <div class="part"><span>Dates</span><Seg label="Dates" options={DATES} bind:value={prefs.relativeDates} /></div>
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

  .part.choices {
    flex-direction: column;
    align-items: stretch;
    gap: 0;
    padding: 6px 12px 6px 16px;
  }

  .choices > span {
    padding: 10px 0 4px;
  }

  .choices button {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 15px;
    text-align: left;
    cursor: pointer;
  }

  .choices button.on :global(svg) {
    color: var(--accent);
  }
</style>
