<script lang="ts">
  // The Library's Tracks (the owner, 2026-10-25: "in the same style as the album list with a beautiful table"): Cratarr's
  // own tracks, from the server a page at a time (a5's /api/library/tracks; the next as the list's end comes near), in
  // the header's order, the page's search and the libraries switched off asked of the server; rows or the table (the
  // Library's View), the table's columns as wide as the list allows (decision 0042). A tap plays from that track, the
  // tracks loaded so far the queue (500 at most); a cover views its album.
  import { albumCoverUrl, libraryTracks, type LibraryTrack } from './api';
  import { fit } from './columns.svelte';
  import Cover from './Cover.svelte';
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

  const table = $derived(prefs.albumLayout === 'table');
  let width = $state(0);
  // The track lists' columns that the server lists (decision 0042).
  const KNOWN = ['album', 'plays', 'added', 'year'];
  const cols = $derived(table ? fit('tracks', width, (id) => KNOWN.includes(id)) : []);
  const clock = (s: number) => `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, '0')}`;
  function cellText(t: LibraryTrack, id: string) {
    if (id === 'album') return t.album ?? '';
    if (id === 'plays') return t.plays ? t.plays.toLocaleString() : '';
    if (id === 'added') return t.added ? addedOn(t.added) : '';
    if (id === 'year') return t.year ? String(t.year) : '';
    return '';
  }
  const cellsOf = (t: LibraryTrack): Cell[] => cols.map((c) => ({ ...c, text: cellText(t, c.id) }));
  const ORDER_OF: Record<string, typeof prefs.libraryTrackOrder> = { album: 'album', plays: 'plays', added: 'added', year: 'year' };
  const headCells = $derived<Cell[]>(
    cols.map((c) => {
      const o = ORDER_OF[c.id];
      return {
        ...c,
        text: c.label,
        on: prefs.libraryTrackOrder === o,
        onclick: () => {
          if (prefs.libraryTrackOrder === o) prefs.libraryTrackDown = !prefs.libraryTrackDown;
          else ((prefs.libraryTrackOrder = o), (prefs.libraryTrackDown = o === 'plays' || o === 'added'));
        },
      };
    }),
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
    <ListHead fields={TRACK_ORDERS} bind:value={prefs.libraryTrackOrder} bind:down={prefs.libraryTrackDown} count={howMany(total, 'track')} />
    {#if cols.length}
      <Row size="large" as="div" head title="Title" cells={headCells} count="Length">
        {#snippet front()}<span class="cover-space"></span>{/snippet}
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
            label={`Play ${t.title}`}
            onclick={() => play(t)}
          >
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

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }

  .error {
    color: var(--danger-ink);
  }
</style>
