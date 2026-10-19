<script lang="ts">
  // An album list (Incoming, Collection), filtered by the pill's search: each server's albums in columns as the playlist chips are,
  // in the order, layout and size chosen in the pill's View (lib/ViewSheet, decided 2026-10-15). Rows are extra large rows (Row),
  // the grid is tiles (Tile), the table one album a line with its columns where there is room (decision 0042). A tap views the album in the panel, its cover's play icon plays it, a row's ⋮ opens its
  // menu (lib/AlbumMenu, the page's: `onmenu`). Select (decided 2026-10-16, as the panel's track lists and
  // Playlists): the list header's icon or a long-press ticks albums, a tap then ticks too, and the select bar
  // (lib/SelectBar) plays, puts next, queues, favours or downloads the ticked ones, one album after another, or opens
  // the menu of one (a tile's menu is this way).
  import { untrack, type Snippet } from 'svelte';
  import { fromAction } from 'svelte/attachments';
  import { albumCoverUrl, setAlbumFavorite, type ListedAlbum, type ShelfSource } from './api';
  import Cover from './Cover.svelte';
  import { addedOn, albumPlay, loadAlbumUses, playAlbums, playListedAlbum, quality, tracksOfAlbums, useOf, weak } from './albums.svelte';
  import { download, inApp } from './app.svelte';
  import { isFavorite, loadAlbumFavorites, setFavorite } from './favourites.svelte';
  import SelectBar from './SelectBar.svelte';
  import { toast } from './toast.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import SelectSheet from './SelectSheet.svelte';
  import { albumSubject, panel, viewPlaylist } from './panel.svelte';
  import { prefs, type AlbumLayout, type CollectionOrder } from './prefs.svelte';
  import { playing as nowPlaying } from './remote.svelte';
  import Row from './Row.svelte';
  import ListHead from './ListHead.svelte';
  import { ALBUM_ORDERS, ALBUM_WIDTHS, howMany, pageView, USE_ORDERS } from './pageView.svelte';
  import { libraryShown } from './sources.svelte';
  import Tile from './Tile.svelte';
  import type { Cell } from './Row.svelte';
  import { fit } from './columns.svelte';

  let {
    sources,
    empty,
    order = $bindable(),
    down = $bindable(false),
    extra,
    onmenu,
    onplay,
    actions,
    countMore,
  }: {
    sources: ShelfSource[];
    /** Said for a server without albums. */
    empty: string;
    /** The order the viewer chose (the list's header, or View), and turned round. */
    order: CollectionOrder;
    down?: boolean;
    /** More lines about an album, the page's own (Incoming: its conversion, how it relates to Music). */
    extra?: Snippet<[ShelfSource, ListedAlbum]>;
    /** An album's menu (lib/AlbumMenu, the page's), from its ⋮ or the select bar's More. */
    onmenu: (source: ShelfSource, a: ListedAlbum, anchor: DOMRect) => void;
    /** A cover's Play, the page's own (Downloads: from the device); else the album plays as the Library's do. */
    onplay?: (source: ShelfSource, a: ListedAlbum) => void;
    /** The select bar's buttons, the page's own: the ticked albums, and the end of selecting. */
    actions?: Snippet<[{ s: ShelfSource; a: ListedAlbum }[], () => void]>;
    /** After the list header's count ("12 albums · 1.2 GB"). */
    countMore?: string;
  } = $props();
  const playOf = (s: ShelfSource, a: ListedAlbum) => (onplay ? onplay(s, a) : playListedAlbum(s.id, a));

  let width = $state(0);
  // View's size steps depend on the width the lists have.
  $effect(() => {
    pageView.width = width;
  });

  // Only what is near is made (2026-10-16: the whole Library at once hung the app). Each server's albums are cut into
  // blocks of whole grid rows, each its own grid, so the columns line up from block to block. A block is made while it
  // is within a screen of what shows, else it is an empty box as tall as it was when last made (an estimate before
  // that), so the page keeps its length and the scroll does not jump. Watched against the box that scrolls (a margin
  // reaches no further than the box that clips it).
  const table = $derived(prefs.albumLayout === 'table');
  // A table's lines touch, as a table's do.
  const GAP = $derived(table ? 0 : 10);
  const ROWS_PER_BLOCK = 8;
  const minPx = $derived(table ? width : ALBUM_WIDTHS[prefs.albumLayout][prefs.albumZoom]);
  const cols = $derived(table ? 1 : Math.max(1, Math.floor((width + GAP) / (minPx + GAP))));
  const perBlock = $derived(cols * ROWS_PER_BLOCK);
  // An album's library as a dot in its colour (decision 0034): none for the primary (Music), and none while only one
  // of the server's libraries shows (every album would carry the same).
  const dotOf = (s: ShelfSource, a: ListedAlbum) => {
    const info = s.library_info;
    if (!info || !a.library || info.filter((l) => libraryShown(s.id, l.id)).length < 2) return null;
    const l = info.find((x) => x.id === a.library);
    return l?.colour ? { colour: l.colour, name: l.name } : null;
  };
  const blocksOf = (albums: ListedAlbum[]) => {
    const out: ListedAlbum[][] = [];
    for (let i = 0; i < albums.length; i += perBlock) out.push(albums.slice(i, i + perBlock));
    return out;
  };
  /** Blocks within reach (made), and each block's height and rows when last made, by key. */
  let near = $state<Record<string, boolean>>({});
  let heights = $state<Record<string, number>>({});
  const rowsOf: Record<string, number> = {};
  /** A grid row's height as the blocks made so far measure it, else a row's usual height (a tile: its column's width
   *  and two lines), for the blocks not made yet. */
  const rowHeight = $derived.by(() => {
    let h = 0;
    let n = 0;
    const tail = `:${perBlock}:${prefs.albumLayout}:`;
    for (const [key, height] of Object.entries(heights)) {
      if (!key.includes(tail) || !rowsOf[key]) continue;
      h += height - (rowsOf[key] - 1) * GAP;
      n += rowsOf[key];
    }
    if (n) return h / n;
    return prefs.albumLayout === 'grid' ? (width - GAP * (cols - 1)) / cols + 64 : table ? 64 : 90;
  });
  const estimate = (n: number) => {
    const rows = Math.ceil(n / cols);
    return rows * rowHeight + (rows - 1) * GAP;
  };
  const scrollerOf = (node: Element): Element | null => {
    for (let el = node.parentElement; el; el = el.parentElement) {
      const o = getComputedStyle(el).overflowY;
      if (o === 'auto' || o === 'scroll') return el;
    }
    return null;
  };
  let seen: IntersectionObserver | null = null;
  const sized = new ResizeObserver((entries) => {
    for (const e of entries) {
      const key = (e.target as HTMLElement).dataset.key!;
      if (near[key] && e.contentRect.height > 0) heights[key] = e.contentRect.height;
    }
  });
  /** Watches a block: made while near, measured while made. */
  const watch = (key: string, rows: number) => (node: HTMLElement) => {
    rowsOf[key] = rows;
    seen ??= new IntersectionObserver(
      (entries) => {
        for (const e of entries) near[(e.target as HTMLElement).dataset.key!] = e.isIntersecting;
      },
      { root: scrollerOf(node), rootMargin: '100% 0px' },
    );
    node.dataset.key = key;
    seen.observe(node);
    sized.observe(node);
    return () => {
      seen?.unobserve(node);
      sized.unobserve(node);
    };
  };
  $effect(() => () => {
    seen?.disconnect();
    sized.disconnect();
  });
  // What is playing (the shell polls it), so the album being listened to stands out.
  const playing = $derived(nowPlaying());

  const matches = (a: ListedAlbum) => {
    const q = pageView.query.trim().toLowerCase();
    if (!q) return true;
    return `${a.artist ?? ''} ${a.title}`.toLowerCase().includes(q);
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
  // An order of one server's albums: how you used them (plays, last played, rating) is per server.
  const compareIn = (source: string): Record<CollectionOrder, (x: ListedAlbum, y: ListedAlbum) => number> => {
    const use = (a: ListedAlbum) => useOf(source, a.id);
    return {
      artist: (x, y) => byArtist(x, y) || byTitle(x, y),
      title: (x, y) => byTitle(x, y) || byArtist(x, y),
      // Ascending as named (oldest first); the header's arrow turns it round.
      year: (x, y) => (x.year ?? -1) - (y.year ?? -1) || byArtist(x, y) || byTitle(x, y),
      added: (x, y) => (x.added ?? 0) - (y.added ?? 0),
      plays: (x, y) => use(x).plays - use(y).plays || (use(x).played ?? 0) - (use(y).played ?? 0),
      played: (x, y) => (use(x).played ?? 0) - (use(y).played ?? 0),
      rating: (x, y) => use(x).rating - use(y).rating || use(x).plays - use(y).plays,
    };
  };
  const shown = $derived(
    sources.map((s) => {
      const albums = s.albums.filter((a) => (!a.library || libraryShown(s.id, a.library)) && matches(a)).sort(compareIn(s.id)[order]);
      return { s, albums: down ? albums.reverse() : albums };
    }),
  );
  // How you used them, read when such an order is chosen (and each time again: plays change).
  $effect(() => {
    if (!USE_ORDERS.includes(order)) return;
    for (const s of untrack(() => sources)) loadAlbumUses(s.id);
  });
  // Plays, Last played and Rating start most first; the arrow still turns them round.
  let orderWas = untrack(() => order);
  $effect(() => {
    const o = order;
    if (o === orderWas) return;
    orderWas = o;
    if (USE_ORDERS.includes(o)) down = true;
  });

  const view = (source: string, a: ListedAlbum) => viewPlaylist(albumSubject(source, a.id));
  // A tile shows its title and artist; the value the list is ordered by, when it is neither, is a pill over the cover's
  // upper left corner (decided 2026-10-16).
  const axisOf = (s: ShelfSource, a: ListedAlbum) => {
    if (order === 'year') return a.year ? String(a.year) : null;
    if (order === 'added') return a.added ? addedOn(a.added) : null;
    const u = useOf(s.id, a.id);
    if (order === 'plays') return u.plays ? (u.plays === 1 ? '1 play' : `${u.plays} plays`) : null;
    if (order === 'played') return u.played ? addedOn(u.played) : null;
    if (order === 'rating') return u.rating ? '★'.repeat(u.rating) : null;
    return null;
  };
  // A row ordered by how you used them says the value where it says when the album came (a tile's pill says it too).
  const usedLine = (s: ShelfSource, a: ListedAlbum) => {
    const v = USE_ORDERS.includes(order) ? axisOf(s, a) : null;
    return v && order === 'played' ? `played ${v}` : v;
  };
  const openMenu = (s: ShelfSource, a: ListedAlbum, anchor: DOMRect) => onmenu(s, a, anchor);

  // The table's columns (decision 0042): as many as the list's width allows, in the owner's order (View).
  const tableCols = $derived(table ? fit('albums', width) : []);
  // The ones that order the list, by the order they stand for; Plays, dates and Rating start most and newest first.
  const ORDER_OF: Partial<Record<string, CollectionOrder>> = { year: 'year', added: 'added', plays: 'plays', lastPlayed: 'played', rating: 'rating' };
  // How you used them, read while a column shows it.
  $effect(() => {
    if (!tableCols.some((c) => c.id === 'plays' || c.id === 'lastPlayed' || c.id === 'rating')) return;
    for (const s of untrack(() => sources)) loadAlbumUses(s.id);
  });
  function cellText(s: ShelfSource, a: ListedAlbum, id: string): string {
    const u = id === 'plays' || id === 'lastPlayed' || id === 'rating' ? useOf(s.id, a.id) : null;
    if (id === 'year') return a.year ? String(a.year) : '';
    if (id === 'tracks') return String(a.tracks);
    if (id === 'added') return a.added ? addedOn(a.added) : '';
    if (id === 'plays') return u?.plays ? u.plays.toLocaleString() : '';
    if (id === 'lastPlayed') return u?.played ? addedOn(u.played) : '';
    if (id === 'format') return quality(a);
    if (id === 'rating') return u?.rating ? '★'.repeat(u.rating) : '';
    return '';
  }
  const cellsOf = (s: ShelfSource, a: ListedAlbum): Cell[] =>
    tableCols.map((c) => ({ ...c, text: cellText(s, a, c.id), warn: c.id === 'format' && weak(a) }));
  const headCells = $derived<Cell[]>(
    tableCols.map((c) => {
      const o = ORDER_OF[c.id];
      return {
        ...c,
        text: c.label,
        on: !!o && o === order,
        onclick: o
          ? () => {
              if (order === o) down = !down;
              else {
                order = o;
                down = o !== 'year';
              }
            }
          : undefined,
      };
    }),
  );

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
  // In select mode a long-press asks what to tick: all, none or the others (lib/SelectSheet).
  let selectSheet = $state(false);
  function selectWhich(which: 'all' | 'none' | 'others') {
    const keys = all.map((x) => keyOf(x.s, x.a));
    selecting = new Set(which === 'all' ? keys : which === 'none' ? [] : keys.filter((k) => !selecting?.has(k)));
  }
  const ticked = $derived(selecting ? all.filter((x) => selecting!.has(keyOf(x.s, x.a))) : []);
  const tickedFav = $derived(ticked.length > 0 && ticked.every((x) => isFavorite(x.s.id, x.a.id)));
  // The albums' hearts (the bar's, the Library's Favourites only): each server's list, asked for when the list shows
  // and again when a selection starts.
  const sourceIds = $derived(sources.map((s) => s.id).join('\n'));
  const choosing = $derived(selecting !== null);
  $effect(() => {
    void choosing;
    for (const id of sourceIds.split('\n')) if (id) loadAlbumFavorites(id);
  });
  // Esc ends it (while no menu of its own is open).
  $effect(() => {
    if (!selecting) return;
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && !document.querySelector('[role="dialog"]') && (selecting = null);
    addEventListener('keydown', esc);
    return () => removeEventListener('keydown', esc);
  });
  const longPress = (s: ShelfSource, a: ListedAlbum) => fromAction(longpress, () => () => (selecting ? (selectSheet = true) : startSelect(s, a)));

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

<div class="lists" class:table bind:clientWidth={width}>
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
        count={`${howMany(albums.length, 'album')}${countMore ? ` · ${countMore}` : ''}`}
        selecting={!!selecting}
        onselect={() => (selecting ? (selecting = null) : startSelect())}
      />
      {#if tableCols.length}
        <!-- The table's column names, a row of the same shape so they line up; a tap on one orders by it. -->
        <Row size="large" as="div" head title="Title" cells={headCells}>
          {#snippet front()}<span class="cover-space"></span>{/snippet}
          {#snippet action()}<span></span>{/snippet}
        </Row>
      {/if}
      {#each blocksOf(albums) as block, bi (`${s.id}:${perBlock}:${prefs.albumLayout}:${bi}`)}
        {@const key = `${s.id}:${perBlock}:${prefs.albumLayout}:${bi}`}
        {@const made = near[key] ?? bi === 0}
        <div class="block" {@attach watch(key, Math.ceil(block.length / cols))} style:height={made ? null : `${heights[key] ?? estimate(block.length)}px`}>
          {#if made}
      <ul class={prefs.albumLayout} style:--min="{minPx}px">
        {#each block as a (a.id)}
          {@const here = playing?.album_id === a.id}
          {@const viewed = panel.viewed === albumSubject(s.id, a.id)}
          {@const on = !!selecting && selecting.has(keyOf(s, a))}
          <li class:playing={here} class:ticked={on}>
            {#if prefs.albumLayout === 'grid'}
              <Tile
                title={a.title}
                dot={dotOf(s, a)}
                tag={axisOf(s, a)}
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
            {:else if table}
              <Row
                size="large"
                title={a.title}
                cells={tableCols.length ? cellsOf(s, a) : undefined}
                label={selecting ? `${on ? 'Untick' : 'Tick'} ${a.title}` : `View ${a.title}`}
                active={viewed}
                onclick={() => (selecting ? tick(s, a) : view(s.id, a))}
                attach={longPress(s, a)}
              >
                {#snippet front()}{@render playCover(s, a)}{/snippet}
                {#snippet badges()}
                  {@const dot = dotOf(s, a)}
                  {#if dot}<i class="dot" style:background={dot.colour} role="img" aria-label={dot.name}></i>{/if}
                {/snippet}
                {#snippet sub()}{a.artist ?? 'Unknown artist'}{#if here}<span class="now"> · playing</span>{/if}{/snippet}
                {#snippet action()}
                  {#if selecting}{@render tickOf(s, a)}{:else}
                    <button class="icon" aria-label="Menu of {a.title}" title="Menu" onclick={(e) => openMenu(s, a, e.currentTarget.getBoundingClientRect())}
                      ><Icon name="moreVert" size={22} /></button
                    >
                  {/if}
                {/snippet}
              </Row>
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
                  {@const dot = dotOf(s, a)}
                  {#if dot}<i class="dot" style:background={dot.colour} role="img" aria-label={dot.name}></i>{/if}
                  {#if a.year}<span class="released">{a.year}</span>{/if}
                {/snippet}
                {#snippet sub()}
                  <span class="artist">{a.artist ?? 'Unknown artist'}{#if here}<span class="now"> · playing</span>{/if}</span>
                  <span class="facts">
                    <span>{a.tracks} {a.tracks === 1 ? 'track' : 'tracks'}</span>
                    <span class:weak={weak(a)}>{quality(a)}</span>
                    {#if usedLine(s, a)}<span>{usedLine(s, a)}</span>{:else if a.added}<span>added {addedOn(a.added)}</span>{/if}
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
        </div>
      {/each}
    {/if}
  {/each}
  {#if selecting}
    <SelectBar label="Selected albums" count="{ticked.length}/{all.length}" onclose={() => (selecting = null)} place="page">
      {#if actions}
        {@render actions(ticked, () => (selecting = null))}
        <button aria-label="Menu of the ticked album" title="More" disabled={ticked.length !== 1} onclick={menuOfTicked}><Icon name="moreVert" size={24} /></button>
      {:else}
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
      {/if}
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

<!-- An album's cover; a favourite's has a small heart in its upper right corner (as Symfonium's lists). -->
{#snippet cover(s: ShelfSource, a: ListedAlbum)}
  <span class="art">
    <Cover src={albumCoverUrl(s.id, a.id)} />
    {#if isFavorite(s.id, a.id)}<span class="fav" role="img" aria-label="Favourite"><Icon name="heart" size={12} /></span>{/if}
  </span>
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
      onclick={() => playOf(s, a)}
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
    onclick={() => playOf(s, a)}><Icon name="play" size={26} /></button
  >
{/snippet}

{#if selectSheet}
  <SelectSheet onall={() => selectWhich('all')} onnone={() => selectWhich('none')} onothers={() => selectWhich('others')} onclose={() => (selectSheet = false)} />
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

  /* In columns, as the playlist chips are (lib/zoom.ts). */
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
  }

  /* A block of whole grid rows (made while near, else an empty box its height); the grid's gap between blocks too. */
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .block + .block {
    margin-top: 10px;
  }

  /* A table (decision 0042): one album a line, the lines touching, a smaller cover in front. */
  ul.table {
    display: block;
  }

  .table .block + .block {
    margin-top: 0;
  }

  ul.table .cover,
  .cover-space {
    width: 48px;
    height: 48px;
    /* As far from the title as a track's cover is (lib/Row's large gap and this make its 10 px). */
    margin-right: 6px;
  }

  .cover-space {
    display: block;
    height: 0;
  }

  ul.table .cover.plays .corner {
    right: 2px;
    bottom: 2px;
    width: 20px;
    height: 20px;
  }

  /* With a mouse, a line's play shows on the line pointed at (a table is many small covers); by touch it stays. */
  @media (hover: hover) {
    ul.table li:not(:hover) .cover.plays:not(:focus-visible) .corner {
      opacity: 0;
    }
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

  .cover > :not(.corner) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .art {
    position: relative;
    display: block;
    width: 100%;
    height: 100%;
  }

  .fav {
    position: absolute;
    top: 4px;
    right: 4px;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: rgb(0 0 0 / 0.55);
    color: var(--accent);
  }

  .fav :global(svg) {
    fill: currentColor;
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

  ul.rows li.playing,
  ul.table li.playing {
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  /* The play button's busy state while its album starts. */
  li :global(button[aria-busy='true']) {
    opacity: 0.6;
  }
</style>
