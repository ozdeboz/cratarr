<script lang="ts">
  import { ApiError, getIncoming, type ListedAlbum, type IncomingSource, type ShelfSource } from './lib/api';
  import AlbumShelf from './lib/AlbumShelf.svelte';
  import Meter from './lib/Meter.svelte';
  import IncomingSheet, { offerOf, type IncomingOffer } from './lib/IncomingSheet.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { toast } from './lib/toast.svelte';
  import { jobs } from './lib/jobs.svelte';
  import { on, stream } from './lib/events.svelte';

  let sources = $state<IncomingSource[] | null>(null);
  let trashDays = $state(14);
  /** The library Keep and Replace put albums in, by its own name. */
  let music = $state<string | null>(null);
  let error = $state<string | null>(null);

  // The long-pressed album's sheet (Play, View, Keep or Replace, Delete).
  let menu = $state<{ source: string; a: ListedAlbum; offer: IncomingOffer } | null>(null);

  // The album left the list (kept, replaced, deleted): gone at once, before the list is read again.
  function gone(source: string, id: string) {
    sources = (sources ?? []).map((s) => (s.id === source ? { ...s, albums: s.albums.filter((x) => x.id !== id) } : s));
  }

  // The live channel says when the list changed (albums came, or an action or its Undo went through): read it again.
  $effect(() => on<{ area: string }>('changed', ({ area }) => area === 'incoming' && reload()));
  // Back after it was down: what changed meanwhile was not told.
  let wasConnected = stream.connected;
  $effect(() => {
    if (stream.connected && !wasConnected) reload();
    wasConnected = stream.connected;
  });
  // Without it: a queued album finished, so read the list again (it is in Music now, or back as it was).
  let seenFinished = jobs.finished;
  $effect(() => {
    if (jobs.finished !== seenFinished) {
      seenFinished = jobs.finished;
      if (!stream.connected) reload();
    }
  });

  // A server's list could not be read while albums are shown (they stay): one toast says so, not a line that comes and
  // goes; it goes when the list reads again. With nothing to show, the page says it in place.
  let readError: string | null = null;
  function tellReadError() {
    const now = (sources ?? []).find((s) => s.error && s.albums.length > 0)?.error ?? null;
    if (now && now !== readError) toast.show({ text: now, tone: 'error' });
    if (!now && readError && toast.current?.text === readError) toast.dismiss();
    readError = now;
  }

  async function reload() {
    try {
      ({ sources, trash_days: trashDays, music } = await getIncoming(prefs.source || undefined, true));
      tellReadError();
    } catch {
      /* the regular refresh will try again */
    }
  }

  // The server follows the source every 30 s and keeps the list. First show whatever copy answers fastest (the
  // service worker's), then ask for the latest; then every 30 s while the live channel is down.
  $effect(() => {
    const want = prefs.source || undefined;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function load(fresh: boolean) {
      try {
        ({ sources, trash_days: trashDays, music } = await getIncoming(want, fresh));
        error = null;
        tellReadError();
      } catch (e) {
        if (!sources) error = e instanceof ApiError ? e.message : 'Unexpected error';
      }
    }
    // The latest once; after that only while the live channel is down (it says when the list changes).
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

  // The long-pressed album's sheet: what Incoming offers for it depends on its server's list.
  function openMenu(shelf: ShelfSource, a: ListedAlbum) {
    const s = (sources ?? []).find((x) => x.id === shelf.id);
    if (s) menu = { source: s.id, a, offer: offerOf(s, a.id) };
  }
  const verdictOf = (shelf: ShelfSource, a: ListedAlbum) => (sources ?? []).find((x) => x.id === shelf.id)?.verdicts?.[a.id];
</script>

<main>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if sources === null}
    <p class="empty">Loading…</p>
  {:else if sources.length === 0}
    <p class="empty">No server with an Incoming library is connected.</p>
  {:else}
    <AlbumShelf {sources} empty="Nothing incoming." onmenu={openMenu}>
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
