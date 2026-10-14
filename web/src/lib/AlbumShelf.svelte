<script lang="ts">
  // An album list (Incoming, Collection): the filter, the order (when the page offers one), rows or a grid of covers,
  // and the size, over each server's albums in columns as the playlist chips are. Rows are extra large rows (Row),
  // the grid is tiles (Tile). A tap views the album in the panel, Play plays it, a long-press opens its menu: the
  // page's own (`onmenu`), else View and Play.
  import type { Snippet } from 'svelte';
  import { fromAction } from 'svelte/attachments';
  import { albumCoverUrl, type ListedAlbum, type ShelfSource } from './api';
  import { addedOn, albumPlay, playListedAlbum, quality, weak } from './albums.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { albumSubject, panel, viewPlaylist } from './panel.svelte';
  import { prefs, type AlbumLayout, type CollectionOrder } from './prefs.svelte';
  import { playing as nowPlaying } from './remote.svelte';
  import Row from './Row.svelte';
  import Seg from './Seg.svelte';
  import SizeControl from './SizeControl.svelte';
  import Tile from './Tile.svelte';

  let {
    sources,
    empty,
    order,
    extra,
    onmenu,
  }: {
    sources: ShelfSource[];
    /** Said for a server without albums. */
    empty: string;
    /** The order the viewer chose; without one, the lists stay as the server sends them (newest first). */
    order?: CollectionOrder;
    /** More lines about an album, the page's own (Incoming: its conversion, how it relates to Music). */
    extra?: Snippet<[ShelfSource, ListedAlbum]>;
    /** The page's own menu for an album, at `anchor`. */
    onmenu?: (source: ShelfSource, a: ListedAlbum, anchor: DOMRect) => void;
  } = $props();

  // An item's least width per size step (prefs.ZOOM_STEPS): rows need room for their lines, covers less.
  const WIDTHS: Record<AlbumLayout, readonly number[]> = {
    rows: [240, 280, 340, 420, 520, 680],
    grid: [110, 140, 170, 210, 260, 340],
  };
  const LAYOUTS = [
    { value: 'rows' as const, label: 'Rows', icon: 'list' as const },
    { value: 'grid' as const, label: 'Covers', icon: 'layoutGrid' as const },
  ];
  const ORDERS = [
    { value: 'artist' as const, label: 'Artist' },
    { value: 'title' as const, label: 'Title' },
    { value: 'year' as const, label: 'Year' },
    { value: 'added' as const, label: 'Added', title: 'Recently added first' },
  ];

  let filter = $state('');
  let width = $state(0);
  let noCover = $state<Record<string, boolean>>({});
  // What is playing (the shell polls it), so the album being listened to stands out.
  const playing = $derived(nowPlaying());
  let menu = $state<{ source: ShelfSource; a: ListedAlbum; anchor: DOMRect } | null>(null);

  const matches = (a: ListedAlbum) => {
    const q = filter.trim().toLowerCase();
    return !q || `${a.artist ?? ''} ${a.title}`.toLowerCase().includes(q);
  };
  // Artists without a leading "The", numbers in number order, accents and case aside.
  const collator = new Intl.Collator(undefined, { sensitivity: 'base', numeric: true });
  const name = (s: string | null) => (s ?? '').replace(/^the\s+/i, '');
  const byArtist = (x: ListedAlbum, y: ListedAlbum) => collator.compare(name(x.artist), name(y.artist)) || (x.year ?? 0) - (y.year ?? 0);
  const byTitle = (x: ListedAlbum, y: ListedAlbum) => collator.compare(x.title, y.title);
  const COMPARE: Record<CollectionOrder, (x: ListedAlbum, y: ListedAlbum) => number> = {
    artist: (x, y) => byArtist(x, y) || byTitle(x, y),
    title: (x, y) => byTitle(x, y) || byArtist(x, y),
    year: (x, y) => (y.year ?? -1) - (x.year ?? -1) || byArtist(x, y) || byTitle(x, y),
    added: (x, y) => (y.added ?? 0) - (x.added ?? 0),
  };
  const shown = $derived(
    sources.map((s) => {
      const albums = s.albums.filter(matches);
      return { s, albums: order ? albums.sort(COMPARE[order]) : albums };
    }),
  );

  const view = (source: string, a: ListedAlbum) => viewPlaylist(albumSubject(source, a.id));
  const longPress = (s: ShelfSource, a: ListedAlbum) =>
    fromAction(longpress, () => (anchor: DOMRect) => (onmenu ? onmenu(s, a, anchor) : (menu = { source: s, a, anchor })));
</script>

