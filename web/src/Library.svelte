<script lang="ts">
  // Library (decision 0034): every album of the followed server's used libraries, Music and Incoming, as one list
  // (lib/AlbumShelf), read at once (GET /api/albums). Which libraries show is the filter's library switches. An album's
  // facts show with it: its library as a dot in that library's colour (none for Music, and only while more than one
  // shows), an Incoming album's conversion, or how it relates to Music (an upgrade, a duplicate). Its menu
  // (lib/LibrarySheet) moves it into another library, replaces the album in Music it upgrades, or deletes it.
  // Cratarr's global exclusions (decision 0024, amended 2026-10-19) are counted here, for the filter sheet's
  // "Excluded (N)" (lib/FilterSheet; the owner moved them off the page: "It takes up a lot of real estate").
  import { ApiError, type AlbumsSource, type ListedAlbum, type ShelfSource } from './lib/api';
  import AlbumShelf from './lib/AlbumShelf.svelte';
  import Meter from './lib/Meter.svelte';
  import AlbumMenu, { offerIn, type AlbumOffer } from './lib/AlbumMenu.svelte';
  import { library, loadAlbumUses, readLibrary, useOf } from './lib/albums.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { toast } from './lib/toast.svelte';
  import { jobs } from './lib/jobs.svelte';
  import { on, stream } from './lib/events.svelte';
  import { isFavorite, loadAlbumFavorites } from './lib/favourites.svelte';
  import { isLossless, pageView } from './lib/pageView.svelte';
  import { passing, switchOn } from './lib/listFilter.svelte';
  import { libraryShown } from './lib/sources.svelte';
  import { untrack } from 'svelte';
  import { me } from './lib/me.svelte';
  import { loadExcluded } from './lib/exclusions.svelte';

  $effect(() => {
    loadExcluded();
  });

  let error = $state<string | null>(null);
  const sources = $derived<AlbumsSource[] | null>(library.view?.sources ?? null);
  const trashDays = $derived(library.view?.trash_days ?? 14);
  /** The primary library, by its own name. */
  const music = $derived(library.view?.music ?? 'Music');

  // An album's menu (lib/AlbumMenu), the same as everywhere.
  // Closing it hides it and keeps its album: what the menu does after it closes (Play, Share, a chosen cover) still
  // reads it.
  let menu = $state<{ source: string; a: ListedAlbum; offer: AlbumOffer } | null>(null);
  let menuShown = $state(false);

  // The album left its library (moved, replaced, deleted): gone at once, before the list is read again.
  function gone(source: string, id: string) {
    const v = library.view;
    if (v) v.sources = v.sources.map((s) => (s.id === source ? { ...s, albums: s.albums.filter((x) => x.id !== id) } : s));
  }

  // A server's list could not be read while albums are shown (they stay): one toast says so, not a line that comes and
  // goes; it goes when the list reads again. With nothing to show, the page says it in place.
  let readError: string | null = null;
  function tellReadError() {
    const now = (sources ?? []).find((s) => s.error && s.albums.length > 0)?.error ?? null;
    if (now && now !== readError) toast.show({ text: now, tone: 'error' });
    if (!now && readError && toast.current?.text === readError) toast.dismiss();
    readError = now;
  }

  async function load(fresh: boolean) {
    try {
      await readLibrary(fresh);
      error = null;
    } catch (e) {
      if (library.view === null) error = e instanceof ApiError ? e.message : 'Unexpected error';
    }
    tellReadError();
  }
  // One read for several changes told at once (an action changes Incoming's list and Music's).
  let soon: ReturnType<typeof setTimeout> | null = null;
  const reload = () => {
    soon ??= setTimeout(() => {
      soon = null;
      load(true);
    }, 50);
  };

  // The live channel says when a list changed (albums came, or an action or its Undo went through): read it again.
  $effect(() =>
    on<{ area: string }>('changed', ({ area }) => {
      if (area === 'incoming' || area === 'collection') reload();
    }),
  );
  // Back after it was down: what changed meanwhile was not told.
  let wasConnected = stream.connected;
  $effect(() => {
    if (stream.connected && !wasConnected) reload();
    wasConnected = stream.connected;
  });
  // Without it: a queued album finished, so read the lists again (it is in Music now, or back as it was).
  let seenFinished = jobs.finished;
  $effect(() => {
    if (jobs.finished !== seenFinished) {
      seenFinished = jobs.finished;
      if (!stream.connected) reload();
    }
  });

  // The kept copies first (the service worker's), then the latest; after that only while the live channel is down
  // (it says when a list changes), every 30 s.
  $effect(() => {
    void prefs.source;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    let first = true;
    async function follow() {
      if (first || !stream.connected) await load(true);
      first = false;
      if (!stopped) timer = setTimeout(follow, 30_000);
    }
    load(false).then(follow);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });

  // An album's menu: what its library allows depends on the library and what Cratarr may do there.
  function openMenu(shelf: ShelfSource, a: ListedAlbum) {
    const s = sources?.find((x) => x.id === shelf.id);
    if (!s) return;
    menu = { source: s.id, a, offer: offerIn(s, a) };
    menuShown = true;
  }
  const verdictOf = (shelf: ShelfSource, a: ListedAlbum) => sources?.find((x) => x.id === shelf.id)?.verdicts[a.id];

  // The filter's own switches (Favourites only, Not played yet, 4 stars and up, Lossless only, Upgrades waiting,
  // Duplicates waiting), remembered as the Library's (decision 0060).
  const fav = $derived(switchOn('library', 'favourites'));
  const unplayed = $derived(switchOn('library', 'unplayed'));
  const top = $derived(switchOn('library', 'topRated'));
  const lossless = $derived(switchOn('library', 'lossless'));
  const up = $derived(switchOn('library', 'upgrades'));
  const dup = $derived(switchOn('library', 'duplicates'));
  // Not played yet and 4 stars and up read how you used the albums, again each time one is switched on.
  $effect(() => {
    if (!unplayed && !top) return;
    for (const s of untrack(() => sources ?? [])) loadAlbumUses(s.id);
  });
  // Favourites only asks each server again (an app may have changed a star meanwhile); one that cannot tell says so.
  $effect(() => {
    if (!fav) return;
    for (const s of untrack(() => sources ?? []))
      loadAlbumFavorites(s.id).then((ok) => ok || toast.show({ text: `${s.label} can't list favourite albums`, tone: 'error' }));
  });
  // Its conditions (decision 0061): the server says which albums pass, any or all of their tracks.
  const pass = passing(() => 'library', () => ({ list: { kind: 'library' }, source: prefs.source || undefined }));
  const shown = $derived.by<ShelfSource[] | null>(() => {
    const keys = pass.keys;
    if (!sources || !(fav || up || dup || unplayed || top || lossless || keys)) return sources;
    const keep = (s: ShelfSource, a: ListedAlbum) => {
      const u = useOf(s.id, a.id);
      return (
        (!fav || isFavorite(s.id, a.id)) &&
        (!up || verdictOf(s, a)?.kind === 'upgrade') &&
        (!dup || verdictOf(s, a)?.kind === 'duplicate') &&
        (!unplayed || (u.plays === 0 && !u.played)) &&
        (!top || u.rating >= 4) &&
        (!lossless || isLossless(a.format)) &&
        (!keys || keys.has(a.id))
      );
    };
    return sources.map((s) => ({ ...s, albums: s.albums.filter((a) => keep(s, a)) }));
  });
  // How many albums the filter lets through, of how many (the Filter sheet's count): of the libraries switched on, as
  // the shelf shows them (it said 605 of 2,861 with Music switched off).
  $effect(() => {
    const count = (l: ShelfSource[] | null) => (l ?? []).reduce((n, s) => n + s.albums.filter((a) => !a.library || libraryShown(s.id, a.library)).length, 0);
    pageView.counted = sources ? { shown: count(shown), total: count(sources) } : null;
    return () => (pageView.counted = null);
  });
