<script lang="ts">
  // The panel's Repair tab (decision 0025; what the Repairs page offered for playlists), for the selected playlists:
  // missing tracks found in another format (fixed together), likely matches (one at a time), repeated tracks, and
  // missing tracks with no file anywhere (Not found: removed one by one or all). Every change can be undone.
  import {
    applyOne,
    applySafe,
    dedupe,
    editPlaylist,
    getDuplicates,
    getRepairs,
    playlistRows,
    undo,
    type DupGroup,
    type PlaylistRow,
    type Repair,
  } from './api';
  import { refresh } from './live.svelte';
  import { shownPlaylists } from './output.svelte';
  import { prefs } from './prefs.svelte';
  import { toast } from './toast.svelte';

  let { ids }: { ids: string[] } = $props();

  const mine = (playlist: string) => ids.some((id) => id.toLowerCase() === playlist.toLowerCase());
  let safe = $state<Repair[]>([]);
  let review = $state<Repair[]>([]);
  let dups = $state<DupGroup[]>([]);
  /** Each selected playlist's missing lines (only asked for when it has any). */
  let missingOf = $state<Record<string, PlaylistRow[]>>({});
  let loaded = $state(false);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let confirming = $state(false);

  async function load() {
    const want = [...ids];
    try {
      const [r, d] = await Promise.all([getRepairs(), getDuplicates()]);
      const dead = shownPlaylists().filter((p) => want.includes(p.id) && p.dead > 0);
      const rows = await Promise.all(dead.map((p) => playlistRows(p.id, prefs.source || undefined).then((x) => [p.id, x.rows.filter((row) => row.missing)] as const)));
      if (want.join('\n') !== ids.join('\n')) return;
      safe = r.safe.filter((x) => mine(x.playlist));
      review = r.review.filter((x) => mine(x.playlist));
      dups = d.filter((g) => mine(g.playlist));
      missingOf = Object.fromEntries(rows);
      error = null;
    } catch (e) {
      if (!loaded) error = e instanceof Error ? e.message : 'Unexpected error';
      else toast.fail(e, 'Could not reload the repairs');
    } finally {
      loaded = true;
    }
  }
  $effect(() => {
    void ids.join('\n');
    loaded = false;
    confirming = false;
    load();
  });

  // Not found: missing lines that no repair is offered for.
  const offered = $derived(new Set([...safe, ...review].map((r) => `${r.playlist.toLowerCase()}\n${r.from}`)));
  const notFound = $derived(
    ids.flatMap((id) =>
      (missingOf[id] ?? []).filter((r) => !offered.has(`${id.toLowerCase()}\n${r.entry}`)).map((r) => ({ playlist: id, row: r })),
    ),
  );
  const nothing = $derived(loaded && !error && !safe.length && !review.length && !dups.length && !notFound.length);

  const fileOf = (path: string) => path.slice(path.lastIndexOf('/') + 1);
  const albumOf = (path: string) => path.slice(0, path.lastIndexOf('/'));
  const tracks = (n: number) => `${n} ${n === 1 ? 'track' : 'tracks'}`;
  const inPlaylists = (n: number) => `${n} ${n === 1 ? 'playlist' : 'playlists'}`;
  const several = $derived(ids.length > 1);

  async function run(action: () => Promise<{ playlists: string[] }>, what: string) {
    if (busy) return;
    busy = true;
    confirming = false;
    try {
      const { playlists } = await action();
      toast.show({ text: several ? `${what} in ${inPlaylists(playlists.length)}.` : `${what}.`, undo: () => undoChanges(playlists) });
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      busy = false;
      refresh();
      await load();
    }
  }

  async function undoChanges(playlists: string[]) {
    busy = true;
    try {
      await undo(playlists);
      toast.show({ text: 'Undone.' });
    } catch (e) {
      toast.fail(e, 'Could not undo');
    } finally {
      busy = false;
      refresh();
      await load();
    }
  }

  /** Takes these lines out of their playlists, each playlist written once. */
  function removeLines(items: { playlist: string; row: PlaylistRow }[]) {
    const n = items.length;
    return run(
      async () => {
        const by = new Map<string, Set<string>>();
        for (const x of items) by.set(x.playlist, (by.get(x.playlist) ?? new Set()).add(x.row.entry));
        const done: string[] = [];
        for (const [playlist, gone] of by) {
          const all = (await playlistRows(playlist, prefs.source || undefined)).rows.map((r) => r.entry);
          await editPlaylist(
            playlist,
            all,
            all.filter((e) => !gone.has(e)),
          );
          done.push(playlist);
        }
        return { playlists: done };
      },
      `Removed ${tracks(n)}`,
    );
  }
