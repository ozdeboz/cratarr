<script lang="ts">
  // The panel's Repair tab (decision 0025; what the Repairs page offered for playlists), for the selected playlists:
  // missing tracks found in another format (fixed together), likely matches (one at a time), and missing tracks with
  // no file anywhere (Not found: removed one by one or all), and tracks a playlist holds more than once (Repeated: the
  // repeats taken out on request, the first copy staying; decision 0025, amended 2026-10-16). Every change can be
  // undone.
  import { batchRows, missingCall } from './api';
  import { trackSteps } from './progress.svelte';
  import {
    applyOne,
    applySafe,
    editPlaylist,
    getRepairs,
    playlistRows,
    removeRepeats,
    type PlaylistRow,
    type Repair,
  } from './api';
  import FileItem from './FileItem.svelte';
  import { refresh } from './live.svelte';
  import PageLoading from './PageLoading.svelte';
  import Section from './Section.svelte';
  import { shownPlaylists } from './output.svelte';
  import { prefs } from './prefs.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  let { ids }: { ids: string[] } = $props();

  const mine = (playlist: string) => ids.some((id) => id.toLowerCase() === playlist.toLowerCase());
  let safe = $state<Repair[]>([]);
  let review = $state<Repair[]>([]);
  /** Each selected playlist's missing lines, and its repeated ones (each once, with how many times it is there). */
  let missingOf = $state<Record<string, PlaylistRow[]>>({});
  let repeatsOf = $state<Record<string, { row: PlaylistRow; times: number }[]>>({});
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let confirming = $state(false);

  async function load() {
    const want = [...ids];
    try {
      const r = await getRepairs();
      // One call where the server has it (0.9.4); else each playlist's, the bar saying how far.
      const rows = await batchRows(want, prefs.source || undefined)
        .then((r) => r.playlists.map((x) => [x.id, 'rows' in x ? x.rows : []] as const))
        .catch((e) => {
          if (!missingCall(e)) throw e;
          return trackSteps(want.length, (done) =>
            Promise.all(want.map((id) => playlistRows(id, prefs.source || undefined).then((x) => (done(), [id, x.rows] as const)))),
          );
        });
      if (want.join('\n') !== ids.join('\n')) return;
      safe = r.safe.filter((x) => mine(x.playlist));
      review = r.review.filter((x) => mine(x.playlist));
      missingOf = Object.fromEntries(rows.map(([id, all]) => [id, all.filter((row) => row.missing)]));
      repeatsOf = Object.fromEntries(rows.map(([id, all]) => [id, repeated(all)]));
      error = null;
    } catch (e) {
      if (!loaded) error = e instanceof Error ? e.message : 'Unexpected error';
      else toast.fail(e, 'Could not reload the repairs');
    } finally {
      loaded = true;
    }
  }
  // Asked again when the selected playlists' missing counts change, and when they first arrive: opened from the
  // address, the tab can ask before the list is in, and would find nothing missing.
  const deadKey = $derived(ids.map((id) => shownPlaylists().find((p) => p.id === id)?.dead ?? '').join('\n'));
  let shownFor = '';
  $effect(() => {
    const key = ids.join('\n');
    void deadKey;
    if (key !== shownFor) {
      shownFor = key;
      loaded = false;
      confirming = false;
    }
    load();
  });

  // Not found: missing lines that no repair is offered for.
  const offered = $derived(new Set([...safe, ...review].map((r) => `${r.playlist.toLowerCase()}\n${r.from}`)));
  const notFound = $derived(
    ids.flatMap((id) =>
      (missingOf[id] ?? []).filter((r) => !offered.has(`${id.toLowerCase()}\n${r.entry}`)).map((r) => ({ playlist: id, row: r })),
    ),
  );
  const repeats = $derived(ids.flatMap((id) => (repeatsOf[id] ?? []).map((x) => ({ playlist: id, ...x }))));
  const nothing = $derived(loaded && !error && !safe.length && !review.length && !notFound.length && !repeats.length);

  const tracks = (n: number) => `${n} ${n === 1 ? 'track' : 'tracks'}`;
  const inPlaylists = (n: number) => `${n} ${n === 1 ? 'playlist' : 'playlists'}`;
  const several = $derived(ids.length > 1);

  /** A fix: the playlists it changed and its undo token (or one per write). */
  async function run(action: () => Promise<{ playlists: string[]; undo: string | null | (string | null)[] }>, what: string) {
    if (busy) return;
    busy = true;
    confirming = false;
    try {
      const { playlists, undo } = await action();
      withUndo(several ? `${what} in ${inPlaylists(playlists.length)}.` : `${what}.`, [undo].flat(), { after: load });
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      busy = false;
      refresh();
      await load();
    }
  }

  /** The lines a playlist holds more than once: each once, with how many times, in the order first met. */
  function repeated(rows: PlaylistRow[]) {
    const times = new Map<string, { row: PlaylistRow; times: number }>();
    for (const row of rows) {
      const t = times.get(row.entry);
      if (t) t.times++;
      else times.set(row.entry, { row, times: 1 });
    }
    return [...times.values()].filter((x) => x.times > 1);
  }

  /** Takes the repeats out of the playlists that have any, the first copy of each track staying. */
  function removeAllRepeats() {
    const lists = [...new Set(repeats.map((x) => x.playlist))];
    return run(async () => {
      const undo: (string | null)[] = [];
      for (const id of lists) undo.push((await removeRepeats(id)).undo);
      return { playlists: lists, undo };
    }, 'Removed the repeats');
  }

  /** Takes these lines out of their playlists, each playlist written once. */
  function removeLines(items: { playlist: string; row: PlaylistRow }[]) {
    const n = items.length;
    return run(
      async () => {
        const by = new Map<string, Set<string>>();
        for (const x of items) by.set(x.playlist, (by.get(x.playlist) ?? new Set()).add(x.row.entry));
        const done: string[] = [];
        const undo: (string | null)[] = [];
        for (const [playlist, gone] of by) {
          const all = (await playlistRows(playlist, prefs.source || undefined)).rows.map((r) => r.entry);
          const r = await editPlaylist(
            playlist,
            all,
            all.filter((e) => !gone.has(e)),
          );
          done.push(playlist);
          undo.push(r.undo);
        }
        return { playlists: done, undo };
      },
      `Removed ${tracks(n)}`,
    );
  }
