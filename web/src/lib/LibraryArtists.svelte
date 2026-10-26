<script lang="ts">
  // The Library's Artists (the owner, 2026-10-25: "in the same style as the album list with a beautiful table"): the
  // libraries' albums by their artists, with the albums' header (its order, how many, Select's room) and their two
  // layouts, as the albums' (the Library's View; the owner, 2026-10-26: Grid and Flow too); in the table their columns as
  // wide as the list allows (decision 0042). The page's search filters them by name. A tap views the artist in the panel, as an album's artist does; a
  // long-press their menu (lib/ArtistRowMenu); Select as the albums' (their tracks to the selection's menu).
  import { panelBeside } from './glass.svelte';
  import { albumCoverUrl, type ShelfSource } from './api';
  import { fit } from './columns.svelte';
  import Cover from './Cover.svelte';
  import FlowStage from './FlowStage.svelte';
  import Tile from './Tile.svelte';
  import { playCollection } from './play.svelte';
  import { plural } from './insights.svelte';
  import ListHead from './ListHead.svelte';
  import { artistSubject, panel, viewPlaylist } from './panel.svelte';
  import { ALBUM_WIDTHS, ARTIST_ORDERS, howMany, matchesSearch } from './pageView.svelte';
  import { leaveSearch } from './globalSearch.svelte';
  import { prefs } from './prefs.svelte';
  import Row, { type Cell } from './Row.svelte';
  import { libraryShown } from './sources.svelte';
  import ArtistRowMenu from './ArtistRowMenu.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import SelectBar from './SelectBar.svelte';
  import SelectionMenu from './SelectionMenu.svelte';
  import SelectSheet from './SelectSheet.svelte';
  import { tracksOfAlbums } from './albums.svelte';
  import type { ListedAlbum } from './api';
  import type { LocalTrack } from './localPlayer.svelte';
  import { fromAction } from 'svelte/attachments';
  import { toast } from './toast.svelte';

  let { sources }: { sources: ShelfSource[] } = $props();

  type Artist = { source: string; name: string; albums: number; tracks: number; cover: string; list: ListedAlbum[] };
  const artists = $derived.by(() => {
    const by = new Map<string, Artist>();
    for (const s of sources)
      for (const a of s.albums) {
        if (a.library && !libraryShown(s.id, a.library)) continue;
        for (const name of a.artists?.length ? a.artists : a.artist ? [a.artist] : []) {
          const key = `${s.id}\u0000${name.toLocaleLowerCase()}`;
          const x = by.get(key) ?? { source: s.id, name, albums: 0, tracks: 0, cover: albumCoverUrl(s.id, a.id), list: [] };
          x.albums++;
          x.list.push(a);
          x.tracks += a.tracks;
          by.set(key, x);
        }
      }
    return [...by.values()];
  });
  const byName = (x: Artist, y: Artist) => x.name.localeCompare(y.name, undefined, { sensitivity: 'base', numeric: true });
  // As every search reads (lib/search, decision 0072); in the header's order, either way round.
  const shown = $derived.by(() => {
    const o = prefs.libraryArtistOrder;
    const list = artists.filter((a) => matchesSearch(a.name));
    list.sort((x, y) => (o === 'name' ? byName(x, y) : x[o] - y[o] || byName(x, y)));
    return prefs.libraryArtistDown ? list.reverse() : list;
  });

  // Select (as the albums'): the ticked ones by `source + name`; null while not selecting.
  const keyOf = (a: Artist) => `${a.source}\u0000${a.name}`;
  let selecting = $state<Set<string> | null>(null);
  let selectSheet = $state(false);
  const ticked = $derived(selecting ? shown.filter((a) => selecting!.has(keyOf(a))) : []);
  function tick(a: Artist) {
    const s = new Set(selecting);
    if (s.has(keyOf(a))) s.delete(keyOf(a));
    else s.add(keyOf(a));
    selecting = s;
  }
  function selectWhich(which: 'all' | 'none' | 'others') {
    const keys = shown.map(keyOf);
    selecting = new Set(which === 'all' ? keys : which === 'none' ? [] : keys.filter((k) => !selecting?.has(k)));
  }
  // The ticked artists' tracks, album by album, for the selection's menu.
  let several = $state<{ title: string; tracks: LocalTrack[] } | null>(null);
  async function menuOfTicked() {
    if (!ticked.length) return;
    try {
      several = { title: plural(ticked.length, 'artist'), tracks: await tracksOfAlbums(ticked.flatMap((a) => a.list.map((x) => ({ source: a.source, a: x })))) };
    } catch (err) {
      toast.fail(err, "Their tracks couldn't be read");
    }
  }
  let menuOf = $state<Artist | null>(null);
  const press = (a: Artist) => fromAction(longpress, () => () => (selecting ? (selectSheet = true) : (menuOf = a)));

  const table = $derived(prefs.albumLayout === 'table');
  const view = (a: Artist) => (leaveSearch(false), viewPlaylist(artistSubject(a.source, a.name)));
  /** Flow: the front artist, by its place in the list. */
  let flowAt = $state(0);
  const flowFront = $derived(shown[Math.min(flowAt, shown.length - 1)] ?? null);
  let starting = $state(false);
  async function playArtist(a: Artist) {
    starting = true;
    try {
      await playCollection({ kind: 'artist', source: a.source, name: a.name }, 'play', a.name);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      starting = false;
    }
  }
  let width = $state(0);
  const cols = $derived(table ? fit('artists', width) : []);
  const cellsOf = (a: Artist): Cell[] => cols.map((c) => ({ ...c, text: (c.id === 'albums' ? a.albums : a.tracks).toLocaleString() }));
  // The column names, a tap on one ordering by it (most first), again the other way round.
  const headCells = $derived<Cell[]>(
    cols.map((c) => {
      const o = c.id as 'albums' | 'tracks';
      return {
        ...c,
        text: c.label,
        on: prefs.libraryArtistOrder === o,
        onclick: () => {
          if (prefs.libraryArtistOrder === o) prefs.libraryArtistDown = !prefs.libraryArtistDown;
          else ((prefs.libraryArtistOrder = o), (prefs.libraryArtistDown = true));
        },
      };
    }),
  );
