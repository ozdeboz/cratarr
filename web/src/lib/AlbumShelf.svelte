<script lang="ts">
  // An album list (Incoming, Collection), filtered by the pill's search: each server's albums in columns as the playlist chips are,
  // in the order, layout and size chosen in the pill's View (lib/ViewSheet, decided 2026-10-15). Rows are extra large rows (Row),
  // the grid is tiles (Tile), the table one album a line with its columns where there is room (decision 0042). A tap views the album in the panel, its cover's play icon plays it, a row's ⋮ opens its
  // menu (lib/AlbumMenu, the page's: `onmenu`). Select (decided 2026-10-16, as the panel's track lists and
  // Playlists): the list header's icon or a long-press ticks albums, a tap then ticks too, and the select bar
  // (lib/SelectBar) plays, puts next, queues, favours or downloads the ticked ones, one album after another, or opens
  // the menu of one (a tile's menu is this way); several, their menu with what one album's has (lib/SeveralAlbums).
  import CoverPlay from './CoverPlay.svelte';
  import CoverWait from './CoverWait.svelte';
  import { NearBlocks } from './nearBlocks.svelte';
  import { panelBeside } from './glass.svelte';
  import { untrack, type Snippet } from 'svelte';
  import { fromAction } from 'svelte/attachments';
  import { albumCoverUrl, search, setAlbumFavorite, type ListedAlbum, type ShelfSource } from './api';
  import { askIfMany, fewAtATime } from './many.svelte';
  import { plural } from './insights.svelte';
  import type { Matched } from './search';
  import Cover from './Cover.svelte';
  import FlowStage from './FlowStage.svelte';
  import { libraryLayout } from './layouts';
  import ViewSheet from './ViewSheet.svelte';
  import { addedOn, albumPlay, loadAlbumUses, playAlbums, playListedAlbum, quality, tracksOfAlbums, useOf, weak } from './albums.svelte';
  import { download, inApp } from './app.svelte';
  import { isFavorite, loadAlbumFavorites, setFavorite } from './favourites.svelte';
  import SelectBar from './SelectBar.svelte';
  import { toast } from './toast.svelte';
  import SelectionMenu from './SelectionMenu.svelte';
  import SeveralAlbums, { type Ask } from './SeveralAlbums.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import { playCollection } from './play.svelte';
  import { DEVICE } from './deviceMusic.svelte';
  import type { LocalTrack } from './localPlayer.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import SelectSheet from './SelectSheet.svelte';
  import { albumSubject, panel, viewPlaylist } from './panel.svelte';
  import { leaveSearch } from './globalSearch.svelte';
  import { prefs, type AlbumLayout, type CollectionOrder } from './prefs.svelte';
  import { clockText } from './remote.svelte';
  import { playing as nowPlaying } from './remote.svelte';
  import Row from './Row.svelte';
  import ListHead from './ListHead.svelte';
  import { matchesSearch, ALBUM_ORDERS, ALBUM_WIDTHS, howMany, isLossless, pageView, qualityOf, USE_ORDERS } from './pageView.svelte';
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

  // Only what is near is made (lib/nearBlocks). Each server's albums are cut into blocks of whole grid rows, each its
  // own grid, so the columns line up from block to block.
  /** As chosen in View, as it can be here (lib/layouts: no table on a phone's screen). */
  const layout = $derived(libraryLayout('albums'));
  const table = $derived(layout === 'table');
  // A table's lines touch, as a table's do.
  const GAP = $derived(table ? 0 : 10);
  const ROWS_PER_BLOCK = 8;
  const minPx = $derived(table ? width : ALBUM_WIDTHS[layout][prefs.albumZoom]);
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
  const blocks = new NearBlocks();
  $effect(() => () => blocks.dispose());
  /** A grid row's height as the blocks made so far measure it, else a row's usual height (a tile: its column's width
   *  and two lines), for the blocks not made yet. */
  const rowHeight = $derived.by(() => {
    let h = 0;
    let n = 0;
    const tail = `:${perBlock}:${layout}:`;
    for (const [key, height] of Object.entries(blocks.heights)) {
      if (!key.includes(tail) || !blocks.rowsOf[key]) continue;
      h += height - (blocks.rowsOf[key] - 1) * GAP;
      n += blocks.rowsOf[key];
    }
    if (n) return h / n;
    return layout === 'grid' ? (width - GAP * (cols - 1)) / cols + 64 : table ? 64 : 90;
  });
  const estimate = (n: number) => {
    const rows = Math.ceil(n / cols);
    return rows * rowHeight + (rows - 1) * GAP;
  };
  // What is playing (the shell polls it), so the album being listened to stands out.
  const playing = $derived(nowPlaying());

  // As every search reads (lib/search, decision 0072).
  // Deep (decision 0072, 0.9.3): Cratarr's own albums the server finds by one of their tracks, with why; asked a moment
  // after the words stop changing. The rest as every search reads, in the browser.
  let deep = $state<Map<string, Matched>>(new Map());
  /** The words the server has answered (or failed) for: till then no "none match" (the owner, 2026-10-25: it said so
   *  before the albums found by a track were in). */
  let deepFor = $state('');
  $effect(() => {
    const q = pageView.query.trim();
    if (!q) {
      deep = new Map();
      deepFor = '';
      return;
    }
    let gone = false;
    const t = setTimeout(() => {
      search(q, ['album'], 100).then(
        (r) => {
          if (gone) return;
          const m = new Map<string, Matched>();
          for (const it of r.groups.album?.items ?? []) {
            const on = it.matched.find((x) => x.on === 'track');
            if (on) m.set(it.subject, on);
          }
          deep = m;
          deepFor = q;
        },
        () => !gone && (deepFor = q),
      );
    }, 250);
    return () => {
      gone = true;
      clearTimeout(t);
    };
  });
  /** Why an album found only by a track is there: the track, and how many of its tracks match. */
  const deepOf = (s: ShelfSource, a: ListedAlbum) => (pageView.query.trim() ? deep.get(albumSubject(s.id, a.id)) : undefined);
  // A classical record's composers found by the search too (0.10: "Brahms" finds Arrau's).
  const matches = (s: ShelfSource, a: ListedAlbum) => matchesSearch(a.artist, a.title, ...(a.composers ?? []).map((c) => c.name)) || !!deepOf(s, a);
  /** Its second line: a classical record's performers (0.10: conductor, orchestra, soloists), else its artist. */
  const byline = (a: ListedAlbum) => (a.performers?.length ? a.performers.join(' · ') : (a.artist ?? 'Unknown artist'));
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
      quality: (x, y) => qualityOf(x) - qualityOf(y) || byArtist(x, y) || byTitle(x, y),
      tracks: (x, y) => x.tracks - y.tracks || byArtist(x, y) || byTitle(x, y),
      // Not known (null) below every known one, whichever way round.
      length: (x, y) => (x.duration_secs ?? -1) - (y.duration_secs ?? -1) || byArtist(x, y) || byTitle(x, y),
    };
  };
  const found = $derived(
    sources.map((s) => {
      const albums = s.albums.filter((a) => (!a.library || libraryShown(s.id, a.library)) && matches(s, a)).sort(compareIn(s.id)[order]);
      return { s, albums: down ? albums.reverse() : albums };
    }),
  );
  // Searching, the server's finds not in yet and the browser's own none: the last list stays till they are (the owner,
  // 2026-10-25: the list and its header went and came back with the albums found by a track).
  let last = $state.raw<typeof found>([]);
  const pending = $derived(!!pageView.query.trim() && deepFor !== pageView.query.trim());
  const none = (l: typeof found) => l.every((x) => !x.albums.length);
  $effect(() => {
    if (!pending || !none(found)) last = found;
  });
  const shown = $derived(pending && none(found) && !none(last) ? last : found);
  // How you used them, read when such an order is chosen (and each time again: plays change).
  $effect(() => {
    if (!USE_ORDERS.includes(order)) return;
    for (const s of untrack(() => sources)) loadAlbumUses(s.id);
  });

  // Opened from a search, the words go, but where it was found by its tracks (lib/globalSearch's leaveSearch).
  const view = (source: string, a: ListedAlbum) => {
    if (pageView.query.trim()) leaveSearch(!!deep.get(albumSubject(source, a.id)));
    viewPlaylist(albumSubject(source, a.id));
  };
  /** A flow's front album's facts, its Details (the owner, 2026-10-26: as the player's info): what is known of it. */
  function detailsOf(s: ShelfSource, a: ListedAlbum) {
    const u = useOf(s.id, a.id);
    const lines = [
      { id: 'artist', label: 'Artist', full: a.artist ?? '' },
      { id: 'composer', label: a.composers && a.composers.length > 1 ? 'Composers' : 'Composer', full: (a.composers ?? []).map((c) => c.name).join(', ') },
      { id: 'performers', label: 'Performers', full: (a.performers ?? []).join(', ') },
      { id: 'year', label: 'Year', full: a.year ? String(a.year) : '' },
      { id: 'tracks', label: 'Tracks', full: a.tracks ? String(a.tracks) : '' },
      { id: 'length', label: 'Length', full: a.duration_secs ? clockText(a.duration_secs) : '' },
      { id: 'quality', label: 'Quality', full: quality(a) ?? '' },
      { id: 'added', label: 'Added', full: a.added ? addedOn(a.added) : '' },
      { id: 'plays', label: 'Plays', full: u.plays ? String(u.plays) : '' },
      { id: 'played', label: 'Last played', full: u.played ? addedOn(u.played) : '' },
      { id: 'rating', label: 'Rating', full: u.rating ? '★'.repeat(u.rating) : '' },
    ].filter((l) => l.full);
    return { genres: a.tags ?? [], lines };
  }
  /** Flow: each server's front album, by its place in its list. */
  let flowAt = $state<Record<string, number>>({});
  // A tile shows its title and artist; the value the list is ordered by, when it is neither, is a pill over the cover's
  // upper left corner (decided 2026-10-16).
  const axisOf = (s: ShelfSource, a: ListedAlbum) => {
    if (order === 'year') return a.year ? String(a.year) : null;
    if (order === 'added') return a.added ? addedOn(a.added) : null;
    if (order === 'length') return a.duration_secs ? clockText(a.duration_secs) : null;
    if (order === 'quality') return a.format ? [a.format, isLossless(a.format) ? null : a.bitrate_kbps].filter(Boolean).join(' ') : null;
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
  const ORDER_OF: Partial<Record<string, CollectionOrder>> = { year: 'year', tracks: 'tracks', length: 'length', added: 'added', plays: 'plays', lastPlayed: 'played', rating: 'rating', format: 'quality' };
  /** A tap on a name: ordered by it; again, the other way round. All but Year and Title start most and newest first. */
  function orderBy(o: CollectionOrder) {
    if (order === o) down = !down;
    else {
      order = o;
      down = o !== 'year' && o !== 'title';
    }
  }
  const headSorts = $derived({ title: { on: order === 'title', onclick: () => orderBy('title') } });
  /** A long press on the column names: the View sheet, where the columns are chosen and ordered (the owner,
   *  2026-10-26). */
  let columnsOpen = $state(false);
  const headPress = fromAction(longpress, () => () => (columnsOpen = true));
  // How you used them, read while a column shows it.
  $effect(() => {
    if (!tableCols.some((c) => c.id === 'plays' || c.id === 'lastPlayed' || c.id === 'rating')) return;
    for (const s of untrack(() => sources)) loadAlbumUses(s.id);
  });
  function cellText(s: ShelfSource, a: ListedAlbum, id: string): string {
    const u = id === 'plays' || id === 'lastPlayed' || id === 'rating' ? useOf(s.id, a.id) : null;
    if (id === 'year') return a.year ? String(a.year) : '';
    if (id === 'tracks') return String(a.tracks);
    if (id === 'length') return a.duration_secs ? clockText(a.duration_secs) : '';
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
        onclick: o ? () => orderBy(o) : undefined,
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
  // A long-press opens the album's menu (decision 0058: the menu in the lists; Select is the header's); in select mode it
  // asks what to tick.
  const longPress = (s: ShelfSource, a: ListedAlbum) => fromAction(longpress, () => (rect: DOMRect) => (selecting ? (selectSheet = true) : openMenu(s, a, rect)));

  const listOf = () => ticked.map((x) => ({ source: x.s.id, a: x.a }));
  async function playTicked(mode: 'play' | 'next' | 'last') {
    const list = listOf();
    if (!list.length) return;
    navigator.vibrate?.(8);
    selecting = null;
    try {
      // One album: the server's choice of it (decision 0059); several: their tracks, album after album, through the
      // same path (the device's own albums as before).
      if (list.length === 1 && list[0].source !== DEVICE) await playCollection({ kind: 'album', source: list[0].source, id: list[0].a.id }, mode, list[0].a.title);
      else if (list.every((x) => x.source !== DEVICE)) {
        const tracks = await tracksOfAlbums(list);
        await playCollection({ kind: 'tracks', source: list[0].source, ids: tracks.map((t) => t.id) }, mode, `${list.length} albums`);
      } else await playAlbums(list, mode);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
  /** The heart on the ticked albums: all favourites, or none when all already are. Shown at once, back on a refusal.
   *  Many ask first (lib/many), the selection kept on Cancel. */
  function favouriteTicked() {
    const list = [...ticked];
    const on = !tickedFav;
    const n = plural(list.length, 'album');
    askIfMany(list.length, { title: on ? `Favourite ${n}?` : `Unfavourite ${n}?`, label: on ? `Favourite ${n}` : `Unfavourite ${n}`, icon: 'heart' }, async () => {
      navigator.vibrate?.(8);
      selecting = null;
      for (const x of list) setFavorite(x.s.id, x.a.id, on);
      const failed = await fewAtATime(list, (x) => setAlbumFavorite(x.s.id, x.a.id, on));
      for (const x of failed) setFavorite(x.s.id, x.a.id, !on);
      if (failed.length) toast.fail(null, "The favourites didn't all change");
    });
  }

  /** The one ticked album's menu (a tile's menu is this way). */
  // The select bar's ⋮ (decision 0058): one album, its own menu; several, the menu of their tracks (lib/SelectionMenu)
  // with what one album's menu has besides (the owner, 2026-10-27: what one has, several have; lib/SeveralAlbums), the
  // ticked ones kept with it.
  let several = $state<{ title: string; tracks: LocalTrack[]; list: { s: ShelfSource; a: ListedAlbum }[] } | null>(null);
  /** Delete's or Move's question for several, once their menu has closed. */
  let asking = $state<Ask | null>(null);
  async function menuOfTicked(e: MouseEvent) {
    const x = ticked[0];
    if (!x) return;
    const anchor = (e.currentTarget as HTMLElement).getBoundingClientRect();
    if (ticked.length === 1) {
      selecting = null;
      return openMenu(x.s, x.a, anchor);
    }
    const list = ticked.map((t) => ({ source: t.s.id, a: t.a }));
    const kept = [...ticked];
    try {
      several = { title: `${list.length} albums`, tracks: await tracksOfAlbums(list), list: kept };
    } catch (err) {
      toast.fail(err, "Their tracks couldn't be read");
    }
  }
</script>

{#snippet why(s: ShelfSource, a: ListedAlbum)}
  <!-- Found by a track (the server's deep search, decision 0072): which, and how many of its tracks match. -->
  {@const d = deepOf(s, a)}
  {#if d}<span class="why"><Icon name="music" size={12} /> {d.text}{#if d.count && d.count > 1} · {d.count} tracks{/if}</span>{/if}
{/snippet}

<div class="lists" class:table bind:clientWidth={width}>
  {#each shown as { s, albums } (s.id)}
    {#if sources.length > 1}<h2>{s.label}</h2>{/if}
    {#if !s.supported}
      <p class="empty">{s.label} can't list albums.</p>
    {:else if s.error && s.albums.length === 0}
      <p class="error" role="alert">{s.error}</p>
    {:else if albums.length === 0 && pageView.query.trim() && deepFor !== pageView.query.trim()}
      <!-- Searching: the server's finds not in yet, so nothing said. -->
    {:else if albums.length === 0}
      <!-- None shown: none at all, or the search or the libraries switched off leave none (it said "0 albums" over
           empty column headings, the owner 2026-10-23). -->
      <p class="empty">{s.albums.length ? 'None in the libraries switched on match.' : empty}</p>
    {:else}
      <ListHead
        fields={ALBUM_ORDERS}
        bind:value={order}
        bind:down
        count={`${howMany(albums.length, 'album')}${countMore ? ` · ${countMore}` : ''}`}
        selecting={!!selecting}
        onselect={() => (selecting ? (selecting = null) : startSelect())}
        countOnly={layout === 'flow'}
      />
      {#if tableCols.length}
        <!-- The table's column names, a row of the same shape so they line up; a tap on one orders by it. -->
        <Row size="large" as="div" head title="Title" cells={headCells} sorts={headSorts} attach={headPress}>
          {#snippet front()}<span class="cover-space"></span>{/snippet}
          {#snippet action()}<span></span>{/snippet}
        </Row>
      {/if}
      {#if layout === 'flow'}
        <!-- Flow (the owner, 2026-10-23: digging through the albums): the list in its order as a coverflow, the front
             album named under it; a tap on it views it (in select mode ticks it, a ticked one highlighted), one
             beside it comes to the front. -->
        {@const front = albums[flowAt[s.id] ?? 0]}
        <FlowStage
          seed="albums:{s.id}"
          covers={albums.map((a) => albumCoverUrl(s.id, a.id))}
          marked={selecting ? albums.map((a) => selecting!.has(keyOf(s, a))) : undefined}
          front={front ? { title: front.title, by: [front.artist ?? 'Unknown artist', front.year].filter(Boolean).join(' · ') } : null}
          onopen={(i) => (selecting ? tick(s, albums[i]) : view(s.id, albums[i]))}
          onfront={(i) => (flowAt[s.id] = i)}
          onmenu={(i, rect) => (selecting ? (selectSheet = true) : openMenu(s, albums[i], rect))}
          onname={() => front && view(s.id, front)}
          onplay={() => front && playOf(s, front)}
          canPlay={albumPlay.starting === null}
          playing={!!front && albumPlay.starting === front.id}
          details={front ? detailsOf(s, front) : null}
        />
      {:else}
      {#each blocksOf(albums) as block, bi (`${s.id}:${perBlock}:${layout}:${bi}`)}
        {@const key = `${s.id}:${perBlock}:${layout}:${bi}`}
        {@const made = blocks.made(key, bi === 0)}
        <div class="block" {@attach blocks.watch(key, Math.ceil(block.length / cols))} style:height={made ? null : `${blocks.heights[key] ?? estimate(block.length)}px`}>
          {#if made}
      <ul class={layout} style:--min="{minPx}px">
        {#each block as a (a.id)}
          {@const here = playing?.album_id === a.id}
          {@const viewed = panelBeside() && panel.viewed === albumSubject(s.id, a.id)}
          {@const on = !!selecting && selecting.has(keyOf(s, a))}
          <li class:playing={here} class:ticked={on}>
            {#if layout === 'grid'}
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
                  <span class="line">{byline(a)}</span>
                  {@render why(s, a)}
                  {@render extra?.(s, a)}
                {/snippet}
                {#snippet over()}{#if !selecting}{@render play(s, a)}{/if}<CoverWait subject={albumSubject(s.id, a.id)} />{/snippet}
                {#snippet action()}{#if selecting}{@render tickOf(s, a)}{/if}{/snippet}
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
                {#snippet sub()}{byline(a)}{#if here}<span class="now"> · playing</span>{/if}{@render why(s, a)}{/snippet}
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
                  <span class="artist">{byline(a)}{#if here}<span class="now"> · playing</span>{/if}</span>
                  {@render why(s, a)}
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
    {/if}
  {/each}
  {#if selecting}
    <SelectBar label="Selected albums" count="{ticked.length}/{all.length}" onclose={() => (selecting = null)} place="page" every={{ all: !!all.length && ticked.length === all.length, onall: () => selectWhich('all'), onnone: () => selectWhich('none') }}>
      {#if actions}
        {@render actions(ticked, () => (selecting = null))}
        <button aria-label="Menu of the ticked albums" title="More" disabled={!ticked.length} onclick={menuOfTicked}><Icon name="moreVert" size={24} /></button>
      {:else}
      <button aria-label="Play" title="Play" disabled={!ticked.length} onclick={() => playTicked('play')}><Icon name="play" size={24} /></button>
      <button aria-label="Play next" title="Play next" disabled={!ticked.length} onclick={() => playTicked('next')}><Icon name="playNext" size={24} /></button>
      <button aria-label="Add to queue" title="Add to queue" disabled={!ticked.length} onclick={() => playTicked('last')}><Icon name="listAdd" size={24} /></button>
      <button class:fav={tickedFav} aria-pressed={tickedFav} aria-label="Favourite" title="Favourite" disabled={!ticked.length} onclick={favouriteTicked}
        ><Icon name="heart" size={24} /></button
      >
      <button aria-label="Menu of the ticked albums" title="More" disabled={!ticked.length} onclick={menuOfTicked}><Icon name="moreVert" size={24} /></button>
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

<!-- An album's cover (no heart on it: the owner, 2026-10-27). -->
{#snippet cover(s: ShelfSource, a: ListedAlbum)}
  <span class="art"><Cover src={albumCoverUrl(s.id, a.id)} /></span>
{/snippet}

<!-- A row's cover (the owner, 2026-10-27): a tap on it opens the album, as on the rest of the row; Play in its upper
     right corner alone plays (lib/CoverPlay), and it dims with the ring while it opens, when that takes long. -->
{#snippet playCover(s: ShelfSource, a: ListedAlbum)}
  {#if selecting}
    <button class="cover plays" aria-label="Select {a.title}" title="Select" onclick={() => tick(s, a)}>{@render cover(s, a)}</button>
  {:else}
    <span class="cover"
      ><button class="open" aria-label="View {a.title}" onclick={() => view(s.id, a)}>{@render cover(s, a)}</button>{@render play(s, a, true)}<CoverWait
        subject={albumSubject(s.id, a.id)}
        size={28}
      /></span
    >
  {/if}
{/snippet}

<!-- Play, in the cover's upper right corner (lib/CoverPlay): the only thing on a cover that plays. -->
{#snippet play(s: ShelfSource, a: ListedAlbum, small = false)}
  <CoverPlay label={a.title} album={a.id} {small} busy={albumPlay.starting === a.id} disabled={albumPlay.starting !== null} onplay={() => playOf(s, a)} />
{/snippet}

{#snippet severalExtra(close: () => void)}
  {#if several}<SeveralAlbums list={several.list} tracks={several.tracks} {close} onask={(q) => (asking = q)} />{/if}
{/snippet}

{#if several}
  <SelectionMenu
    title={several.title}
    tracks={several.tracks}
    chips={[plural(several.list.length, 'album')]}
    extra={severalExtra}
    onclose={() => ((several = null), (selecting = null))}
  />
{/if}
{#if asking}
  {@const q = asking}
  <ConfirmSheet title={q.title} text={q.text} label={q.label} icon={q.icon} onconfirm={q.go} onclose={() => (asking = null)} />
{/if}

{#if columnsOpen}<ViewSheet page="library" kind="albums" onclose={() => (columnsOpen = false)} />{/if}
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
    color: var(--danger-ink);
  }

  /* In columns, as the playlist chips are (lib/zoom.ts). A size wider than the room there is takes the room: the
     largest rows (680 px) scrolled a tablet in portrait sideways (the owner, 2026-10-29). */
  ul {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(var(--min), 100%), 1fr));
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

  .cover {
    position: relative;
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
    color: var(--accent-ink);
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

  /* A cover that is a button (selecting: it ticks). */
  .cover.plays {
    position: relative;
    padding: 0;
    border: 0;
    cursor: pointer;
  }

  .cover.plays:disabled {
    cursor: default;
  }

  .cover > .art,
  .cover > .open {
    display: block;
    width: 100%;
    height: 100%;
  }

  .open {
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }

  .art {
    position: relative;
    display: block;
    width: 100%;
    height: 100%;
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

  /* Why an album is there when only one of its tracks matched (decision 0072). */
  .why {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    overflow: hidden;
    color: var(--accent-ink);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    color: var(--accent-ink);
    font-weight: 600;
  }

  .weak {
    color: var(--danger-ink);
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
