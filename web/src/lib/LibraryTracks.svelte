<script lang="ts">
  // The Library's Tracks (the owner, 2026-10-25: "in the same style as the album list with a beautiful table"): Cratarr's
  // own tracks, from the server a page at a time (a5's /api/library/tracks; the next as the list's end comes near), in
  // the header's order, the page's search and the libraries switched off asked of the server; rows or the table (the
  // Library's View), the table's columns as wide as the list allows (decision 0042). A tap plays from that track, the
  // tracks loaded so far the queue (500 at most); rows or the table only (the owner, 2026-10-26: no Covers or Flow for
  // tracks); a cover views its album; a long-press the track's menu (lib/TrackMenu);
  // Select as the albums' (the ticked ones' heart, as a playlist's select bar has it, and the selection's menu).
  import { albumCoverUrl, libraryTracks, setTrackFavorite, type LibraryTrack } from './api';
  import { askIfMany, fewAtATime } from './many.svelte';
  import { setFavorite } from './favourites.svelte';
  import { fit, LIBRARY_TRACK_COLUMNS } from './columns.svelte';
  import ViewSheet from './ViewSheet.svelte';
  import Cover from './Cover.svelte';
  import { libraryLayout } from './layouts';
  import { addedOn } from './albums.svelte';
  import { hiddenLibraries } from './listFilter.svelte';
  import ListHead from './ListHead.svelte';
  import PageLoading from './PageLoading.svelte';
  import { albumSubject, viewPlaylist } from './panel.svelte';
  import { howMany, pageView, TRACK_ORDERS } from './pageView.svelte';
  import { playCollection } from './play.svelte';
  import { prefs } from './prefs.svelte';
  import Row, { type Cell } from './Row.svelte';
  import { toast } from './toast.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import SelectBar from './SelectBar.svelte';
  import SelectionMenu from './SelectionMenu.svelte';
  import SelectSheet from './SelectSheet.svelte';
  import TrackMenu, { type MenuTrack } from './TrackMenu.svelte';
  import type { LocalTrack } from './localPlayer.svelte';
  import { plural } from './insights.svelte';
  import { fromAction } from 'svelte/attachments';

  const PAGE = 200;
  let items = $state<LibraryTrack[]>([]);
  let total = $state<number | null>(null);
  let error = $state<string | null>(null);
  let loading = false;
  /** What the list was asked for: another order, words or libraries start it again. */
  const asked = $derived(JSON.stringify([pageView.query.trim(), prefs.libraryTrackOrder, prefs.libraryTrackDown, hiddenLibraries() ?? []]));
  let gen = 0;
  async function more(reset = false) {
    if (loading && !reset) return;
    const mine = reset ? ++gen : gen;
    loading = true;
    try {
      const r = await libraryTracks({
        q: pageView.query.trim() || undefined,
        order: prefs.libraryTrackOrder,
        down: prefs.libraryTrackDown,
        offset: reset ? 0 : items.length,
        limit: PAGE,
        without: hiddenLibraries(),
      });
      if (mine !== gen) return;
      items = reset ? r.items : [...items, ...r.items];
      total = r.total;
      error = null;
    } catch (e) {
      if (mine === gen) error = e instanceof Error ? e.message : 'The tracks could not be listed';
    } finally {
      if (mine === gen) loading = false;
    }
  }
  // Asked again a moment after the words (or the order) stop changing.
  $effect(() => {
    void asked;
    const t = setTimeout(() => more(true), 200);
    return () => clearTimeout(t);
  });
  // The next page as the list's end comes near.
  let end = $state<HTMLElement>();
  $effect(() => {
    const el = end;
    if (!el) return;
    const io = new IntersectionObserver((e) => e.some((x) => x.isIntersecting) && total !== null && items.length < total && more(), { rootMargin: '800px' });
    io.observe(el);
    return () => io.disconnect();
  });

  /** As chosen in View, as it can be here (lib/layouts). */
  const layout = $derived(libraryLayout('tracks'));
  const table = $derived(layout === 'table');
  let width = $state(0);
  // The track lists' columns that the server lists (decision 0042).
  const cols = $derived(table ? fit('tracks', width, (id) => LIBRARY_TRACK_COLUMNS.includes(id)) : []);
  const clock = (s: number) => `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, '0')}`;
  function cellText(t: LibraryTrack, id: string) {
    if (id === 'album') return t.album ?? '';
    if (id === 'plays') return t.plays ? t.plays.toLocaleString() : '';
    if (id === 'added') return t.added ? addedOn(t.added) : '';
    if (id === 'year') return t.year ? String(t.year) : '';
    if (id === 'tempo') return t.bpm ? String(t.bpm) : '';
    if (id === 'lastPlayed') return t.last_played ? addedOn(t.last_played) : '';
    if (id === 'albumArtist') return t.album_artist ?? '';
    return '';
  }
  const cellsOf = (t: LibraryTrack): Cell[] => cols.map((c) => ({ ...c, text: cellText(t, c.id) }));
  const ORDER_OF: Record<string, typeof prefs.libraryTrackOrder> = { album: 'album', plays: 'plays', added: 'added', year: 'year', tempo: 'tempo', lastPlayed: 'lastPlayed', albumArtist: 'albumArtist' };
  /** A tap on a name: ordered by it; again, the other way round. Plays, Added and Last played start most and newest
   *  first. */
  function orderBy(o: typeof prefs.libraryTrackOrder) {
    if (prefs.libraryTrackOrder === o) prefs.libraryTrackDown = !prefs.libraryTrackDown;
    else ((prefs.libraryTrackOrder = o), (prefs.libraryTrackDown = o === 'plays' || o === 'added' || o === 'lastPlayed'));
  }
  const headSorts = $derived({
    title: { on: prefs.libraryTrackOrder === 'title', onclick: () => orderBy('title') },
    count: { on: prefs.libraryTrackOrder === 'duration', onclick: () => orderBy('duration') },
  });
  /** A long press on the column names: the View sheet, where the columns are chosen and ordered (the owner,
   *  2026-10-26). */
  let columnsOpen = $state(false);
  const headPress = fromAction(longpress, () => () => (columnsOpen = true));
  const headCells = $derived<Cell[]>(
    cols.map((c) => {
      const o = ORDER_OF[c.id];
      return {
        ...c,
        text: c.label,
        on: prefs.libraryTrackOrder === o,
        onclick: () => orderBy(o),
      };
    }),
  );

  // Select (as the albums'): the ticked ones by id; null while not selecting.
  let selecting = $state<Set<string> | null>(null);
  let selectSheet = $state(false);
  const ticked = $derived(selecting ? items.filter((t) => selecting!.has(t.id)) : []);
  function tick(t: LibraryTrack) {
    const s = new Set(selecting);
    if (s.has(t.id)) s.delete(t.id);
    else s.add(t.id);
    selecting = s;
  }
  function selectWhich(which: 'all' | 'none' | 'others') {
    const ids = items.map((t) => t.id);
    selecting = new Set(which === 'all' ? ids : which === 'none' ? [] : ids.filter((id) => !selecting?.has(id)));
  }
  /** The heart on the ticked tracks (the owner, 2026-10-27: what one has, several have): all favourites, or none when
   *  all already are, as a playlist's select bar (lib/PlaylistView). Shown at once, back on a refusal; many ask first. */
  const tickedFav = $derived(ticked.length > 0 && ticked.every((t) => t.favorite));
  function favouriteTicked() {
    const list = ticked;
    const on = !tickedFav;
    const n = plural(list.length, 'track');
    askIfMany(list.length, { title: on ? `Favourite ${n}?` : `Unfavourite ${n}?`, label: on ? `Favourite ${n}` : `Unfavourite ${n}`, icon: 'heart' }, async () => {
      navigator.vibrate?.(8);
      const was = new Map(list.map((t) => [t.id, t.favorite]));
      const show = (t: LibraryTrack, fav: boolean) => {
        t.favorite = fav;
        setFavorite('cratarr', t.id, fav);
      };
      for (const t of list) show(t, on);
      selecting = null;
      const failed = await fewAtATime(list, (t) => setTrackFavorite('cratarr', t.id, on));
      for (const t of failed) show(t, was.get(t.id) ?? !on);
      if (failed.length) toast.fail(null, "The favourites didn't all change");
    });
  }
  const asTrack = (t: LibraryTrack): LocalTrack => ({ source: 'cratarr', id: t.id, name: t.title, artist: t.artist, album: t.album, album_id: t.album_id, duration_secs: t.duration });
  let several = $state<LocalTrack[] | null>(null);
  let menuOf = $state<MenuTrack | null>(null);
  const press = (t: LibraryTrack) =>
    fromAction(longpress, () => () =>
      selecting ? (selectSheet = true) : (menuOf = { source: 'cratarr', id: t.id, name: t.title, artist: t.artist, album: t.album, album_id: t.album_id }),
    );

  async function play(t: LibraryTrack) {
    navigator.vibrate?.(8);
    try {
      await playCollection({ kind: 'tracks', source: 'cratarr', ids: items.slice(0, 500).map((x) => x.id) }, 'play', 'Library', t.id);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
</script>

<div class="lists" class:table bind:clientWidth={width}>
  {#if error && !items.length}
    <p class="error" role="alert">{error}</p>
  {:else if total === null}
    <PageLoading />
  {:else if !items.length}
    <p class="empty">{pageView.query.trim() ? 'No tracks match.' : 'No tracks.'}</p>
  {:else}
    <ListHead
      fields={TRACK_ORDERS}
      bind:value={prefs.libraryTrackOrder}
      bind:down={prefs.libraryTrackDown}
      count={howMany(total, 'track')}
      selecting={!!selecting}
      onselect={() => (selecting = selecting ? null : new Set())}
    />
    {#if cols.length}
      <Row size="large" as="div" head title="Title" cells={headCells} count="Length" sorts={headSorts} attach={headPress}>
        {#snippet front()}<span class="cover-space"></span>{/snippet}
        <!-- The rows' slot for Select's tick, kept here too: the headings over their columns. -->
        {#snippet action()}<span></span>{/snippet}
      </Row>
    {/if}
    <ul class:table>
      {#each items as t (t.id)}
        <li>
          <Row
            size="large"
            title={t.title}
            sub={[t.artist, cols.some((c) => c.id === 'album') ? null : t.album].filter(Boolean).join(' · ')}
            cells={cols.length ? cellsOf(t) : undefined}
            count={clock(t.duration)}
            label={selecting ? `${selecting.has(t.id) ? 'Untick' : 'Tick'} ${t.title}` : `Play ${t.title}`}
            active={!!selecting?.has(t.id)}
            onclick={() => (selecting ? tick(t) : play(t))}
            attach={press(t)}
          >
            {#snippet action()}
              {#if selecting}
                {@const on = selecting.has(t.id)}
                <button class="icon tick" class:on role="checkbox" aria-checked={on} aria-label="Select {t.title}" onclick={() => tick(t)}
                  ><Icon name={on ? 'circleCheck' : 'circle'} size={24} /></button
                >
              {/if}
            {/snippet}
            {#snippet front()}
              <button class="cover" aria-label="View {t.album ?? 'its album'}" title="View the album" onclick={(e) => (e.stopPropagation(), viewPlaylist(albumSubject('cratarr', t.album_id)))}
                ><Cover src={albumCoverUrl('cratarr', t.album_id)} /></button
              >
            {/snippet}
          </Row>
        </li>
      {/each}
    </ul>
    <div class="end" bind:this={end} aria-hidden="true"></div>
  {/if}
</div>

{#if selecting}
  <!-- All: the tracks loaded so far (the list comes a page at a time). -->
  <SelectBar
    label="Selected tracks"
    count="{ticked.length}/{total ?? 0}"
    onclose={() => (selecting = null)}
    place="page"
    every={{ all: !!items.length && ticked.length === items.length, onall: () => selectWhich('all'), onnone: () => selectWhich('none') }}
  >
    <button class:fav={tickedFav} aria-pressed={tickedFav} aria-label="Favourite" title="Favourite" disabled={!ticked.length} onclick={favouriteTicked}><Icon name="heart" size={24} /></button>
    <button aria-label="Menu of the ticked tracks" title="More" disabled={!ticked.length} onclick={() => (several = ticked.map(asTrack))}><Icon name="moreVert" size={24} /></button>
  </SelectBar>
{/if}
{#if several}
  <SelectionMenu title={plural(several.length, 'track')} tracks={several} onclose={() => ((several = null), (selecting = null))} />
{/if}
{#if selectSheet}
  <SelectSheet onall={() => selectWhich('all')} onnone={() => selectWhich('none')} onothers={() => selectWhich('others')} onclose={() => (selectSheet = false)} />
{/if}
{#if columnsOpen}<ViewSheet page="library" kind="tracks" onclose={() => (columnsOpen = false)} />{/if}
{#if menuOf}
  <TrackMenu track={menuOf} onclose={() => (menuOf = null)} />
{/if}

<style>
  ul {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    min-width: 0;
    content-visibility: auto;
    contain-intrinsic-size: auto 64px;
  }

  /* The table (decision 0042): one track a line, the lines touching. */
  ul.table {
    display: block;
  }

  .cover,
  .cover-space {
    display: block;
    width: 48px;
    height: 48px;
    margin-right: 6px;
    padding: 0;
    overflow: hidden;
    border: 0;
    border-radius: 6px;
    background: none;
    cursor: pointer;
  }

  .end {
    height: 1px;
  }

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

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }

  .error {
    color: var(--danger-ink);
  }
</style>
