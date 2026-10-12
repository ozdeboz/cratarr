<script lang="ts">
  import { ApiError, applyOne, applySafe, getRepairs, undo, type Repair, type Repairs } from './lib/api';

  let data = $state<Repairs | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<{ text: string; undo: string[] } | null>(null);
  let confirming = $state(false);
  let busy = $state(false);

  async function load() {
    try {
      data = await getRepairs();
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

  async function run(action: () => Promise<{ playlists: string[] }>, what: string) {
    if (busy) return;
    busy = true;
    confirming = false;
    try {
      const { playlists } = await action();
      notice = { text: `${what} (${playlists.length} ${playlists.length === 1 ? 'playlist' : 'playlists'}).`, undo: playlists };
    } catch (e) {
      notice = { text: e instanceof ApiError ? e.message : 'Something went wrong', undo: [] };
    } finally {
      busy = false;
      await load();
    }
  }

  async function undoLast() {
    if (!notice || busy) return;
    busy = true;
    const ids = notice.undo;
    try {
      await undo(ids);
      notice = { text: 'Undone.', undo: [] };
    } catch (e) {
      notice = { text: e instanceof ApiError ? e.message : 'Something went wrong', undo: [] };
    } finally {
      busy = false;
      await load();
    }
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
  <h1>Repairs</h1>
  <p class="lede">
    Lines in playlists that point at a file that no longer exists. Every change keeps a backup and can be undone.
  </p>

  {#if error}<div class="banner error" role="alert">{error}</div>{/if}
  {#if notice}
    <div class="banner" role="status">
      {notice.text}
      {#if notice.undo.length > 0}<button class="link" onclick={undoLast} disabled={busy}>Undo</button>{/if}
    </div>
  {/if}

  {#if !data && !error}
    <p class="empty">Checking…</p>
  {:else if data && safe.length === 0 && review.length === 0}
    <p class="empty">Nothing to repair.</p>
  {:else if data}
    <section>
      <h2>Safe to fix <span class="n">{safe.length}</span></h2>
      {#if safe.length > 0}
        <p class="hint">The track is in the library in another format (for example a FLAC upgrade of an MP3).</p>
        {#if confirming}
          <button class="primary" onclick={() => run(applySafe, `Fixed ${safe.length} lines`)} disabled={busy}>
            Rewrite {safe.length} lines in {playlistsOf(safe).length} playlists
          </button>
          <button onclick={() => (confirming = false)}>Cancel</button>
        {:else}
          <button class="primary" onclick={() => (confirming = true)} disabled={busy}>Fix all {safe.length}</button>
        {/if}
        <ul class="rows">
          {#each safeByPlaylist as [playlist, rs] (playlist)}
            <li>
              <strong>{playlist}</strong> <span class="n">{rs.length}</span>
              <ul>
                {#each rs as r (r.from)}
                  <li title={albumOf(r.from)}>{fileOf(r.from)} <span class="arrow">→</span> {fileOf(r.to)}</li>
                {/each}
              </ul>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if review.length > 0}
      <section>
        <h2>Needs a look <span class="n">{review.length}</span></h2>
        <p class="hint">Same track number, similar but different title. Check before accepting.</p>
        <ul class="rows">
          {#each review as r (r.playlist + r.from)}
            <li class="review">
              <div>
                <strong>{r.playlist}</strong>
                <div class="pair" title={albumOf(r.from)}>{fileOf(r.from)} <span class="arrow">→</span> {fileOf(r.to)}</div>
              </div>
              <button onclick={() => run(() => applyOne(r), 'Accepted')} disabled={busy}>Accept</button>
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
    padding: 8px clamp(12px, 2vw, 32px) 48px;
  }

  h1 {
    margin: 16px 0 4px;
    font-size: 1.25rem;
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

  button.link {
    min-height: 0;
    margin-left: 10px;
    padding: 0;
    background: none;
    border: 0;
    color: var(--accent);
    text-decoration: underline;
  }

  .banner {
    margin: 12px 0;
    padding: 10px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    font-size: 0.9rem;
  }

  .banner.error {
    border-color: var(--danger);
    color: var(--danger);
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