</script>

<div class="tab">
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !loaded}
    <p class="empty">Loading…</p>
  {:else if nothing}
    <p class="empty">Nothing to repair.</p>
  {:else}
    {#if safe.length > 0}
      <section>
        <h3>Found in another format <span class="n">{tracks(safe.length)}</span></h3>
        <p class="hint">The same track is in your Music in another format, for example a FLAC that replaced an MP3.</p>
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
        <ul class="rows">
          {#each safe as r (r.playlist + r.from)}
            <li title={albumOf(r.from)}>
              {#if several}<strong>{r.playlist}</strong>{/if}
              <div class="pair">{fileOf(r.from)} <span class="arrow">→</span> {fileOf(r.to)}</div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if review.length > 0}
      <section>
        <h3>Likely match <span class="n">{tracks(review.length)}</span></h3>
        <p class="hint">A missing track with a file of the same number and a similar title. Check before using it.</p>
        <ul class="rows">
          {#each review as r (r.playlist + r.from)}
            <li class="item">
              <div>
                {#if several}<strong>{r.playlist}</strong>{/if}
                <div class="pair" title={albumOf(r.from)}>{fileOf(r.from)} <span class="arrow">→</span> {fileOf(r.to)}</div>
              </div>
              <button onclick={() => run(() => applyOne(r), 'Used the matching file')} disabled={busy}>Use this file</button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if notFound.length > 0}
      <section>
        <h3>Not found <span class="n">{tracks(notFound.length)}</span></h3>
        <p class="hint">Missing tracks with no file anywhere in your Music.</p>
        <button class="primary" onclick={() => removeLines(notFound)} disabled={busy}>Remove {tracks(notFound.length)}</button>
        <ul class="rows">
          {#each notFound as x, i (x.playlist + '\n' + x.row.entry + '\n' + i)}
            <li class="item">
              <div>
                {#if several}<strong>{x.playlist}</strong>{/if}
                <div class="pair" title={albumOf(x.row.entry)}>{x.row.name ?? fileOf(x.row.entry)}{#if x.row.artist}&nbsp;· {x.row.artist}{/if}</div>
              </div>
              <button onclick={() => removeLines([x])} disabled={busy}>Remove</button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if dups.length > 0}
      <section>
        <h3>Repeated tracks <span class="n">{tracks(dups.reduce((n, g) => n + g.extra, 0))}</span></h3>
        <p class="hint">The same track more than once in a playlist. Removing keeps the first.</p>
        <ul class="rows">
          {#each dups as g (g.playlist)}
            <li class="item">
              <div>
                <strong>{g.playlist}</strong> <span class="n">{g.extra} repeated</span>
                <div class="pair">{g.entries.slice(0, 3).map((e) => fileOf(e.entry)).join(' · ')}{g.entries.length > 3 ? ' …' : ''}</div>
              </div>
              <button
                onclick={() =>
                  run(
                    async () => {
                      await dedupe(g.playlist);
                      return { playlists: [g.playlist] };
                    },
                    `Removed ${g.extra} repeated ${g.extra === 1 ? 'track' : 'tracks'}`,
                  )}
                disabled={busy}>Remove {g.extra === 1 ? 'repeat' : 'repeats'}</button
              >
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>

<style>
  .tab {
    padding: 0 16px;
  }

  section + section {
    margin-top: 8px;
  }

  h3 {
    margin: 14px 0 4px;
    font-size: 15px;
    font-weight: 600;
  }

  .hint,
  .empty {
    margin: 0 0 8px;
    color: var(--muted);
    font-size: 0.9rem;
  }

  .n {
    margin-left: 6px;
    color: var(--muted);
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }

  button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .ask {
    margin: 8px 0;
    font-size: 0.92rem;
  }

  .row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .rows {
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
  }

  .rows > li {
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
    font-size: 0.9rem;
  }

  .item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .arrow {
    color: var(--accent);
  }

  .pair {
    margin-top: 2px;
    color: var(--muted);
    font-size: 0.85rem;
    overflow-wrap: anywhere;
  }

  .error {
    color: var(--danger, #c33);
  }
</style>
