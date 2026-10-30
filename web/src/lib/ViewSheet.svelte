<script lang="ts" module>
  import type { ColumnKind } from './columns.svelte';

  /** A list's order shown in View when it is not a page's (the panel's track list); `columns`: its columns where it is
   *  wide enough for any (decision 0042), the ones shown now and the ones it has values for. */
  export type ListOrder = {
    fields: { value: string; label: string }[];
    value: string;
    down: boolean;
    set: (value: string, down: boolean) => void;
    columns?: { kind: ColumnKind; shown: readonly string[]; offered: readonly string[] };
    /** A crate's: as records or as tracks (0.10, lib/crateRecords), its own choice. */
    view?: { value: 'records' | 'tracks'; set: (v: 'records' | 'tracks') => void };
  };
  const CRATE_VIEWS = [
    { value: 'records' as const, label: 'Records' },
    { value: 'tracks' as const, label: 'Tracks' },
  ];
</script>

<script lang="ts">
  // The page's View (lib/TopPill's view button, decided 2026-10-15): how its list shows: for playlists chips or a table
  // and the size, for albums rows, covers or a table and the size, a track list's columns. Not its order or direction,
  // which only its list's header sets (decision 0060).
  import ColumnOrder from './ColumnOrder.svelte';
  import { COLUMNS, fit, LIBRARY_TRACK_COLUMNS } from './columns.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { ALBUM_WIDTHS, DATES, PLAYLIST_WIDTHS, pageView } from './pageView.svelte';
  import { libraryLayout, libraryLayouts, playlistLayout, playlistLayouts, type LibraryList } from './layouts';
  import { prefs } from './prefs.svelte';
  import Seg from './Seg.svelte';
  import SettingRow from './SettingRow.svelte';
  import SizeControl from './SizeControl.svelte';

  // `order`: the panel list's own order instead of a page's (`page` null). The Library's columns are its list's: its
  // records' (`kind`: a list of the device's, lib/layouts).
  let { page = null, order = null, kind, onclose }: { page?: string | null; order?: ListOrder | null; kind?: LibraryList; onclose: () => void } = $props();
  const listKind = $derived<LibraryList>(kind ?? 'albums');
  const offered = $derived(listKind === 'tracks' ? LIBRARY_TRACK_COLUMNS : COLUMNS[listKind].map((c) => c.id));
</script>

<MenuSheet label="View" icon="sliders" title="View" {onclose}>
  <div class="parts">
    {#if order}
      {#if order.view}
        {@const v = order.view}
        <SettingRow card label="Show"><Seg label="Show" options={CRATE_VIEWS} bind:value={() => v.value, (x) => v.set(x)} /></SettingRow>
      {/if}
      {#if order.columns?.shown.length && order.view?.value !== 'records'}<ColumnOrder {...order.columns} />{/if}
    {:else if page === 'playlists'}
      <SettingRow card label="Layout"><Seg label="Layout" options={playlistLayouts()} bind:value={playlistLayout, (v) => (prefs.playlistLayout = v)} /></SettingRow>
      {#if playlistLayout() === 'table'}
        <ColumnOrder kind="playlists" shown={fit('playlists', pageView.width).map((c) => c.id)} offered={COLUMNS.playlists.map((c) => c.id)} />
      {:else}
        <SettingRow card label="Size"><SizeControl widths={PLAYLIST_WIDTHS} width={pageView.width} bind:value={prefs.zoom} /></SettingRow>
      {/if}
    {:else}
      {@render albums()}
    {/if}
  </div>
</MenuSheet>

{#snippet albums()}
  <SettingRow card label="Layout"><Seg label="Layout" options={libraryLayouts(listKind)} bind:value={() => libraryLayout(listKind), (v) => (prefs.albumLayout = v)} /></SettingRow>
  {#if libraryLayout(listKind) === 'table'}
    <!-- A table has no sizes; its columns, as many as the page has room for, in the order dragged here (decision 0042). -->
    <ColumnOrder kind={listKind} shown={fit(listKind, pageView.width, (id) => offered.includes(id)).map((c) => c.id)} {offered} />
  {:else}
    <SettingRow card label="Size"><SizeControl widths={ALBUM_WIDTHS[libraryLayout(listKind)]} width={pageView.width} bind:value={prefs.albumZoom} /></SettingRow>
  {/if}
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
