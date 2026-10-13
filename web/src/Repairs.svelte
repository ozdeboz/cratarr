<script lang="ts">
  import {
    ApiError,
    applyOne,
    applySafe,
    dedupe,
    getDuplicates,
    getRepairs,
    undo,
    type DupGroup,
    type Repair,
    type Repairs,
  } from './lib/api';
  import Notice from './lib/Notice.svelte';
  import { toast } from './lib/toast.svelte';
  import { getFileProblems, type FileProblem } from './lib/api';

  // Files the catalogue could not read properly, by kind, worst first.
  let fileProblems = $state<FileProblem[]>([]);
  $effect(() => {
    getFileProblems()
      .then((p) => (fileProblems = p))
      .catch(() => {
        /* the catalogue may not be ready; the section just stays away */
      });
  });
  const KINDS: { kind: FileProblem['kind']; title: string; hint: string }[] = [
    { kind: 'broken', title: "Files that can't be read", hint: 'Not readable as audio at all. Usually a damaged or incomplete file; replace it.' },
    {
      kind: 'damaged_tag',
      title: 'Files with a damaged tag',
      hint: 'One tag field is broken and was skipped; the music and the other tags are fine. Saving the tags again in a tag editor fixes it.',
    },
    { kind: 'no_tags', title: 'Files without tags', hint: 'No artist, title or other tags at all.' },
    { kind: 'no_length', title: 'Files without a length', hint: "The audio's length could not be read." },
  ];
  const ofKind = (k: FileProblem['kind']) => fileProblems.filter((p) => p.kind === k);

  let data = $state<Repairs | null>(null);
  let dups = $state<DupGroup[]>([]);
  let error = $state<string | null>(null);
  let confirming = $state(false);
  let busy = $state(false);

  async function load() {
    try {
      [data, dups] = await Promise.all([getRepairs(), getDuplicates()]);
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'Unexpected error';
    }
  }

  $effect(() => {
    load();
  });

  const playlistsOf = (rs: Repair[]) => [...new Set(rs.map((r) => r.playlist))];
  const fileOf = (path: string) => path.slice(path.lastIndexOf('/') + 1);
  const albumOf = (path: string) => path.slice(0, path.lastIndexOf('/'));

  const tracks = (n: number) => `${n} ${n === 1 ? 'track' : 'tracks'}`;
  const inPlaylists = (n: number) => `${n} ${n === 1 ? 'playlist' : 'playlists'}`;

  async function run(action: () => Promise<{ playlists: string[] }>, what: string) {
    if (busy) return;
    busy = true;
    confirming = false;
    try {
      const { playlists } = await action();
      toast.show({ text: `${what} in ${inPlaylists(playlists.length)}.`, undo: () => undoChanges(playlists) });
    } catch (e) {
      toast.fail(e, 'Something went wrong');
    } finally {
      busy = false;
      await load();
    }
  }

  async function undoChanges(ids: string[]) {
    busy = true;
    try {
      await undo(ids);
      toast.show({ text: 'Undone.' });
    } catch (e) {
      toast.fail(e, 'Could not undo');
    } finally {
      busy = false;
      await load();
    }
  }

  async function removeDups(playlist: string, extra: number) {
    await run(async () => {
      await dedupe(playlist);
      return { playlists: [playlist] };
    }, `Removed ${extra} repeated ${extra === 1 ? 'track' : 'tracks'}`);
  }

  const safe = $derived(data?.safe ?? []);
  const review = $derived(data?.review ?? []);
  const safeByPlaylist = $derived.by(() => {
    const groups = new Map<string, Repair[]>();
    for (const r of safe) groups.set(r.playlist, [...(groups.get(r.playlist) ?? []), r]);
    return [...groups.entries()];
  });
</script>

