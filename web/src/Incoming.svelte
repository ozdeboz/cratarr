<script lang="ts">
  import {
    ApiError,
    deleteIncoming,
    getIncoming,
    albumCoverUrl,
    keepIncoming,
    keepPreview,
    replaceIncoming,
    replacePreview,
    cancelJob,
    type AlbumMoved,
    type KeepPreview,
    type ReplacePreview,
    type ListedAlbum,
    type IncomingSource,
    type ShelfSource,
  } from './lib/api';
  import AlbumShelf from './lib/AlbumShelf.svelte';
  import MenuEntry from './lib/MenuEntry.svelte';
  import MenuSheet from './lib/MenuSheet.svelte';
  import { albumGone, albumSubject, viewPlaylist } from './lib/panel.svelte';
  import { prefs } from './lib/prefs.svelte';
  import { toast } from './lib/toast.svelte';
  import { withUndo } from './lib/undo.svelte';
  import { playListedAlbum } from './lib/albums.svelte';
  import { jobs } from './lib/jobs.svelte';
  import { on, stream } from './lib/events.svelte';

  let sources = $state<IncomingSource[] | null>(null);
  let trashDays = $state(14);
  // Said in the Keep and Replace questions when no Lidarr is set to be told to stop fetching the album.
  const NO_LIDARR = "Lidarr isn't set up in Settings, so it may fetch this album again.";
  let error = $state<string | null>(null);

  // The open menu of one album (Play, Keep or Replace in Music, Delete album), anchored to it; `ask` = the in-place
  // question. Keep is offered for an album that is not in Music yet (no verdict), Replace for an upgrade, both only
  // when Cratarr can write the album's folder.
  let menu = $state<{
    source: string;
    a: ListedAlbum;
    anchor: DOMRect;
    ask: false | 'delete' | 'keep' | 'replace';
    deletable: boolean;
    keepable: boolean;
    replaceable: boolean;
    /** Its job in the conversion queue: only Cancel is offered then. */
    job: number | null;
  } | null>(null);
  let keep = $state<KeepPreview | null>(null);
  let swap = $state<ReplacePreview | null>(null);
  let year = $state('');

  async function askReplace() {
    if (!menu) return;
    menu.ask = 'replace';
    swap = null;
    year = '';
    try {
      swap = await replacePreview(menu.source, menu.a.id);
    } catch (e) {
      toast.fail(e, 'Could not pair the tracks');
      menu = null;
    }
  }

  async function replaceIt(source: string, a: ListedAlbum) {
    if (working) return;
    working = true;
    try {
      const r = await replaceIncoming(source, a.id, year.trim() || undefined);
      moved(source, a, r, 'Replaced');
    } catch (e) {
      toast.fail(e, 'Could not replace the album');
    } finally {
      working = false;
      menu = null;
    }
  }

  async function askKeep() {
    if (!menu) return;
    menu.ask = 'keep';
    keep = null;
    year = '';
    try {
      keep = await keepPreview(menu.source, menu.a.id);
    } catch (e) {
      toast.fail(e, 'Could not work out the names');
      menu = null;
    }
  }

  // With a year typed in, show what the names would be.
  async function tryYear() {
    if (!menu || !/^\d{4}$/.test(year.trim())) return;
    try {
      if (menu.ask === 'replace') swap = await replacePreview(menu.source, menu.a.id, year.trim());
      else keep = await keepPreview(menu.source, menu.a.id, year.trim());
    } catch {
      /* keep the question open */
    }
  }

  async function keepIt(source: string, a: ListedAlbum) {
    if (working) return;
    working = true;
    try {
      const r = await keepIncoming(source, a.id, year.trim() || undefined);
      moved(source, a, r, 'Kept');
    } catch (e) {
      toast.fail(e, 'Could not keep the album');
    } finally {
      working = false;
      menu = null;
    }
  }
  let working = $state(false);

  // Done: the album leaves the list. Queued (hi-res files to convert first): its row shows the progress instead, and
  // the queue says when it is done.
  function moved(source: string, a: ListedAlbum, r: AlbumMoved, verb: string) {
    if (r.queued) {
      jobs.poke();
      return;
    }
    sources = (sources ?? []).map((s) => (s.id === source ? { ...s, albums: s.albums.filter((x) => x.id !== a.id) } : s));
    albumGone(source, a.id);
    withUndo(`${verb} ${r.title} in Music.`, [r.undo], { undone: `${r.title} is back in Incoming.`, after: reload });
  }

  async function cancel(id: number) {
    menu = null;
    try {
      await cancelJob(id);
      await jobs.poke();
    } catch (e) {
      toast.fail(e, 'Could not cancel it');
    }
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
      ({ sources, trash_days: trashDays } = await getIncoming(prefs.source || undefined, true));
      tellReadError();
    } catch {
      /* the regular refresh will try again */
    }
  }

  async function remove(source: string, a: ListedAlbum) {
    if (working) return;
    working = true;
    try {
      const { undo, title } = await deleteIncoming(source, a.id);
      sources = (sources ?? []).map((s) => (s.id === source ? { ...s, albums: s.albums.filter((x) => x.id !== a.id) } : s));
      albumGone(source, a.id);
      withUndo(`Deleted ${title}.`, [undo], { undone: `${title} is back in Incoming.`, after: reload });
    } catch (e) {
      toast.fail(e, 'Could not delete the album');
    } finally {
      working = false;
      menu = null;
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
        ({ sources, trash_days: trashDays } = await getIncoming(want, fresh));
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

  // The long-pressed album's menu: what Incoming offers for it depends on its server's list.
  function openMenu(shelf: ShelfSource, a: ListedAlbum, anchor: DOMRect) {
    const s = (sources ?? []).find((x) => x.id === shelf.id);
    if (!s) return;
    menu = {
      source: s.id,
      a,
      anchor,
      ask: false,
      deletable: s.deletable.includes(a.id),
      keepable: s.deletable.includes(a.id) && !s.verdicts?.[a.id],
      replaceable: s.deletable.includes(a.id) && s.verdicts?.[a.id]?.kind === 'upgrade',
      job: jobs.of(s.id, a.id)?.id ?? null,
    };
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
          <span class="job">
            {job.state === 'waiting' ? 'Waiting to convert' : `Converting ${job.done} of ${job.total}`}
            <span class="track" aria-hidden="true"><span style:width="{(100 * job.done) / Math.max(job.total, 1)}%"></span></span>
          </span>
        {:else if verdictOf(s, a)}
          {@const v = verdictOf(s, a)!}
          <span class="verdict" class:upgrade={v.kind === 'upgrade'} title={v.library}>
            <span class="badge">{v.kind === 'upgrade' ? 'Upgrade' : 'In Music'}</span>
            {v.why}
          </span>
        {/if}
      {/snippet}
    </AlbumShelf>
  {/if}
</main>

{#if menu}
  {@const m = menu}
  <!-- The album's menu: a sheet as every menu (decision 0021). -->
  <MenuSheet
    label={m.a.title}
    onclose={() => (menu = null)}
    images={[albumCoverUrl(m.source, m.a.id)]}
    title={m.a.title}
    chips={[m.a.artist ?? 'Unknown artist']}
    actions={m.ask
      ? []
      : [
          {
            icon: 'play',
            label: 'Play',
            onclick: () => {
              const { source, a } = m;
              menu = null;
              playListedAlbum(source, a);
            },
          },
        ]}
    onback={m.ask ? () => menu && (menu.ask = false) : null}
    step={m.ask || 'menu'}
  >
    {#if !m.ask}
      <div role="menu">
        <MenuEntry
          icon="squareArrowOutUpRight"
          label="View"
          onclick={() => {
            // Read before closing: `m` follows `menu`.
            const subject = albumSubject(m.source, m.a.id);
            menu = null;
            viewPlaylist(subject);
          }}
        />
        {#if m.job !== null}
          {@const id = m.job}
          <MenuEntry icon="close" label="Cancel conversion" danger onclick={() => cancel(id)} />
        {:else if m.keepable}
          <MenuEntry icon="check" label="Keep in Music" onclick={askKeep} />
        {/if}
        {#if m.replaceable}
          <MenuEntry icon="swap" label="Replace in Music" onclick={askReplace} />
        {/if}
        {#if m.deletable && m.job === null}
          <MenuEntry icon="trash" label="Delete album" danger onclick={() => menu && (menu.ask = 'delete')} />
        {/if}
      </div>
    {:else if m.ask === 'keep'}
      <div class="form">
        {#if !keep}
          <p class="ask">Working out the names…</p>
        {:else if keep.refused?.kind === 'needs_year'}
          <p class="ask">The tags don't say when {m.a.title} came out. Which year?</p>
          <input class="year" inputmode="numeric" maxlength="4" placeholder="Year" aria-label="Release year" bind:value={year} oninput={tryYear} />
        {:else if keep.refused}
          <p class="ask">Can't keep {m.a.title}: {keep.refused.detail}</p>
        {:else}
          <p class="ask">
            Move {m.a.title} into Music as <strong>{keep.folder}</strong> ({keep.files} {keep.files === 1 ? 'file' : 'files'})?{#if keep.convert}
              {` ${keep.convert} hi-res ${keep.convert === 1 ? 'file is' : 'files are'} converted to 16-bit/44.1 kHz first.`}{/if}{#if !keep.acquisition}
              {` ${NO_LIDARR}`}{/if}
          </p>
        {/if}
        <div class="actions">
          {#if keep && (!keep.refused || keep.refused.kind === 'needs_year')}
            <button class="primary" disabled={working || !!keep.refused} aria-busy={working} onclick={() => keepIt(m.source, m.a)}>Keep in Music</button>
          {/if}
          <button onclick={() => (menu = null)}>Cancel</button>
        </div>
      </div>
    {:else if m.ask === 'replace'}
      <div class="form">
        {#if !swap}
          <p class="ask">Pairing the tracks…</p>
        {:else if swap.refused?.kind === 'needs_year'}
          <p class="ask">The tags don't say when {m.a.title} came out. Which year?</p>
          <input class="year" inputmode="numeric" maxlength="4" placeholder="Year" aria-label="Release year" bind:value={year} oninput={tryYear} />
        {:else if swap.refused?.kind === 'unmatched'}
          <p class="ask">Can't replace {swap.old}: no match for {swap.refused.detail.join(', ')}.</p>
        {:else if swap.refused}
          <p class="ask">Can't replace {swap.old}: {swap.refused.detail}</p>
        {:else}
          <p class="ask">
            Replace <strong>{swap.old}</strong> with this copy ({swap.tracks} {swap.tracks === 1 ? 'track' : 'tracks'}{swap.added
              ? `, ${swap.added} new`
              : ''}){#if swap.folder !== swap.old}, as <strong>{swap.folder}</strong>{/if}? The old copy stays in the trash for {trashDays}
            {trashDays === 1 ? 'day' : 'days'}.{#if swap.convert}
              {` ${swap.convert} hi-res ${swap.convert === 1 ? 'file is' : 'files are'} converted to 16-bit/44.1 kHz first.`}{/if}{#if !swap.acquisition}
              {` ${NO_LIDARR}`}{/if}
          </p>
        {/if}
        <div class="actions">
          {#if swap && (!swap.refused || swap.refused.kind === 'needs_year')}
            <button class="primary" disabled={working || !!swap.refused} aria-busy={working} onclick={() => replaceIt(m.source, m.a)}>Replace</button>
          {/if}
          <button onclick={() => (menu = null)}>Cancel</button>
        </div>
      </div>
    {:else}
      <div class="form">
        <p class="ask">Delete {m.a.title}? It stays in the trash for {trashDays} {trashDays === 1 ? 'day' : 'days'}.</p>
        <div class="actions">
          <button class="danger solid" disabled={working} aria-busy={working} onclick={() => remove(m.source, m.a)}>Delete album</button>
          <button onclick={() => (menu = null)}>Cancel</button>
        </div>
      </div>
    {/if}
  </MenuSheet>
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

  /* In the conversion queue: what it is at, and a thin bar of the files done. */
  .job {
    display: block;
    margin-top: 3px;
    color: var(--accent);
    font-size: 0.8rem;
  }

  .job .track {
    display: block;
    max-width: 240px;
    height: 3px;
    margin-top: 4px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    overflow: hidden;
  }

  .job .track span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.4s;
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

  /* The album menu's steps (inside the sheet, decision 0021) */
  .danger {
    color: var(--danger);
  }

  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .ask {
    margin: 0;
    font-size: 0.92rem;
  }

  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .actions button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .actions button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .year {
    min-height: 40px;
    width: 7em;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  .actions button.danger.solid {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
    font-weight: 600;
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
