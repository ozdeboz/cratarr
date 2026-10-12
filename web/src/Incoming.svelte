<script lang="ts">
  import { ApiError, getIncoming, getState, incomingCoverUrl, type IncomingAlbum, type IncomingSource, type NowPlaying } from './lib/api';
  import { prefs } from './lib/prefs.svelte';

  let sources = $state<IncomingSource[] | null>(null);
  let error = $state<string | null>(null);
  let noCover = $state<Record<string, boolean>>({});
  let filter = $state('');
  let playing = $state<NowPlaying | null>(null);

  // What is playing, so the album being listened to stands out. Cheap: the state call is the one the Playlists page polls.
  $effect(() => {
    const want = prefs.source || undefined;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        playing = (await getState(want)).now_playing;
      } catch {
        /* keep the last known track */
      }
      if (!stopped) timer = setTimeout(poll, 5000);
    }
    poll();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });

  $effect(() => {
    const want = prefs.source || undefined;
    getIncoming(want)
      .then((r) => {
        sources = r.sources;
        error = null;
      })
      .catch((e) => (error = e instanceof ApiError ? e.message : 'Unexpected error'));
  });

  const lossless = (a: IncomingAlbum) => a.format === 'FLAC';
  const quality = (a: IncomingAlbum) =>
    [a.format ?? 'Mixed', lossless(a) || a.bitrate_kbps === null ? null : `${a.bitrate_kbps} kbps`].filter(Boolean).join(' ');
  const added = (t: number | null) =>
    t === null ? '' : new Date(t * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
  const matches = (a: IncomingAlbum) => {
    const q = filter.trim().toLowerCase();
    return !q || `${a.artist ?? ''} ${a.title}`.toLowerCase().includes(q);
  };
</script>

<main>
  <h1>Incoming</h1>
  <p class="lede">New music waiting to be listened to, newest first.</p>

  {#if playing}
    <div class="np">
      {#if playing.album_id && !noCover['np:' + playing.album_id]}
        <img
          class="cover"
          src={incomingCoverUrl(playing.source, playing.album_id)}
          alt=""
          decoding="async"
          onerror={() => playing?.album_id && (noCover['np:' + playing.album_id] = true)}
        />
      {:else}
        <div class="cover none" aria-hidden="true"></div>
      {/if}
      <div class="text">
        <div class="label">Now playing</div>
        <div class="title">{playing.name}</div>
        <div class="artist">{[playing.artist, playing.album].filter(Boolean).join(' · ')}</div>
      </div>
    </div>
  {/if}

  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if sources === null}
    <p class="empty">Loading…</p>
  {:else if sources.length === 0}
    <p class="empty">No incoming library is set. Choose one in Settings.</p>
  {:else}
    <input class="filter" type="search" placeholder="Filter" bind:value={filter} aria-label="Filter albums" />
    {#each sources as s (s.id)}
      {#if sources.length > 1}<h2>{s.label}</h2>{/if}
      {#if !s.supported}
        <p class="empty">{s.label} cannot list albums.</p>
      {:else if s.error}
        <p class="error" role="alert">{s.error}</p>
      {:else if s.albums.length === 0}
        <p class="empty">Nothing incoming.</p>
      {:else}
        <p class="count">{s.albums.length} {s.albums.length === 1 ? 'album' : 'albums'}</p>
        <ul>
          {#each s.albums.filter(matches) as a (a.id)}
            <li class:playing={playing?.album_id === a.id}>
              {#if noCover[a.id]}
                <div class="cover none" aria-hidden="true"></div>
              {:else}
                <img
                  class="cover"
                  src={incomingCoverUrl(s.id, a.id)}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  onerror={() => (noCover[a.id] = true)}
                />
              {/if}
              <div class="text">
                <div class="title">{a.title}{#if a.year}<span class="year"> ({a.year})</span>{/if}</div>
                <div class="artist">{a.artist ?? 'Unknown artist'}{#if playing?.album_id === a.id}<span class="now"> · playing</span>{/if}</div>
                <div class="meta">
                  <span>{a.tracks} {a.tracks === 1 ? 'track' : 'tracks'}</span>
                  <span class:weak={!lossless(a) && a.bitrate_kbps !== null && a.bitrate_kbps < 320}>{quality(a)}</span>
                  {#if a.added}<span>added {added(a.added)}</span>{/if}
                  {#each a.tags as t (t)}<span class="tag">{t}</span>{/each}
                </div>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    {/each}
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
    margin: 24px 0 6px;
    font-size: 1rem;
  }

  .lede,
  .empty,
  .count {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .error {
    color: var(--danger);
  }

  .filter {
    width: 100%;
    max-width: 360px;
    min-height: 40px;
    margin: 8px 0 4px;
    padding: 0 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    color: var(--text);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }

  .cover {
    flex: none;
    width: 56px;
    height: 56px;
    border-radius: 8px;
    object-fit: cover;
    background: var(--surface);
  }

  .cover.none {
    border: 1px solid var(--border);
  }

  .text {
    min-width: 0;
  }

  .title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .year,
  .artist {
    color: var(--muted);
    font-weight: 400;
  }

  .artist {
    font-size: 0.9rem;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin-top: 2px;
    color: var(--muted);
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
  }

  .np {
    display: flex;
    gap: 12px;
    align-items: center;
    margin: 12px 0 8px;
    padding: 10px 12px;
    background: var(--surface);
    border: 1px solid var(--accent);
    border-radius: 12px;
  }

  .np .label {
    color: var(--accent);
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  li.playing {
    margin: 0 -8px;
    padding-inline: 8px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-radius: 10px;
  }

  .now {
    color: var(--accent);
    font-weight: 600;
  }

  .weak {
    color: var(--danger);
  }

  .tag {
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
  }
</style>
