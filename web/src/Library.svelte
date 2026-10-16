<script lang="ts">
  // Library (decision 0034): every album of the followed server's used libraries, Music and Incoming, as one list
  // (lib/AlbumShelf). Which libraries show is the filter's library switches. An album's facts show with it: an Incoming
  // album's conversion, or how it relates to Music (an upgrade, a duplicate). Until the server lists every library at
  // once (GET /api/albums), the page reads Incoming's and Collection's lists and puts each server's together; the
  // album's own `library` says which it is in. An Incoming album's menu is Incoming's sheet (Keep, Replace, Delete);
  // a Music album's is the list's own until moves come.
  import { ApiError, getCollection, getIncoming, type IncomingSource, type ListedAlbum, type ShelfSource } from './lib/api';
  import AlbumShelf from './lib/AlbumShelf.svelte';
  import Meter from './lib/Meter.svelte';
  import IncomingSheet, { offerOf, type IncomingOffer } from './lib/IncomingSheet.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { toast } from './lib/toast.svelte';
  import { jobs } from './lib/jobs.svelte';
  import { on, stream } from './lib/events.svelte';

  let incoming = $state<IncomingSource[] | null>(null);
  let collection = $state<ShelfSource[] | null>(null);
  let trashDays = $state(14);
  /** The library Keep and Replace put albums in, by its own name. */
  let music = $state<string | null>(null);
  let error = $state<string | null>(null);

  // Each server's albums of both lists as one source.
  const sources = $derived.by<ShelfSource[] | null>(() => {
    if (incoming === null && collection === null) return null;
    const ids = [...new Set([...(collection ?? []).map((s) => s.id), ...(incoming ?? []).map((s) => s.id)])];
    return ids.map((id) => {
      const c = collection?.find((s) => s.id === id);
      const i = incoming?.find((s) => s.id === id);
      const any = (c ?? i)!;
      return {
        id,
        label: any.label,
        supported: !!(c?.supported || i?.supported),
        albums: [...(c?.albums ?? []), ...(i?.albums ?? [])],
        error: c?.error ?? i?.error ?? null,
        as_of: Math.max(c?.as_of ?? 0, i?.as_of ?? 0),
        libraries: [...(c?.libraries ?? []), ...(i?.libraries ?? [])],
      };
    });
  });

  // The Incoming list an album is in, if it is in one.
  const incomingOf = (sourceId: string, a: ListedAlbum) => {
    const s = incoming?.find((x) => x.id === sourceId);
    return s && s.albums.some((x) => x.id === a.id) ? s : null;
  };

  // The long-pressed album's sheet (Play, View, Keep or Replace, Delete).
  let menu = $state<{ source: string; a: ListedAlbum; offer: IncomingOffer } | null>(null);

  // The album left Incoming (kept, replaced, deleted): gone at once, before the lists are read again.
  function gone(source: string, id: string) {
    incoming = (incoming ?? []).map((s) => (s.id === source ? { ...s, albums: s.albums.filter((x) => x.id !== id) } : s));
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

  async function readIncoming(fresh: boolean) {
    ({ sources: incoming, trash_days: trashDays, music } = await getIncoming(prefs.source || undefined, fresh));
  }
  async function readCollection(fresh: boolean) {
    ({ sources: collection } = await getCollection(prefs.source || undefined, fresh));
  }
  async function load(fresh: boolean) {
    const [i, c] = await Promise.allSettled([readIncoming(fresh), readCollection(fresh)]);
    const failed = [i, c].find((r) => r.status === 'rejected') as PromiseRejectedResult | undefined;
    if (failed && sources === null) error = failed.reason instanceof ApiError ? failed.reason.message : 'Unexpected error';
    else error = null;
    tellReadError();
  }
  const reload = () => load(true).catch(() => {});

  // The live channel says when a list changed (albums came, or an action or its Undo went through): read it again.
  $effect(() =>
    on<{ area: string }>('changed', ({ area }) => {
      if (area === 'incoming') readIncoming(true).then(tellReadError, () => {});
      if (area === 'collection') readCollection(true).then(tellReadError, () => {});
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

  // An Incoming album's sheet: what it offers depends on its server's Incoming list. A Music album's menu is the
  // list's own (false: not handled here).
  function openMenu(shelf: ShelfSource, a: ListedAlbum) {
    const s = incomingOf(shelf.id, a);
    if (!s) return false;
    menu = { source: s.id, a, offer: offerOf(s, a.id) };
    return true;
  }
  const verdictOf = (shelf: ShelfSource, a: ListedAlbum) => incomingOf(shelf.id, a)?.verdicts?.[a.id];
</script>

<main>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if sources === null}
    <PageLoading />
  {:else if sources.length === 0}
    <p class="empty">No server with a Music or Incoming library is connected.</p>
  {:else}
    <AlbumShelf {sources} empty="No albums." onmenu={openMenu} bind:order={prefs.libraryOrder} bind:down={prefs.libraryDown}>
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
            <span class="badge">{v.kind === 'upgrade' ? 'Upgrade' : `In ${music ?? 'Music'}`}</span>
            {v.why}
          </span>
        {/if}
      {/snippet}
    </AlbumShelf>
  {/if}
</main>

{#if menu}
  {@const m = menu}
  <IncomingSheet
    source={m.source}
    album={m.a}
    offer={m.offer}
    {trashDays}
    music={music ?? 'Music'}
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