</script>

<div class="tab">
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !loaded}
    <PageLoading />
  {:else if nothing}
    <p class="empty">Nothing to repair.</p>
  {:else}
    {#if safe.length > 0}
      <Section title="Found in another format" count={tracks(safe.length)} hint="The same track is in your Music in another format, for example a FLAC that replaced an MP3.">
        <div class="do">
          {#if confirming}
            <p class="ask">Point {tracks(safe.length)} at the new files?</p>
            <div class="row">
              <button class="primary" onclick={() => run(() => applySafe([...ids]), `Fixed ${tracks(safe.length)}`)} disabled={busy} aria-busy={busy}>
                Fix {tracks(safe.length)}
              </button>
              <button onclick={() => (confirming = false)}>Cancel</button>
            </div>
          {:else}
            <button class="primary" onclick={() => (confirming = true)} disabled={busy}>Fix {safe.length} missing {safe.length === 1 ? 'track' : 'tracks'}</button>
          {/if}
        </div>
        <ul class="rows">
          {#each safe as r (r.playlist + r.from)}
            <li><FileItem path={r.from} to={r.to} playlist={several ? r.playlist : undefined} /></li>
          {/each}
        </ul>
      </Section>
    {/if}

    {#if review.length > 0}
      <Section title="Likely match" count={tracks(review.length)} hint="A missing track with a file of the same number and a similar title. Check before using it.">
        <ul class="rows">
          {#each review as r (r.playlist + r.from)}
            <li>
              <FileItem path={r.from} to={r.to} playlist={several ? r.playlist : undefined}>
                {#snippet action()}<button class="text" onclick={() => run(() => applyOne(r), 'Used the matching file')} disabled={busy}>Use this file</button>{/snippet}
              </FileItem>
            </li>
          {/each}
        </ul>
      </Section>
    {/if}

    {#if notFound.length > 0}
      <Section title="Not found" count={tracks(notFound.length)} hint="Missing tracks with no file anywhere in your Music.">
        <div class="do"><button class="primary" onclick={() => removeLines(notFound)} disabled={busy}>Remove {tracks(notFound.length)}</button></div>
        <ul class="rows">
          {#each notFound as x, i (x.playlist + '\n' + x.row.entry + '\n' + i)}
            <li>
              <FileItem path={x.row.entry} name={x.row.name ?? undefined} detail={x.row.artist ?? undefined} playlist={several ? x.playlist : undefined}>
                {#snippet action()}<button class="text" onclick={() => removeLines([x])} disabled={busy}>Remove</button>{/snippet}
              </FileItem>
            </li>
          {/each}
        </ul>
      </Section>
    {/if}
    {#if repeats.length > 0}
      <Section title="Repeated" count={tracks(repeats.length)} hint="Tracks a playlist holds more than once. Removing the repeats keeps the first copy of each.">
        <div class="do"><button class="primary" onclick={removeAllRepeats} disabled={busy}>Remove repeats</button></div>
        <ul class="rows">
          {#each repeats as x (`${x.playlist}\n${x.row.entry}`)}
            <li>
              <FileItem path={x.row.entry} name={x.row.name ?? undefined} detail={`${x.times} times${x.row.artist ? ` · ${x.row.artist}` : ''}`} playlist={several ? x.playlist : undefined} />
            </li>
          {/each}
        </ul>
      </Section>
    {/if}
  {/if}
</div>

<style>
  /* Sections of large rows (lib/Section.svelte, lib/Row.svelte); what is the tab's own is its fixes' buttons. */
  .tab {
    padding: 0 8px;
  }

  .empty,
  .error {
    margin: 12px;
    color: var(--muted);
    font-size: 0.9rem;
  }

  .error {
    color: var(--danger-ink);
  }

  .do {
    margin: 0 12px 4px;
  }

  .do button,
  .row button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 999px;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .do button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .do button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .ask {
    margin: 0 0 8px;
    font-size: 0.92rem;
  }

  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
