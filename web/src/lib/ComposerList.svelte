<script lang="ts">
  // The Library's Classical, first (0.10; the owner, 2026-10-30: "it opens on a composer list"): its composers, each
  // with how many records, a round picture where one is known; a composer opens their records (lib/classical). The
  // page's search narrows them. As the Library's albums (the owner, 2026-10-31: no order, and View did nothing): the
  // list's header orders them (Name, Records) and turns them round, View's layout (rows, covers or a table; no Flow,
  // lib/layouts) and size apply, and a table's column names order it as every table's head does (lib/columnHead).
  import Cover from './Cover.svelte';
  import PageLoading from './PageLoading.svelte';
  import ListHead from './ListHead.svelte';
  import Row, { type Cell } from './Row.svelte';
  import Tile from './Tile.svelte';
  import ViewSheet from './ViewSheet.svelte';
  import { fromAction } from 'svelte/attachments';
  import { longpress } from './longpress';
  import { classical, loadComposers, openComposer } from './classical.svelte';
  import { fit } from './columns.svelte';
  import { headCells as headCellsOf, headSort, type Order } from './columnHead';
  import { plural } from './insights.svelte';
  import { libraryLayoutShown } from './layouts';
  import { ALBUM_WIDTHS, COMPOSER_ORDERS, howMany, matchesSearch, pageView, searchNone } from './pageView.svelte';
  import { prefs, type ComposerOrder } from './prefs.svelte';
  import { composersInOrder } from './composers';

  $effect(() => loadComposers());
  const shown = $derived(composersInOrder((classical.composers ?? []).filter((c) => matchesSearch(c.name, c.sort_name)), prefs.composerOrder, prefs.composerDown));
  // Its words leave none: the pill says so, from its magnifier (pageView searchNone).
  searchNone(() => !shown.length && { n: classical.composers?.length ?? 0, one: 'composer' });

  let width = $state(0);
  // View's size steps depend on the width the list has.
  $effect(() => {
    pageView.width = width;
  });
  // As chosen in View, as it can be here; the table while the page's search has words (lib/layouts).
  const layout = $derived(libraryLayoutShown('composers'));
  const table = $derived(layout === 'table');
  const minPx = $derived(table ? width : ALBUM_WIDTHS[layout][prefs.albumZoom]);

  // The table's columns as many as the width allows (decision 0042), its names ordering it.
  const tableCols = $derived(table ? fit('composers', width) : []);
  const ORDER_OF: Partial<Record<string, ComposerOrder>> = { records: 'records' };
  const now = $derived<Order<ComposerOrder>>({ key: prefs.composerOrder, down: prefs.composerDown });
  const setOrder = (o: Order<ComposerOrder>) => ((prefs.composerOrder = o.key), (prefs.composerDown = o.down));
  const headCells = $derived<Cell[]>(headCellsOf(tableCols, (id) => ORDER_OF[id], now, COMPOSER_ORDERS, setOrder));
  const headSorts = $derived({ title: headSort<ComposerOrder>('name', now, COMPOSER_ORDERS, setOrder) });
  /** A long press on the column names: View, where the columns are chosen (as the albums' table). */
  let columnsOpen = $state(false);
  const headPress = fromAction(longpress, () => () => (columnsOpen = true));
  const cellsOf = (records: number): Cell[] => tableCols.map((c) => ({ ...c, text: records.toLocaleString() }));
  /** Over a tile's picture, what the list is ordered by (as an album's year). */
  const tagOf = (records: number) => (prefs.composerOrder === 'records' ? plural(records, 'record') : null);
</script>

{#if classical.error && !classical.composers}
  <p class="why" role="alert">{classical.error}</p>
{:else if !classical.composers}
  <PageLoading />
{:else if !shown.length}
  {#if !classical.composers.length}<p class="empty">No classical records yet.</p>
  {:else if !pageView.none}<p class="empty">No composers match.</p>{/if}
{:else}
  <div class="list" bind:clientWidth={width}>
    <ListHead fields={COMPOSER_ORDERS} bind:value={prefs.composerOrder} bind:down={prefs.composerDown} count={howMany(shown.length, 'composer')} />
    {#if tableCols.length}
      <Row size="large" as="div" head title="Name" cells={headCells} sorts={headSorts} attach={headPress}>
        {#snippet front()}<span class="art small space"></span>{/snippet}
        {#snippet action()}<span></span>{/snippet}
      </Row>
    {/if}
    <ul class={layout} aria-label="Composers" style:--min="{minPx}px">
      {#each shown as c (c.key)}
        <li>
          {#if layout === 'grid'}
            <Tile title={c.name} sub={plural(c.records, 'record')} tag={tagOf(c.records)} label={`Open ${c.name}`} onclick={() => openComposer(c)}>
              {#snippet image()}<span class="art round"><Cover src={c.photo ?? null} /></span>{/snippet}
            </Tile>
          {:else if table}
            <Row size="large" title={c.name} cells={tableCols.length ? cellsOf(c.records) : undefined} sub={tableCols.length ? undefined : plural(c.records, 'record')} label={`Open ${c.name}`} onclick={() => openComposer(c)}>
              {#snippet front()}<span class="art small"><Cover src={c.photo ?? null} /></span>{/snippet}
            </Row>
          {:else}
            <Row size="xl" title={c.name} sub={plural(c.records, 'record')} label={`Open ${c.name}`} onclick={() => openComposer(c)}>
              {#snippet front()}<span class="art"><Cover src={c.photo ?? null} /></span>{/snippet}
            </Row>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
{/if}
{#if columnsOpen}<ViewSheet page="library" kind="composers" onclose={() => (columnsOpen = false)} />{/if}

<style>
  /* In columns as wide as View's size makes them, as the albums are (lib/AlbumShelf); a table one a line. */
  ul {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(var(--min), 100%), 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  ul.table {
    display: block;
  }

  li {
    min-width: 0;
  }

  .art {
    position: relative;
    flex: none;
    display: block;
    width: 72px;
    height: 72px;
    overflow: hidden;
    border-radius: 50%;
  }

  /* A table's, as its albums' smaller cover; the head's room for it. */
  .art.small {
    width: 48px;
    height: 48px;
    margin-right: 6px;
  }

  .art.space {
    height: 0;
  }

  /* A tile's: its picture's whole width, round. */
  .art.round {
    width: 100%;
    height: auto;
    aspect-ratio: 1;
  }

  .empty,
  .why {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }
</style>