</script>

<div class="lists" class:table bind:clientWidth={width}>
  {#if !shown.length}
    <p class="empty">{artists.length ? 'No artists match.' : 'No artists.'}</p>
  {:else}
    <ListHead
      fields={ARTIST_ORDERS}
      bind:value={prefs.libraryArtistOrder}
      bind:down={prefs.libraryArtistDown}
      count={howMany(shown.length, 'artist')}
      selecting={!!selecting}
      onselect={() => (selecting = selecting ? null : new Set())}
    />
    {#if prefs.albumLayout === 'flow'}
      <FlowStage
        covers={shown.map((a) => a.cover)}
        marked={selecting ? shown.map((a) => selecting!.has(keyOf(a))) : undefined}
        front={flowFront ? { title: flowFront.name, by: `${plural(flowFront.albums, 'album')} · ${plural(flowFront.tracks, 'track')}` } : null}
        onopen={(i) => (selecting ? tick(shown[i]) : view(shown[i]))}
        onfront={(i) => (flowAt = i)}
        onmenu={(i) => (selecting ? (selectSheet = true) : (menuOf = shown[i]))}
        onname={() => flowFront && view(flowFront)}
        onplay={() => flowFront && playArtist(flowFront)}
        canPlay={!starting}
        playing={starting}
      />
    {:else if prefs.albumLayout === 'grid'}
      <ul class="grid" style:--min="{ALBUM_WIDTHS.grid[prefs.albumZoom] ?? 170}px">
        {#each shown as a (a.source + a.name)}
          {@const on = !!selecting?.has(keyOf(a))}
          <li>
            <Tile
              title={a.name}
              sub={`${plural(a.albums, 'album')} · ${plural(a.tracks, 'track')}`}
              label={selecting ? `${on ? 'Untick' : 'Tick'} ${a.name}` : `View ${a.name}`}
              active={(panelBeside() && panel.viewed === artistSubject(a.source, a.name)) || on}
              onclick={() => (selecting ? tick(a) : view(a))}
              attach={press(a)}
            >
              {#snippet image()}<span class="face round"><Cover src={a.cover} /></span>{/snippet}
              {#snippet action()}
                {#if selecting}
                  <button class="icon tick" class:on role="checkbox" aria-checked={on} aria-label="Select {a.name}" onclick={() => tick(a)}
                    ><Icon name={on ? 'circleCheck' : 'circle'} size={24} /></button
                  >
                {/if}
              {/snippet}
            </Tile>
          </li>
        {/each}
      </ul>
    {:else}
    {#if cols.length}
      <Row size="large" as="div" head title="Name" cells={headCells}>
        {#snippet front()}<span class="face-space"></span>{/snippet}
        <!-- The rows' slot for Select's tick, kept here too: the headings over their columns. -->
        {#snippet action()}<span></span>{/snippet}
      </Row>
    {/if}
    <ul class:table>
      {#each shown as a (a.source + a.name)}
        <li>
          <Row
            size={table ? 'large' : 'xl'}
            title={a.name}
            sub={table && cols.length ? undefined : `${plural(a.albums, 'album')} · ${plural(a.tracks, 'track')}`}
            cells={cols.length ? cellsOf(a) : undefined}
            label={selecting ? `${selecting.has(keyOf(a)) ? 'Untick' : 'Tick'} ${a.name}` : `View ${a.name}`}
            active={(panelBeside() && panel.viewed === artistSubject(a.source, a.name)) || !!selecting?.has(keyOf(a))}
            onclick={() => (selecting ? tick(a) : view(a))}
            attach={press(a)}
          >
            {#snippet front()}<span class="face"><Cover src={a.cover} /></span>{/snippet}
            {#snippet action()}
              {#if selecting}
                {@const on = selecting.has(keyOf(a))}
                <button class="icon tick" class:on role="checkbox" aria-checked={on} aria-label="Select {a.name}" onclick={() => tick(a)}
                  ><Icon name={on ? 'circleCheck' : 'circle'} size={24} /></button
                >
              {/if}
            {/snippet}
          </Row>
        </li>
      {/each}
    </ul>
    {/if}
  {/if}
</div>

{#if selecting}
  <SelectBar
    label="Selected artists"
    count="{ticked.length}/{shown.length}"
    onclose={() => (selecting = null)}
    place="page"
    every={{ all: !!shown.length && ticked.length === shown.length, onall: () => selectWhich('all'), onnone: () => selectWhich('none') }}
  >
    <button aria-label="Menu of the ticked artists" title="More" disabled={!ticked.length} onclick={menuOfTicked}><Icon name="moreVert" size={24} /></button>
  </SelectBar>
{/if}
{#if several}
  <SelectionMenu title={several.title} tracks={several.tracks} onclose={() => ((several = null), (selecting = null))} />
{/if}
{#if selectSheet}
  <SelectSheet onall={() => selectWhich('all')} onnone={() => selectWhich('none')} onothers={() => selectWhich('others')} onclose={() => (selectSheet = false)} />
{/if}
{#if menuOf}
  {@const m = menuOf}
  <ArtistRowMenu source={m.source} name={m.name} cover={m.cover} onclose={() => (menuOf = null)} />
{/if}

<style>
  .tick {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .tick.on {
    color: var(--accent-ink);
  }

  ul {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* As the albums' rows: drawn as they come near (a long list). */
  li {
    min-width: 0;
    content-visibility: auto;
    contain-intrinsic-size: auto 80px;
  }

  /* Grid: as the albums' tiles, each artist's face round in its square. */
  ul.grid {
    grid-template-columns: repeat(auto-fill, minmax(var(--min), 1fr));
  }

  ul.grid li {
    contain-intrinsic-size: auto 240px;
  }

  .face.round {
    width: 100%;
    height: auto;
    aspect-ratio: 1;
  }

  ul.grid .tick {
    background: color-mix(in srgb, var(--surface) 80%, transparent);
    color: var(--text);
  }

  ul.grid .tick.on {
    background: var(--accent);
    color: var(--accent-text);
  }

  /* The table (decision 0042): one artist a line, the lines touching, a smaller face in front. */
  ul.table {
    display: block;
  }

  ul.table li {
    contain-intrinsic-size: auto 60px;
  }

  .face {
    display: block;
    width: 64px;
    height: 64px;
    overflow: hidden;
    border-radius: 50%;
  }

  .table .face,
  .face-space {
    width: 48px;
    height: 48px;
    margin-right: 6px;
  }

  .face-space {
    display: block;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }
</style>