<div class="tools">
  <input class="filter" type="search" placeholder="Filter albums" aria-label="Filter albums" bind:value={filter} />
  {#if order}<Seg label="Order" options={ORDERS} bind:value={prefs.collectionOrder} />{/if}
  <Seg label="Layout" options={LAYOUTS} bind:value={prefs.albumLayout} />
  <SizeControl widths={WIDTHS[prefs.albumLayout]} {width} bind:value={prefs.albumZoom} />
</div>

<div class="lists" bind:clientWidth={width}>
  {#each shown as { s, albums } (s.id)}
    {#if sources.length > 1}<h2>{s.label}</h2>{/if}
    {#if !s.supported}
      <p class="empty">{s.label} can't list albums.</p>
    {:else if s.error && s.albums.length === 0}
      <p class="error" role="alert">{s.error}</p>
    {:else if s.albums.length === 0}
      <p class="empty">{empty}</p>
    {:else}
      <p class="count">{s.albums.length} {s.albums.length === 1 ? 'album' : 'albums'}{order ? '.' : ', newest first.'}</p>
      <ul class={prefs.albumLayout} style:--min="{WIDTHS[prefs.albumLayout][prefs.albumZoom]}px">
        {#each albums as a (a.id)}
          {@const here = playing?.album_id === a.id}
          {@const viewed = panel.viewed === albumSubject(s.id, a.id)}
          <li class:playing={here}>
            {#if prefs.albumLayout === 'grid'}
              <Tile title={a.title} label="View {a.title}" active={viewed} strong={here} onclick={() => view(s.id, a)} attach={longPress(s, a)}>
                {#snippet image()}{@render cover(s, a)}{/snippet}
                {#snippet sub()}
                  <span class="line">{a.artist ?? 'Unknown artist'}</span>
                  {#if a.year}<span class="line">{a.year}</span>{/if}
                  {@render extra?.(s, a)}
                {/snippet}
                {#snippet action()}{@render play(s, a)}{/snippet}
              </Tile>
            {:else}
              <Row size="xl" title={a.title} label="View {a.title}" active={viewed} onclick={() => view(s.id, a)} attach={longPress(s, a)}>
                {#snippet lead()}<span class="cover">{@render cover(s, a)}</span>{/snippet}
                {#snippet badges()}
                  {#if a.year}<span class="released">{a.year}</span>{/if}
                {/snippet}
                {#snippet sub()}
                  <span class="artist">{a.artist ?? 'Unknown artist'}{#if here}<span class="now"> · playing</span>{/if}</span>
                  <span class="facts">
                    <span>{a.tracks} {a.tracks === 1 ? 'track' : 'tracks'}</span>
                    <span class:weak={weak(a)}>{quality(a)}</span>
                    {#if a.added}<span>added {addedOn(a.added)}</span>{/if}
                    {#each a.tags as t (t)}<span class="tag">{t}</span>{/each}
                  </span>
                  {@render extra?.(s, a)}
                {/snippet}
                {#snippet action()}{@render play(s, a)}{/snippet}
              </Row>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/each}
</div>

{#snippet cover(s: ShelfSource, a: ListedAlbum)}
  {#if noCover[a.id]}
    <span class="none" aria-hidden="true"></span>
  {:else}
    <img src={albumCoverUrl(s.id, a.id)} alt="" loading="lazy" decoding="async" onerror={() => (noCover[a.id] = true)} />
  {/if}
{/snippet}

{#snippet play(s: ShelfSource, a: ListedAlbum)}
  <button
    class="icon"
    aria-label="Play {a.title}"
    disabled={albumPlay.starting !== null}
    aria-busy={albumPlay.starting === a.id}
    onclick={() => playListedAlbum(s.id, a)}><Icon name="play" size={26} /></button
  >
{/snippet}

{#if menu}
  {@const m = menu}
  <!-- The album's menu: a sheet as every menu (decision 0021). -->
  <MenuSheet
    label={m.a.title}
    onclose={() => (menu = null)}
    images={[albumCoverUrl(m.source.id, m.a.id)]}
    title={m.a.title}
    chips={[m.a.artist ?? 'Unknown artist']}
    actions={[
      {
        icon: 'play',
        label: 'Play',
        onclick: () => {
          menu = null;
          playListedAlbum(m.source.id, m.a);
        },
      },
    ]}
  >
    <div role="menu">
      <MenuEntry
        icon="squareArrowOutUpRight"
        label="View"
        onclick={() => {
          menu = null;
          view(m.source.id, m.a);
        }}
      />
    </div>
  </MenuSheet>
{/if}

<style>
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin: 8px 0 4px;
  }

  .filter {
    flex: 1 1 220px;
    min-width: 0;
    max-width: 360px;
    min-height: 44px;
    padding: 0 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: var(--text);
  }

  h2 {
    margin: 24px 0 6px;
    font-size: 1rem;
  }

  .empty,
  .count {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .error {
    color: var(--danger);
  }

  /* In columns, as the playlist chips are (lib/zoom.ts). The browser skips laying out what is off screen. */
  ul {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--min), 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    min-width: 0;
    content-visibility: auto;
  }

  ul.rows li {
    contain-intrinsic-size: auto 96px;
  }

  ul.grid li {
    contain-intrinsic-size: auto calc(var(--min) + 60px);
  }

  .cover {
    flex: none;
    display: block;
    width: 72px;
    height: 72px;
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface);
  }

  .cover > *,
  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .none {
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: inherit;
  }

  .released,
  .artist {
    color: var(--muted);
    font-weight: 400;
  }

  .artist {
    display: block;
    font-size: 0.9rem;
  }

  .line {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin-top: 2px;
    color: var(--muted);
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
  }

  .now {
    color: var(--accent);
    font-weight: 600;
  }

  .weak {
    color: var(--danger);
  }

  .tag {
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
  }

  ul.rows li.playing {
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  /* The play button's busy state while its album starts. */
  li :global(button[aria-busy='true']) {
    opacity: 0.6;
  }
</style>