<main>
  <p class="lede">Missing and repeated tracks in playlists (every change can be undone), and music files that can't be read properly.</p>

  {#if error}<Notice text={error} />{/if}

  {#if !data && !error}
    <p class="empty">Loading…</p>
  {:else if data && safe.length === 0 && review.length === 0 && dups.length === 0 && fileProblems.length === 0}
    <p class="empty">Nothing to repair.</p>
  {:else if data}
    {#if safe.length > 0}
      <section>
        <h2>Found in another format <span class="n">{tracks(safe.length)}</span></h2>
        <p class="hint">The same track is in your Music in another format, for example a FLAC that replaced an MP3.</p>
        {#if confirming}
          <p class="ask">Point {tracks(safe.length)} in {inPlaylists(playlistsOf(safe).length)} at the new files?</p>
          <div class="row">
            <button class="primary" onclick={() => run(applySafe, `Fixed ${tracks(safe.length)}`)} disabled={busy} aria-busy={busy}>
              Fix {tracks(safe.length)}
            </button>
            <button onclick={() => (confirming = false)}>Cancel</button>
          </div>
        {:else}
          <button class="primary" onclick={() => (confirming = true)} disabled={busy}>Fix {safe.length} missing {safe.length === 1 ? 'track' : 'tracks'}</button>
        {/if}
        <ul class="rows">
          {#each safeByPlaylist as [playlist, rs] (playlist)}
            <li>
              <strong>{playlist}</strong> <span class="n">{tracks(rs.length)}</span>
              <ul>
                {#each rs as r (r.from)}
                  <li title={albumOf(r.from)}>{fileOf(r.from)} <span class="arrow">→</span> {fileOf(r.to)}</li>
                {/each}
              </ul>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if review.length > 0}
      <section>
        <h2>Likely match <span class="n">{tracks(review.length)}</span></h2>
        <p class="hint">A missing track with a file of the same number and a similar title. Check before using it.</p>
        <ul class="rows">
          {#each review as r (r.playlist + r.from)}
            <li class="review">
              <div>
                <strong>{r.playlist}</strong>
                <div class="pair" title={albumOf(r.from)}>{fileOf(r.from)} <span class="arrow">→</span> {fileOf(r.to)}</div>
              </div>
              <button onclick={() => run(() => applyOne(r), 'Used the matching file')} disabled={busy}>Use this file</button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if fileProblems.length > 0}
      <h2 class="group">Files with problems</h2>
      {#each KINDS as k (k.kind)}
        {@const list = ofKind(k.kind)}
        {#if list.length > 0}
          <details class="files">
            <summary>{k.title} <span class="n">{list.length}</span></summary>
            <p class="hint">{k.hint}</p>
            <ul class="rows">
              {#each list as p (p.folder + p.rel)}
                <li title={p.folder}>
                  <strong>{fileOf(p.rel)}</strong>
                  <div class="pair">{albumOf(p.rel)}{#if p.detail} · {p.detail}{/if}</div>
                </li>
              {/each}
            </ul>
          </details>
        {/if}
      {/each}
    {/if}

    {#if dups.length > 0}
      <section>
        <h2>Repeated tracks <span class="n">{tracks(dups.reduce((n, g) => n + g.extra, 0))}</span></h2>
        <p class="hint">The same track more than once in a playlist. Removing keeps the first.</p>
        <ul class="rows">
          {#each dups as g (g.playlist)}
            <li class="review">
              <div>
                <strong>{g.playlist}</strong> <span class="n">{g.extra} repeated</span>
                <div class="pair">{g.entries.slice(0, 3).map((e) => fileOf(e.entry)).join(' · ')}{g.entries.length > 3 ? ' …' : ''}</div>
              </div>
              <button onclick={() => removeDups(g.playlist, g.extra)} disabled={busy}>Remove {g.extra === 1 ? 'repeat' : 'repeats'}</button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</main>

<style>
  main {
    max-width: 900px;
    padding: 16px clamp(12px, 2vw, 32px) 48px;
  }

  h2 {
    margin: 28px 0 6px;
    font-size: 1rem;
  }

  .lede,
  .hint,
  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .empty {
    margin-top: 32px;
  }

  .n {
    margin-left: 6px;
    color: var(--muted);
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }

  button {
    min-height: 40px;
    padding: 0 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
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

  .lede {
    margin: 0 0 14px;
  }

  h2.group {
    margin-top: 36px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  details.files {
    margin: 10px 0;
  }

  details.files summary {
    cursor: pointer;
    font-weight: 600;
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

  .rows,
  .rows ul {
    margin: 12px 0 0;
    padding: 0;
    list-style: none;
  }

  .rows > li {
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }

  .rows ul li {
    padding: 2px 0 2px 14px;
    color: var(--muted);
    font-size: 0.85rem;
    overflow-wrap: anywhere;
  }

  .arrow {
    color: var(--accent);
  }

  .review {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .pair {
    margin-top: 2px;
    color: var(--muted);
    font-size: 0.85rem;
    overflow-wrap: anywhere;
  }
</style>
