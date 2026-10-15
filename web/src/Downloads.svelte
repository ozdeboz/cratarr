<script lang="ts">
  // Downloads (decision 0033, step 4): the tracks kept on this device, by album, as the app tells them. Only in the app;
  // it needs no server, so it is the page to play from without a connection.
  import { askDownloads, downloads, removeDownloads, type DownloadedTrack } from './lib/app.svelte';
  import Icon from './lib/Icon.svelte';
  import { playQueue } from './lib/localPlayer.svelte';

  askDownloads();

  type Album = { key: string; album: string; artist: string | null; tracks: DownloadedTrack[] };
  const albums = $derived.by(() => {
    const by = new Map<string, Album>();
    for (const t of downloads.tracks) {
      const key = t.album_id ?? `${t.album ?? ''}\n${t.artist ?? ''}`;
      const a = by.get(key) ?? { key, album: t.album ?? '', artist: t.artist, tracks: [] };
      a.tracks.push(t);
      by.set(key, a);
    }
    return [...by.values()].sort((a, b) => a.album.localeCompare(b.album));
  });

  const size = (bytes: number) => (bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`);
  // What plays is what is whole on the device; the rest is still coming.
  const playable = (a: Album) => a.tracks.filter((t) => t.download.state === 'done');
  const play = (a: Album, from: DownloadedTrack) => {
    const tracks = playable(a);
    const at = tracks.indexOf(from);
    if (at >= 0) playQueue(tracks, at, null);
  };
</script>

<main>
  {#if downloads.known && !albums.length}
    <p class="empty">Nothing downloaded.</p>
  {:else if albums.length}
    <p class="total">{downloads.tracks.length} tracks · {size(downloads.bytes)}</p>
    {#each albums as a (a.key)}
      <section>
        <header>
          <div class="names">
            <h2>{a.album || '—'}</h2>
            {#if a.artist}<span class="artist">{a.artist}</span>{/if}
          </div>
          <button class="icon" aria-label="Play" disabled={!playable(a).length} onclick={() => play(a, playable(a)[0])}><Icon name="play" /></button>
          <button class="icon" aria-label="Remove" onclick={() => removeDownloads(a.tracks.map((t) => t.download.id))}><Icon name="trash" /></button>
        </header>
        <ol>
          {#each a.tracks as t (t.download.id)}
            <li>
              <button class="row" disabled={t.download.state !== 'done'} onclick={() => play(a, t)}>
                <span class="name">{t.name}</span>
                {#if t.download.state === 'failed'}
                  <span class="state failed">Failed</span>
                {:else if t.download.state !== 'done'}
                  <span class="state">{t.download.percent > 0 ? `${Math.round(t.download.percent)}%` : '…'}</span>
                {/if}
              </button>
            </li>
          {/each}
        </ol>
      </section>
    {/each}
  {/if}
</main>

<style>
  main {
    max-width: 900px;
    padding: 16px clamp(12px, 2vw, 32px) 48px;
  }

  .empty,
  .total {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .empty {
    margin-top: 32px;
  }

  section {
    margin-top: 20px;
  }

  header {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .names {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: 1rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .artist {
    color: var(--muted);
    font-size: 0.85rem;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .icon:disabled {
    opacity: 0.4;
    cursor: default;
  }

  ol {
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 44px;
    padding: 0 4px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row:disabled {
    color: var(--muted);
    cursor: default;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .state {
    color: var(--muted);
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
  }

  .state.failed {
    color: var(--danger);
  }
</style>
