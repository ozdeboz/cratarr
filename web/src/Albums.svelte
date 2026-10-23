<script lang="ts">
  import { ApiError, getAlbumDuplicates, type Album, type AlbumGroup } from './lib/api';

  let groups = $state<AlbumGroup[] | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    getAlbumDuplicates()
      .then((r) => (groups = r.groups))
      .catch((e) => (error = e instanceof ApiError ? e.message : 'Unexpected error'));
  });

  const mb = (b: number) => `${(b / 1048576).toFixed(0)} MB`;
  const kb = (b: number | null) => (b === null ? 'none' : `${Math.round(b / 1024)} KB`);
  const lines = (a: Album) => a.playlist_lines.reduce((n, [, c]) => n + c, 0);
  const time = (s: number | null) => (s === null ? '–' : `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, '0')}`);
  const wording = { same: 'Same album', similar: 'Same album, files differ', different: 'Different albums' };
</script>

<main>
  <h1>Duplicate albums</h1>
  <p class="lede">Folders that differ only by accents or punctuation, side by side. Nothing here changes your library.</p>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if groups === null}
    <p class="empty">Looking…</p>
  {:else if groups.length === 0}
    <p class="empty">No duplicate album folders found.</p>
  {:else}
    {#each groups as g}
      <section>
        <h2>
          {wording[g.comparisons[0]?.verdict ?? 'different']}
          <span class="n">
            {g.comparisons[0].same_length} of {g.comparisons[0].matched} matching tracks have the same length,
            {g.comparisons[0].same_size} the same file size
          </span>
        </h2>

        <div class="cols" style="--cols: {g.albums.length}">
          {#each g.albums as a}
            <div class="card" class:keep={a.folder === g.suggestion.keep}>
              <h3>{a.folder}</h3>
              {#if a.folder === g.suggestion.keep}<p class="badge">Suggested to keep</p>{/if}
              <dl>
                <dt>Tracks</dt><dd>{a.tracks.length}</dd>
                <dt>Size</dt><dd>{mb(a.total_bytes)}</dd>
                <dt>Cover</dt><dd>{kb(a.cover_bytes)}</dd>
                <dt>Plays</dt><dd>{a.plays}</dd>
                <dt>Favourites</dt><dd>{a.favourites}</dd>
                <dt>In playlists</dt>
                <dd>{lines(a) === 0 ? 'none' : a.playlist_lines.map(([p, c]) => `${p} (${c})`).join(', ')}</dd>
              </dl>
              <details>
                <summary>Tracks</summary>
                <table>
                  <tbody>
                    {#each a.tracks as t}
                      <tr>
                        <td class="num">{t.key ?? ''}</td>
                        <td class="file" title={t.file}>{t.file}</td>
                        <td class="num">{time(t.seconds)}</td>
                        <td class="num">{t.kbps ?? '–'}</td>
                        <td class="num">{t.plays}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </details>
            </div>
          {/each}
        </div>

        {#if g.suggestion.reasons.length}
          <p class="hint">Suggested because: {g.suggestion.reasons.join('; ')}.</p>
        {/if}
      </section>
    {/each}
  {/if}
</main>

<style>
  main {
    padding: 8px clamp(12px, 2vw, 32px) 48px;
  }

  h1 {
    margin: 16px 0 4px;
    font-size: 1.25rem;
  }

  h2 {
    margin: 28px 0 10px;
    font-size: 1rem;
  }

  h3 {
    margin: 0 0 4px;
    font-size: 0.95rem;
    overflow-wrap: anywhere;
  }

  .lede,
  .hint,
  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .n {
    margin-left: 6px;
    color: var(--muted);
    font-weight: 400;
    font-size: 0.85rem;
  }

  .error {
    color: var(--danger-ink);
  }

  .cols {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    gap: 12px;
  }

  @media (max-width: 700px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }

  .card {
    padding: 12px 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
  }

  .card.keep {
    border-color: var(--accent);
  }

  .badge {
    margin: 0 0 8px;
    color: var(--accent-ink);
    font-size: 0.8rem;
    font-weight: 600;
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 14px;
    margin: 8px 0;
    font-size: 0.9rem;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }

  summary {
    cursor: pointer;
    color: var(--muted);
    font-size: 0.85rem;
  }

  table {
    width: 100%;
    margin-top: 6px;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  td {
    padding: 2px 4px;
  }

  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .file {
    max-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
