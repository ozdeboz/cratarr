<script lang="ts">
  // A missing track, one at a time (0.9.8, lib/replaceTrack; administrators): the album's release as MusicBrainz has it,
  // the missing tracks first; one tapped is looked for on Soulseek (one search, never in bulk), its candidates listed
  // lossless first, then free slots, then speed; one tapped is fetched and checked (decoded whole, its length, AcoustID
  // naming that recording), and once it passes, Add to album puts it in the album's folder, tagged from MusicBrainz, with
  // Undo. Or a file already on the server (0.10, its download folder: one found on an old drive, put there), checked the
  // same way and added the same way. `replace`: a track already there, swapped for a better copy (its old file to the trash).
  import { untrack } from 'svelte';
  import { undo } from './api';
  import { on } from './events.svelte';
  import { plural } from './insights.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import PageLoading from './PageLoading.svelte';
  import {
    addFetched,
    dropFetch,
    fetchCandidate,
    fetchFile,
    getFetch,
    getRelease,
    getSearch,
    getTrackFiles,
    searchTrack,
    type Candidate,
    type Fetch,
    type Release,
    type ReleaseTrack,
    type Search,
    type TrackFile,
  } from './replaceTrack';
  import { toast } from './toast.svelte';

  let {
    source,
    album,
    replace = null,
    onclose,
  }: { source: string; album: { id: string; title: string }; replace?: { source: string; id: string } | null; onclose: () => void } = $props();

  let release = $state<Release | null>(null);
  let error = $state<string | null>(null);
  let step = $state<'tracks' | 'search' | 'files' | 'fetch'>('tracks');
  let files = $state<TrackFile[] | null>(null);
  /** Where the fetch shown came from, to go back to. */
  let fetchedFrom = $state<'search' | 'files'>('search');
  let wanted = $state<ReleaseTrack | null>(null);
  let searchId = $state<string | null>(null);
  let search = $state<Search | null>(null);
  let fetchId = $state<string | null>(null);
  let fetching = $state<Fetch | null>(null);
  let adding = $state(false);

  // Read once, as the sheet opens (on the album it was opened for).
  untrack(() => getRelease(source, album.id)).then(
    (r) => {
      release = r;
      // Opened on a track (Replace): straight to its search.
      const t = replace ? r.tracks.find((x) => x.have?.source === replace!.source && x.have?.id === replace!.id) : null;
      if (t) void find(t);
    },
    (e) => (error = e instanceof Error ? e.message : String(e)),
  );
  const missing = $derived(release?.tracks.filter((t) => !t.have) ?? []);
  const there = $derived(release?.tracks.filter((t) => t.have) ?? []);
  const clock = (ms: number | null) => (ms ? `${Math.floor(ms / 60000)}:${String(Math.round((ms / 1000) % 60)).padStart(2, '0')}` : '');
  const mb = (n: number) => `${(n / 1048576).toFixed(n < 10485760 ? 1 : 0)} MB`;

  // Asked again every 1.5 s while it runs (the fetch's live event too, where the server sends it).
  async function poll<T extends { state: string }>(get: () => Promise<T>, set: (v: T) => void, done: (v: T) => boolean, alive: () => boolean) {
    while (alive()) {
      try {
        const v = await get();
        if (!alive()) return;
        set(v);
        if (done(v)) return;
      } catch (e) {
        if (alive()) error = e instanceof Error ? e.message : String(e);
        return;
      }
      await new Promise((r) => setTimeout(r, 1500));
    }
  }

  async function find(t: ReleaseTrack) {
    wanted = t;
    step = 'search';
    search = null;
    error = null;
    try {
      const { id } = await searchTrack({ source, id: album.id }, t.recording, replace ?? (t.have ? t.have : undefined));
      searchId = id;
      void poll(
        () => getSearch(id),
        (v) => (search = v),
        (v) => v.state !== 'searching',
        () => searchId === id && step === 'search',
      );
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  /** Back to Soulseek's candidates: asked on where the search was still running when it was left. */
  function backToSearch() {
    step = 'search';
    const id = searchId;
    if (!id || search?.state !== 'searching') return;
    void poll(
      () => getSearch(id),
      (v) => (search = v),
      (v) => v.state !== 'searching',
      () => searchId === id && step === 'search',
    );
  }
  /** The files on the server, the best match for the wanted track first. */
  async function fromFiles() {
    step = 'files';
    files = null;
    error = null;
    try {
      files = await getTrackFiles([wanted?.artist ?? release?.artist ?? '', wanted?.title ?? ''].filter(Boolean).join(' '));
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  /** A Soulseek candidate or a file on the server taken: fetched (or copied) and checked, then the same steps. */
  async function take(c: Candidate | TrackFile) {
    const fromFile = 'path' in c;
    if (!fromFile && !searchId) return;
    if (fromFile && !wanted) return;
    fetchedFrom = fromFile ? 'files' : 'search';
    step = 'fetch';
    fetching = { state: fromFile ? 'checking' : 'queued', percent: null };
    error = null;
    try {
      const { id } = fromFile ? await fetchFile(c.path, { source, id: album.id }, wanted!.recording, replace ?? (wanted!.have ?? undefined)) : await fetchCandidate(searchId!, c.id);
      fetchId = id;
      void poll(
        () => getFetch(id),
        (v) => (fetching = v),
        (v) => v.state === 'passed' || v.state === 'refused' || v.state === 'added',
        () => fetchId === id && step === 'fetch',
      );
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  // The live word on a fetch (`fetch` event), between the polls.
  $effect(() =>
    on<Fetch & { id: string }>('fetch', (f) => {
      if (f.id === fetchId) fetching = f;
    }),
  );

  async function add() {
    if (!fetchId) return;
    adding = true;
    try {
      const { undo: tokens } = await addFetched(fetchId);
      fetchId = null;
      onclose();
      toast.show({ text: `${wanted?.title ?? 'The track'} added.`, tone: 'done', act: { label: 'Undo', run: () => void undo(tokens) } });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      adding = false;
    }
  }
  /** A refused or unwanted fetch dropped; back to the candidates. */
  async function drop() {
    const id = fetchId;
    fetchId = null;
    fetching = null;
    if (fetchedFrom === 'files') step = 'files';
    else backToSearch();
    if (id) await dropFetch(id).catch(() => {});
  }
  const fetchLabel = (f: Fetch) =>
    f.state === 'queued'
      ? 'Waiting for its uploader'
      : f.state === 'downloading'
        ? `Downloading${f.percent !== null ? ` ${f.percent}%` : ''}`
        : f.state === 'checking'
          ? 'Checking it'
          : f.state === 'passed'
            ? 'It is the right recording'
            : f.state === 'refused'
              ? 'Not the right copy'
              : 'Added';
</script>

<MenuSheet
  label="Missing tracks"
  icon="search"
  title={step === 'tracks' ? album.title : (wanted?.title ?? album.title)}
  chips={step === 'tracks' && release ? [release.artist ?? '', release.year ? String(release.year) : '', missing.length ? `${plural(missing.length, 'track')} missing` : 'Complete'].filter(Boolean) : []}
  {onclose}
  step={step === 'tracks' ? 'menu' : step}
  onback={step === 'tracks' || (replace && step === 'search') ? null : step === 'fetch' ? () => void drop() : step === 'files' ? backToSearch : () => ((step = 'tracks'), (searchId = null))}
>
  {#if error}
    <p class="why">{error}</p>
  {/if}
  {#if step === 'tracks'}
    {#if !release && !error}
      <PageLoading />
    {:else if release}
      <MenuCards>
        {#if missing.length}
          <MenuGroup>
            {#each missing as t (t.recording)}
              <MenuEntry icon="search" label={`${t.disc > 1 ? `${t.disc}-` : ''}${t.position}. ${t.title}`} sub={['Missing', clock(t.lengthMs)].filter(Boolean).join(' · ')} onclick={() => void find(t)} />
            {/each}
          </MenuGroup>
        {/if}
        <MenuGroup>
          {#each there as t (t.recording)}
            <MenuEntry icon="check" label={`${t.disc > 1 ? `${t.disc}-` : ''}${t.position}. ${t.title}`} sub={['There', clock(t.lengthMs)].filter(Boolean).join(' · ')} onclick={() => void find(t)} />
          {/each}
        </MenuGroup>
      </MenuCards>
    {/if}
  {:else if step === 'search'}
    {#if !search || search.state === 'searching'}
      <PageLoading />
    {:else if search.state === 'failed'}
      <p class="why">{search.why ?? 'The search did not finish.'}</p>
    {:else}
      <MenuCards>
        <MenuGroup>
          {#each search.candidates as c (c.id)}
            <MenuEntry
              icon="arrowDownToLine"
              label={c.file.split(/[\\/]/).pop() ?? c.file}
              sub={[
                c.lossless ? [c.format, c.bitDepth && c.sampleRate ? `${c.bitDepth}/${Math.round(c.sampleRate / 100) / 10}` : null].filter(Boolean).join(' ') : `${c.format} ${c.bitrate ?? ''} kbps`,
                mb(c.size),
                c.freeSlot ? 'free slot' : c.queue !== null ? `${c.queue} waiting` : null,
                c.user,
              ]
                .filter(Boolean)
                .join(' · ')}
              onclick={() => void take(c)}
            />
          {:else}
            <p class="why">No copy of it was found.</p>
          {/each}
        </MenuGroup>
        {#if search.bandcamp}
          <MenuGroup>
            <MenuEntry icon="externalLink" label="On Bandcamp" onclick={() => window.open(search!.bandcamp!, '_blank', 'noopener')} />
          </MenuGroup>
        {/if}
      </MenuCards>
    {/if}
    <!-- A file already on the server, whatever Soulseek finds (or while it still looks). -->
    <MenuCards>
      <MenuGroup>
        <MenuEntry icon="intoBox" label="From a file on the server" onclick={() => void fromFiles()} />
      </MenuGroup>
    </MenuCards>
  {:else if step === 'files'}
    {#if !files && !error}
      <PageLoading />
    {:else if files}
      <MenuCards>
        <MenuGroup>
          {#each files as f (f.path)}
            <MenuEntry
              icon="intoBox"
              label={f.title ? `${f.trackNo ? `${f.trackNo}. ` : ''}${f.title}` : f.name}
              sub={[f.artist, f.album, f.durationSecs ? clock(f.durationSecs * 1000) : null, mb(f.bytes)].filter(Boolean).join(' · ')}
              onclick={() => void take(f)}
            />
          {:else}
            <p class="why">No audio files in the server's download folder.</p>
          {/each}
        </MenuGroup>
      </MenuCards>
    {/if}
  {:else if fetching}
    <MenuCards>
      <MenuGroup>
        <MenuEntry
          icon={fetching.state === 'refused' ? 'close' : fetching.state === 'passed' ? 'circleCheck' : 'arrowDownToLine'}
          label={fetchLabel(fetching)}
          sub={fetching.why ?? null}
          progress={fetching.state === 'downloading' && fetching.percent !== null ? fetching.percent / 100 : null}
          busy={fetching.state === 'queued' || fetching.state === 'checking'}
          onclick={() => {}}
        />
      </MenuGroup>
      <MenuGroup>
        {#if fetching.state === 'passed'}
          <MenuEntry icon="intoBox" label="Add to album" busy={adding} onclick={() => void add()} />
        {/if}
        {#if fetching.state === 'refused' || fetching.state === 'passed'}
          <MenuEntry icon="back" label="Another copy" onclick={() => void drop()} />
        {/if}
      </MenuGroup>
    </MenuCards>
  {/if}
</MenuSheet>

<style>
  .why {
    margin: 8px 16px;
    color: var(--danger-ink);
    font-size: 0.9rem;
  }
</style>