</script>

<main>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if shown === null}
    <PageLoading />
  {:else if shown.length === 0}
    <p class="empty">No server with a Music or Incoming library is connected.</p>
  {:else}
    <AlbumShelf sources={shown} empty="No albums." onmenu={openMenu} bind:order={prefs.libraryOrder} bind:down={prefs.libraryDown}>
      {#snippet extra(s: ShelfSource, a: ListedAlbum)}
        {#if jobs.of(s.id, a.id)}
          {@const job = jobs.of(s.id, a.id)!}
          <Meter
            label={job.state === 'waiting' ? 'Waiting to convert' : `Converting ${job.done} of ${job.total}`}
            value={(job.done + job.part) / Math.max(job.total, 1)}
          />
        {:else if me.admin && verdictOf(s, a)}
          {@const v = verdictOf(s, a)!}
          <span class="verdict" class:upgrade={v.kind === 'upgrade'} title={v.library}>
            <span class="badge">{v.kind === 'upgrade' ? 'Upgrade' : `In ${music}`}</span>
            {v.why}
          </span>
        {/if}
      {/snippet}
    </AlbumShelf>
  {/if}
</main>

{#if menuShown && menu}
  {@const m = menu}
  <AlbumMenu
    source={m.source}
    album={m.a}
    offer={m.offer}
    {trashDays}
    {music}
    onclose={() => (menuShown = false)}
    ongone={() => gone(m.source, m.a.id)}
    onundone={reload}
  />
{/if}

<style>
  /* As wide as the page, as Playlists: the albums take as many columns as fit. */
  main {
    padding: 6px clamp(12px, 2vw, 32px) 48px;
  }

  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .error {
    color: var(--danger);
  }

  .verdict {
    display: block;
    margin-top: 3px;
    color: var(--muted);
    font-size: 0.8rem;
  }

  .verdict .badge {
    margin-right: 6px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text);
    font-weight: 600;
  }

  .verdict.upgrade .badge {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
