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
  import Card from './Card.svelte';
  import ChoiceList from './ChoiceList.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { ALBUM_LAYOUTS, ALBUM_ORDERS, ALBUM_WIDTHS, CRATE_ORDERS, DATES, DIRECTIONS, PLAYLIST_ORDERS, PLAYLIST_WIDTHS, pageView } from './pageView.svelte';
  import { prefs } from './prefs.svelte';
  import Seg from './Seg.svelte';
  import SettingRow from './SettingRow.svelte';
  import SizeControl from './SizeControl.svelte';

  // `order`: the panel list's own order instead of a page's (`page` null).
  let { page = null, order = null, onclose }: { page?: string | null; order?: ListOrder | null; onclose: () => void } = $props();
</script>

<MenuSheet label="View" icon="sliders" title="View" {onclose}>
  <div class="parts">
    {#if order}
      <!-- One under another (the list header's Order sheet's): a track list has too many to fit in a row. -->
      {@const o = order}
      <Card title="Order" label="Order">
        <ChoiceList label="Order" options={o.fields} value={o.value} onchange={(v) => o.set(v, o.down)} />
      </Card>
      <SettingRow card label="Direction">
        <Seg label="Direction" options={DIRECTIONS} bind:value={() => o.down, (d) => o.set(o.value, d)} />
      </SettingRow>
    {:else if page === 'playlists'}
      <SettingRow card label="Order"><Seg label="Order" options={PLAYLIST_ORDERS} bind:value={prefs.playlistOrder} /></SettingRow>
      <SettingRow card label="Direction"><Seg label="Direction" options={DIRECTIONS} bind:value={prefs.playlistDown} /></SettingRow>
      <SettingRow card label="Size"><SizeControl widths={PLAYLIST_WIDTHS} width={pageView.width} bind:value={prefs.zoom} /></SettingRow>
    {:else if page === 'crates'}
      <SettingRow card label="Order"><Seg label="Order" options={CRATE_ORDERS} bind:value={prefs.crateOrder} /></SettingRow>
      <SettingRow card label="Direction"><Seg label="Direction" options={DIRECTIONS} bind:value={prefs.crateDown} /></SettingRow>
    {:else if page === 'downloads'}
      <SettingRow card label="Order"><Seg label="Order" options={ALBUM_ORDERS} bind:value={prefs.downloadsOrder} /></SettingRow>
      <SettingRow card label="Direction"><Seg label="Direction" options={DIRECTIONS} bind:value={prefs.downloadsDown} /></SettingRow>
      {@render albums()}
    {:else}
      <SettingRow card label="Order"><Seg label="Order" options={ALBUM_ORDERS} bind:value={prefs.libraryOrder} /></SettingRow>
      <SettingRow card label="Direction"><Seg label="Direction" options={DIRECTIONS} bind:value={prefs.libraryDown} /></SettingRow>
      {@render albums()}
    {/if}
  </div>
</MenuSheet>

{#snippet albums()}
  <SettingRow card label="Layout"><Seg label="Layout" options={ALBUM_LAYOUTS} bind:value={prefs.albumLayout} /></SettingRow>
  <SettingRow card label="Size"><SizeControl widths={ALBUM_WIDTHS[prefs.albumLayout]} width={pageView.width} bind:value={prefs.albumZoom} /></SettingRow>
  <SettingRow card label="Dates"><Seg label="Dates" options={DATES} bind:value={prefs.relativeDates} /></SettingRow>
{/snippet}

<style>
  /* Each setting a row on a card of its own (lib/SettingRow, lib/Card). */
  .parts {
    display: grid;
    gap: 4px;
    padding: 4px 16px 16px;
  }
</style>
