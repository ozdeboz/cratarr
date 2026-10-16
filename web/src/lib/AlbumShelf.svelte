<script lang="ts">
  // An album list (Incoming, Collection), filtered by the pill's search: each server's albums in columns as the playlist chips are,
  // in the order, layout and size chosen in the pill's View (lib/ViewSheet, decided 2026-10-15). Rows are extra large rows (Row),
  // the grid is tiles (Tile). A tap views the album in the panel, its cover's play icon plays it, a row's ⋮ opens its
  // menu (the page's own, `onmenu`, else View and Play). Select (decided 2026-10-16, as the panel's track lists and
  // Playlists): the list header's icon or a long-press ticks albums, a tap then ticks too, and the select bar
  // (lib/SelectBar) plays, puts next, queues, favours or downloads the ticked ones, one album after another, or opens
  // the menu of one (a tile's menu is this way).
  import type { Snippet } from 'svelte';
  import { fromAction } from 'svelte/attachments';
  import { albumCoverUrl, setAlbumFavorite, type ListedAlbum, type ShelfSource } from './api';
  import { addedOn, albumPlay, playAlbums, playListedAlbum, quality, tracksOfAlbums, weak } from './albums.svelte';
  import { download, inApp } from './app.svelte';
  import { isFavorite, loadFavorites, setFavorite } from './favourites.svelte';
  import SelectBar from './SelectBar.svelte';
  import { toast } from './toast.svelte';
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
  import { libraryShown } from './sources.svelte';
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
    /** The page's own menu for an album, at `anchor`; true when it opened one (else the list's own opens). */
    onmenu?: (source: ShelfSource, a: ListedAlbum, anchor: DOMRect) => boolean;
  } = $props();

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
    const q = pageView.query.trim().toLowerCase();
    return !q || `${a.artist ?? ''} ${a.title}`.toLowerCase().includes(q);
  };
  // The libraries switched off in the filter leave their albums out; the pill's funnel says so while one is.
  $effect(() => {
    pageView.narrowed = sources.some((s) => (s.libraries ?? []).some((l) => !libraryShown(s.id, l)));
    return () => (pageView.narrowed = false);
  });
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
      const albums = s.albums.filter((a) => (!a.library || libraryShown(s.id, a.library)) && matches(a)).sort(COMPARE[order]);
      return { s, albums: down ? albums.reverse() : albums };
    }),
  );

  const view = (source: string, a: ListedAlbum) => viewPlaylist(albumSubject(source, a.id));
  // A tile shows its title and artist; the value the list is ordered by, when it is neither, is a pill over the cover's
  // upper left corner (decided 2026-10-16).
  const axisOf = (a: ListedAlbum) => (order === 'year' ? (a.year ? String(a.year) : null) : order === 'added' && a.added ? addedOn(a.added) : null);
  const openMenu = (s: ShelfSource, a: ListedAlbum, anchor: DOMRect) => {
    if (!onmenu?.(s, a, anchor)) menu = { source: s, a, anchor };
  };

  // Select: the ticked albums by `source/id`; null while not selecting.
  let selecting = $state<Set<string> | null>(null);
  const keyOf = (s: ShelfSource, a: ListedAlbum) => `${s.id}/${a.id}`;
  function startSelect(s?: ShelfSource, a?: ListedAlbum) {
    navigator.vibrate?.(8);
    selecting = new Set(s && a ? [keyOf(s, a)] : []);
  }
  function tick(s: ShelfSource, a: ListedAlbum) {
    if (!selecting) return;
    const next = new Set(selecting);
    const k = keyOf(s, a);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    selecting = next;
  }
  const all = $derived(shown.flatMap(({ s, albums }) => albums.map((a) => ({ s, a }))));
  const ticked = $derived(selecting ? all.filter((x) => selecting!.has(keyOf(x.s, x.a))) : []);
  const tickedFav = $derived(ticked.length > 0 && ticked.every((x) => isFavorite(x.s.id, x.a.id)));
  // The ticked albums' hearts, for the bar's.
  $effect(() => {
    for (const s of sources) {
      const ids = ticked.filter((x) => x.s.id === s.id).map((x) => x.a.id);
      if (ids.length) loadFavorites(s.id, ids);
    }
  });
  // Esc ends it (while no menu of its own is open).
  $effect(() => {
    if (!selecting) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && !menu && (selecting = null);
    addEventListener('keydown', esc);
    return () => removeEventListener('keydown', esc);
  });
  const longPress = (s: ShelfSource, a: ListedAlbum) => fromAction(longpress, () => () => (selecting ? tick(s, a) : startSelect(s, a)));

  const listOf = () => ticked.map((x) => ({ source: x.s.id, a: x.a }));
  async function playTicked(mode: 'play' | 'next' | 'last') {
    const list = listOf();
    if (!list.length) return;
    navigator.vibrate?.(8);
    selecting = null;
    try {
      await playAlbums(list, mode);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
  /** The heart on the ticked albums: all favourites, or none when all already are. Shown at once, back on a refusal. */
  async function favouriteTicked() {
    const list = [...ticked];
    const on = !tickedFav;
    navigator.vibrate?.(8);
    selecting = null;
    for (const x of list) setFavorite(x.s.id, x.a.id, on);
    const failed = (await Promise.allSettled(list.map((x) => setAlbumFavorite(x.s.id, x.a.id, on)))).flatMap((r, n) => (r.status === 'rejected' ? [list[n]] : []));
    for (const x of failed) setFavorite(x.s.id, x.a.id, !on);
    if (failed.length) toast.fail(null, "The favourites didn't all change");
  }
  async function downloadTicked() {
    const list = listOf();
    selecting = null;
    try {
      download(await tracksOfAlbums(list));
    } catch (e) {
      toast.fail(e, "The albums couldn't be downloaded");
    }
  }
  /** The one ticked album's menu (a tile's menu is this way). */
  function menuOfTicked(e: MouseEvent) {
    const x = ticked[0];
    if (!x) return;
    const anchor = (e.currentTarget as HTMLElement).getBoundingClientRect();
    selecting = null;
    openMenu(x.s, x.a, anchor);
  }
</script>

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
      <ListHead
        fields={ALBUM_ORDERS}
        bind:value={order}
        bind:down
        count={howMany(albums.length, 'album')}
        selecting={!!selecting}
        onselect={() => (selecting ? (selecting = null) : startSelect())}
      />
      <ul class={prefs.albumLayout} style:--min="{ALBUM_WIDTHS[prefs.albumLayout][prefs.albumZoom]}px">
        {#each albums as a (a.id)}
          {@const here = playing?.album_id === a.id}
          {@const viewed = panel.viewed === albumSubject(s.id, a.id)}
          {@const on = !!selecting && selecting.has(keyOf(s, a))}
          <li class:playing={here} class:ticked={on}>
            {#if prefs.albumLayout === 'grid'}
              <Tile
                title={a.title}
                tag={axisOf(a)}
                label={selecting ? `${on ? 'Untick' : 'Tick'} ${a.title}` : `View ${a.title}`}
                active={viewed || on}
                strong={here}
                onclick={() => (selecting ? tick(s, a) : view(s.id, a))}
                attach={longPress(s, a)}
              >
                {#snippet image()}{@render cover(s, a)}{/snippet}
                {#snippet sub()}
                  <span class="line">{a.artist ?? 'Unknown artist'}</span>
                  {@render extra?.(s, a)}
                {/snippet}
                {#snippet action()}{#if selecting}{@render tickOf(s, a)}{:else}{@render play(s, a)}{/if}{/snippet}
              </Tile>
            {:else}
              <Row
                size="xl"
                title={a.title}
                label={selecting ? `${on ? 'Untick' : 'Tick'} ${a.title}` : `View ${a.title}`}
                active={viewed}
                onclick={() => (selecting ? tick(s, a) : view(s.id, a))}
                attach={longPress(s, a)}
              >
                {#snippet front()}{@render playCover(s, a)}{/snippet}
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
                {#snippet action()}
                  {#if selecting}{@render tickOf(s, a)}{:else}
                    <button class="icon" aria-label="Menu of {a.title}" title="Menu" onclick={(e) => openMenu(s, a, e.currentTarget.getBoundingClientRect())}
                      ><Icon name="moreVert" size={22} /></button
                    >
                  {/if}
                {/snippet}
              </Row>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/each}
  {#if selecting}
    <SelectBar label="Selected albums" count="{ticked.length}/{all.length}" onclose={() => (selecting = null)} place="page">
      <button aria-label="Play" title="Play" disabled={!ticked.length} onclick={() => playTicked('play')}><Icon name="play" size={24} /></button>
      <button aria-label="Play next" title="Play next" disabled={!ticked.length} onclick={() => playTicked('next')}><Icon name="playNext" size={24} /></button>
      <button aria-label="Add to queue" title="Add to queue" disabled={!ticked.length} onclick={() => playTicked('last')}><Icon name="listAdd" size={24} /></button>
      <button class:fav={tickedFav} aria-pressed={tickedFav} aria-label="Favourite" title="Favourite" disabled={!ticked.length} onclick={favouriteTicked}
        ><Icon name="heart" size={24} /></button
      >
      {#if inApp}
        <button aria-label="Download" title="Download" disabled={!ticked.length} onclick={downloadTicked}><Icon name="arrowDownToLine" size={24} /></button>
      {/if}
      <button aria-label="Menu of the ticked album" title="More" disabled={ticked.length !== 1} onclick={menuOfTicked}><Icon name="moreVert" size={24} /></button>
    </SelectBar>
  {/if}
</div>

<!-- An album's tick while selecting: at a row's end, over a tile's corner. -->
{#snippet tickOf(s: ShelfSource, a: ListedAlbum)}
  {@const on = !!selecting && selecting.has(keyOf(s, a))}
  <button class="icon tick" class:on role="checkbox" aria-checked={on} aria-label="Select {a.title}" onclick={() => tick(s, a)}
    ><Icon name={on ? 'circleCheck' : 'circle'} size={24} /></button
  >
{/snippet}

{#snippet cover(s: ShelfSource, a: ListedAlbum)}
  {#if noCover[a.id]}
    <span class="none" aria-hidden="true"></span>
  {:else}
    <img src={albumCoverUrl(s.id, a.id)} alt="" loading="lazy" decoding="async" onerror={() => (noCover[a.id] = true)} />
  {/if}
{/snippet}

<!-- A row's cover plays the album (decided 2026-10-16): wherever a cover plays, the play icon is in its lower right
     corner, as on a tile's. -->
{#snippet playCover(s: ShelfSource, a: ListedAlbum)}
  {#if selecting}
    <button class="cover plays" aria-label="Select {a.title}" title="Select" onclick={() => tick(s, a)}>{@render cover(s, a)}</button>
  {:else}
    <button
      class="cover plays"
      aria-label="Play {a.title}"
      title="Play"
      disabled={albumPlay.starting !== null}
      aria-busy={albumPlay.starting === a.id}
      onclick={() => playListedAlbum(s.id, a)}
      >{@render cover(s, a)}<span class="corner" aria-hidden="true"><Icon name="play" size={14} /></span></button
    >
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

  /* A ticked album: a row lighter across its whole width (a tile has the border); the tick in the accent. */
  li.ticked {
    --row-bg: color-mix(in srgb, var(--text) 10%, transparent);
  }

  .tick :global(svg) {
    opacity: 0.7;
  }

  .tick.on {
    color: var(--accent);
  }

  .tick.on :global(svg) {
    opacity: 1;
  }

  /* A tile's tick over its corner, where Play is otherwise: round, on the cover. */
  ul.grid .tick {
    border-radius: 50%;
    background: color-mix(in srgb, var(--surface) 80%, transparent);
    color: var(--text);
  }

  ul.grid .tick.on {
    background: var(--accent);
    color: var(--accent-text);
  }

  /* The cover that plays: a button, its play icon a small round in the accent in the lower right corner (the tile's
     Play, smaller). */
  .cover.plays {
    position: relative;
    padding: 0;
    border: 0;
    cursor: pointer;
  }

  .cover.plays:disabled {
    cursor: default;
  }

  .cover.plays .corner {
    position: absolute;
    right: 4px;
    bottom: 4px;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.4);
  }

  .cover > :not(.corner),
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
