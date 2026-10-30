<script lang="ts">
  // Library (decision 0034): every album of the followed server's used libraries, Music and Incoming, as one list
  // (lib/AlbumShelf), read at once (GET /api/albums). Which libraries show is the filter's library switches. An album's
  // facts show with it: its library as a dot in that library's colour (none for Music, and only while more than one
  // shows), an Incoming album's conversion, or how it relates to Music (an upgrade, a duplicate). Its menu
  // (lib/LibrarySheet) moves it into another library, replaces the album in Music it upgrades, or deletes it. Its
  // records only (the owner, 2026-10-29: Artists and Tracks went, crates being the way to tracks); which kind beside
  // back (lib/RecordsPicker).
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
  import { pageView } from './lib/pageView.svelte';
  import { passing } from './lib/listFilter.svelte';
  import { libraryShown } from './lib/sources.svelte';
  import { untrack } from 'svelte';
  import { me } from './lib/me.svelte';
  import ComposerList from './lib/ComposerList.svelte';
  import { classical, followAddress } from './lib/classical.svelte';
  import { classicalListed, kindNow } from './lib/recordKinds';

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
  /** What an Incoming album lacks of what the library requires, to see before Keep (the owner, 2026-10-24). */
  const lacksOf = (shelf: ShelfSource, a: ListedAlbum) => sources?.find((x) => x.id === shelf.id)?.lacks?.[a.id] ?? [];

  // The filter (decisions 0060 and 0061): its own switches (Favourites only, Unplayed only, 4 stars and up) and its
  // conditions (the kind beside back is its Type row), remembered as the Library's; one left on from before goes by
  // nothing. One engine: the server, which holds what each switch means (the owner, 2026-10-29), says which records pass.
  const pass = passing(() => 'library', () => ({ list: { kind: 'library' }, source: prefs.source || undefined }));
  // Classical (0.10): its composers first; a composer's records, the list narrowed to them (lib/classical).
  const inClassical = $derived(kindNow() === 'Classical' && classicalListed());
  $effect(() => {
    if (inClassical) followAddress();
  });
  const composer = $derived(inClassical ? classical.composer : null);
  const shown = $derived.by<ShelfSource[] | null>(() => {
    const keys = pass.keys;
    // What a filter goes by not in yet: loading, not the albums unfiltered or none (the owner, 2026-10-23).
    if (pass.pending && !keys) return null;
    const of = composer?.records;
    if (!sources || (!keys && !of)) return sources;
    return sources.map((s) => ({ ...s, albums: s.albums.filter((a) => (!keys || keys.has(a.id)) && (!of || of.has(a.id))) }));
  });
  // How many albums the filter lets through, of how many (the Filter sheet's count): of the libraries switched on, as
  // the shelf shows them (it said 605 of 2,861 with Music switched off).
  $effect(() => {
    const count = (l: ShelfSource[] | null) => (l ?? []).reduce((n, s) => n + s.albums.filter((a) => !a.library || libraryShown(s.id, a.library)).length, 0);
    pageView.counted = sources ? { shown: count(shown), total: count(sources), pending: pass.pending } : null;
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
  {:else if inClassical && !composer}
    <ComposerList />
  {:else}
    {#if composer}<h2 class="composer">{composer.name}</h2>{/if}
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
        {#if me.admin && lacksOf(s, a).length}<span class="lacks">Missing {lacksOf(s, a).join(', ')}</span>{/if}
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
    /* Its side gutter, which Flow's covers reach past to the page's edges (lib/AlbumShelf). */
    --gutter: clamp(12px, 2vw, 32px);
    padding: 6px var(--gutter) 48px;
  }

  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }

  /* A composer's records: their name over them. */
  .composer {
    margin: 4px 0 8px;
    font-size: 1.1rem;
  }

  .error {
    color: var(--danger-ink);
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

  /* What an Incoming album lacks: a warning, as Health's. */
  .lacks {
    display: block;
    margin-top: 3px;
    color: var(--warn-ink);
    font-size: 0.8rem;
  }

  .verdict.upgrade .badge {
    border-color: var(--accent);
    color: var(--accent-ink);
  }
</style>
