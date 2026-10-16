<script lang="ts">
  // Library (decision 0034): every album of the followed server's used libraries, Music and Incoming, as one list
  // (lib/AlbumShelf), read at once (GET /api/albums). Which libraries show is the filter's library switches. An album's
  // facts show with it: its library as a dot in that library's colour (none for Music, and only while more than one
  // shows), an Incoming album's conversion, or how it relates to Music (an upgrade, a duplicate). Its menu
  // (lib/LibrarySheet) moves it into another library, replaces the album in Music it upgrades, or deletes it.
  import { ApiError, type AlbumsSource, type ListedAlbum, type ShelfSource } from './lib/api';
  import AlbumShelf from './lib/AlbumShelf.svelte';
  import Meter from './lib/Meter.svelte';
  import AlbumMenu, { offerIn, type AlbumOffer } from './lib/AlbumMenu.svelte';
  import { library, readLibrary } from './lib/albums.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { toast } from './lib/toast.svelte';
  import { jobs } from './lib/jobs.svelte';
  import { on, stream } from './lib/events.svelte';
  import { isFavorite, loadAlbumFavorites } from './lib/favourites.svelte';
  import { pageView } from './lib/pageView.svelte';
  import { untrack } from 'svelte';

  let error = $state<string | null>(null);
  const sources = $derived<AlbumsSource[] | null>(library.view?.sources ?? null);
  const trashDays = $derived(library.view?.trash_days ?? 14);
  /** The primary library, by its own name. */
  const music = $derived(library.view?.music ?? 'Music');

  // An album's menu (lib/AlbumMenu), the same as everywhere.
  let menu = $state<{ source: string; a: ListedAlbum; offer: AlbumOffer } | null>(null);

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
    if (s) menu = { source: s.id, a, offer: offerIn(s, a) };
  }
  const verdictOf = (shelf: ShelfSource, a: ListedAlbum) => sources?.find((x) => x.id === shelf.id)?.verdicts[a.id];

  // The filter's own switches, Favourites only and Upgrades waiting: the list's, so off again when the page goes.
  $effect(() => () => {
    pageView.favouritesOnly = false;
    pageView.upgradesOnly = false;
  });
  // Favourites only asks each server again (an app may have changed a star meanwhile); one that cannot tell says so.
  $effect(() => {
    if (!pageView.favouritesOnly) return;
    for (const s of untrack(() => sources ?? []))
      loadAlbumFavorites(s.id).then((ok) => ok || toast.show({ text: `${s.label} can't list favourite albums`, tone: 'error' }));
  });
  const shown = $derived.by<ShelfSource[] | null>(() => {
    const { favouritesOnly: fav, upgradesOnly: up } = pageView;
    if (!sources || !(fav || up)) return sources;
    const keep = (s: ShelfSource, a: ListedAlbum) => (!fav || isFavorite(s.id, a.id)) && (!up || verdictOf(s, a)?.kind === 'upgrade');
    return sources.map((s) => ({ ...s, albums: s.albums.filter((a) => keep(s, a)) }));
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
        {:else if verdictOf(s, a)}
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

{#if menu}
  {@const m = menu}
  <AlbumMenu
    source={m.source}
    album={m.a}
    offer={m.offer}
    {trashDays}
    {music}
    onclose={() => (menu = null)}
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
