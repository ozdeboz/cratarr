<script lang="ts">
  // An album list (Incoming, Collection): the filter, then each server's albums in columns as the playlist chips are,
  // in the order, layout and size chosen in the pill's View (lib/ViewSheet, decided 2026-10-15). Rows are extra large rows (Row),
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
  import ListHead from './ListHead.svelte';
  import { ALBUM_ORDERS, ALBUM_WIDTHS, howMany, pageView } from './pageView.svelte';
  import Tile from './Tile.svelte';

  let {
    sources,
    empty,
    order = $bindable(),
    down = $bindable(false),
    extra,
    onmenu,
  }: {
    sources: ShelfSource[];
    /** Said for a server without albums. */
    empty: string;
    /** The order the viewer chose (the list's header, or View), and turned round. */
    order: CollectionOrder;
    down?: boolean;
    /** More lines about an album, the page's own (Incoming: its conversion, how it relates to Music). */
    extra?: Snippet<[ShelfSource, ListedAlbum]>;
    /** The page's own menu for an album, at `anchor`. */
    onmenu?: (source: ShelfSource, a: ListedAlbum, anchor: DOMRect) => void;
  } = $props();

  let filter = $state('');
  let width = $state(0);
  // View's size steps depend on the width the lists have.
  $effect(() => {
    pageView.width = width;
  });
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
    // Ascending as named (oldest first); the header's arrow turns it round.
    year: (x, y) => (x.year ?? -1) - (y.year ?? -1) || byArtist(x, y) || byTitle(x, y),
    added: (x, y) => (x.added ?? 0) - (y.added ?? 0),
  };
  const shown = $derived(
    sources.map((s) => {
      const albums = s.albums.filter(matches).sort(COMPARE[order]);
      return { s, albums: down ? albums.reverse() : albums };
    }),
  );

  const view = (source: string, a: ListedAlbum) => viewPlaylist(albumSubject(source, a.id));
  const longPress = (s: ShelfSource, a: ListedAlbum) =>
    fromAction(longpress, () => (anchor: DOMRect) => (onmenu ? onmenu(s, a, anchor) : (menu = { source: s, a, anchor })));
</script>

<div class="tools">
  <input class="filter" type="search" placeholder="Filter albums" aria-label="Filter albums" bind:value={filter} />
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
      <ListHead fields={ALBUM_ORDERS} bind:value={order} bind:down count={howMany(albums.length, 'album')} />
      <ul class={prefs.albumLayout} style:--min="{ALBUM_WIDTHS[prefs.albumLayout][prefs.albumZoom]}px">
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
