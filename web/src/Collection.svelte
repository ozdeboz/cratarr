<script lang="ts">
  // Collection: the albums of each server's Music library, in the order the viewer chooses (lib/AlbumShelf).
  import { ApiError, getCollection, type ShelfSource } from './lib/api';
  import AlbumShelf from './lib/AlbumShelf.svelte';
  import { on, stream } from './lib/events.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { toast } from './lib/toast.svelte';

  let sources = $state<ShelfSource[] | null>(null);
  let error = $state<string | null>(null);

  // A server's list could not be read while albums are shown (they stay): one toast says so.
  let readError: string | null = null;
  function tellReadError() {
    const now = (sources ?? []).find((s) => s.error && s.albums.length > 0)?.error ?? null;
    if (now && now !== readError) toast.show({ text: now, tone: 'error' });
    readError = now;
  }

  async function load(fresh: boolean) {
    try {
      ({ sources } = await getCollection(prefs.source || undefined, fresh));
      error = null;
      tellReadError();
    } catch (e) {
      if (!sources) error = e instanceof ApiError ? e.message : 'Unexpected error';
    }
  }

  // The kept copy first (the service worker's), then the latest; after that the live channel says when it changed,
  // and while it is down the page asks every minute.
  $effect(() => {
    prefs.source;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    let first = true;
    async function follow() {
      if (first || !stream.connected) await load(true);
      first = false;
      if (!stopped) timer = setTimeout(follow, 60_000);
    }
    load(false).then(follow);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });
  $effect(() => on<{ area: string }>('changed', ({ area }) => area === 'collection' && load(true)));
  // Back after it was down: what changed meanwhile was not told.
  let wasConnected = stream.connected;
  $effect(() => {
    if (stream.connected && !wasConnected) load(true);
    wasConnected = stream.connected;
  });
</script>

<main>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if sources === null}
    <p class="empty">Loading…</p>
  {:else if sources.length === 0}
    <p class="empty">No server with a Music library is connected.</p>
  {:else}
    <AlbumShelf {sources} empty="No albums." order={prefs.collectionOrder} />
  {/if}
</main>

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
</style>
