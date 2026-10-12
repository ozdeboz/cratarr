<script lang="ts">
  import { ApiError, getInsights, type Count, type Insights, type Signal } from './lib/api';
  import { prefs } from './lib/prefs.svelte';
  import ProximityGraph from './lib/ProximityGraph.svelte';
  import { key, rankers, weigh } from './lib/proximity';

  let data = $state<Insights | null>(null);
  let error = $state<string | null>(null);
  let selected = $state<number | null>(null);

  $effect(() => {
    getInsights()
      .then((d) => (data = d))
      .catch((e) => (error = e instanceof ApiError ? e.message : 'Unexpected error'));
  });

  const TABS = [
    { id: 'statistics', label: 'Statistics' },
    { id: 'related', label: 'Related' },
  ] as const;
  const SIGNALS: { id: Signal; label: string }[] = [
    { id: 'tracks', label: 'Shared tracks' },
    { id: 'genre', label: 'Genre' },
    { id: 'artists', label: 'Artists' },
  ];
  const anyOn = $derived(SIGNALS.some((s) => prefs.signals[s.id]));

  const num = (n: number) => n.toLocaleString();
  const plural = (n: number, one: string, many = `${one}s`) => `${num(n)} ${n === 1 ? one : many}`;
  const hours = (ms: number) => plural(Math.round(ms / 3_600_000), 'hour');
  const size = (bytes: number) => (bytes >= 1e12 ? `${(bytes / 1e12).toFixed(1)} TB` : `${Math.round(bytes / 1e9)} GB`);
  const pct = (part: number, whole: number) => (whole ? `${Math.round((100 * part) / whole)}%` : '0%');
  const widest = (rows: Count[]) => Math.max(1, ...rows.map((r) => r.n));

  /** The selected playlist's closest playlists under the current signals (scored as the graph does), with what they share. */
  const rank = $derived(data ? rankers(data.proximity) : null);
  const weights = $derived(data && rank ? weigh(data.proximity, rank, prefs.signals) : new Map<string, number>());
  const closest = $derived.by(() => {
    if (!data || selected === null) return [];
    return data.proximity.links
      .filter((l) => l.a === selected || l.b === selected)
      .map((l) => ({ l, other: l.a === selected ? l.b : l.a, w: weights.get(key(l.a, l.b)) ?? 0 }))
      .filter((x) => x.w > 0)
      .sort((a, b) => b.w - a.w)
      .slice(0, 8);
  });
</script>

<main>
  <div class="toolbar">
    <div class="seg" role="group" aria-label="View">
      {#each TABS as t (t.id)}
        <button aria-pressed={prefs.insightsTab === t.id} onclick={() => (prefs.insightsTab = t.id)}>{t.label}</button>
      {/each}
    </div>
    {#if prefs.insightsTab === 'related'}
      <div class="switches" role="group" aria-label="What brings playlists together">
        {#each SIGNALS as s (s.id)}
          <button class="chip" aria-pressed={prefs.signals[s.id]} onclick={() => (prefs.signals[s.id] = !prefs.signals[s.id])}>
            {s.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !data}
    <p class="empty">Loading…</p>
  {:else if prefs.insightsTab === 'statistics'}
    {@const l = data.library}
    {@const p = data.playlists}
    <section>
      <h2>Music</h2>
      <p class="figures">
        <span>{plural(l.tracks, 'track')}</span>
        <span>{plural(l.albums, 'album')}</span>
        <span>{plural(l.artists, 'artist')}</span>
        <span>{hours(l.duration_ms)}</span>
        <span>{size(l.bytes)}</span>
      </p>
      <p class="figures">
        <span>{plural(l.lossless_albums, 'lossless album')}</span>
        <span>{plural(l.below_320_albums, 'album')} below 320 kbps</span>
        {#each l.formats as f (f.name)}<span>{num(f.n)} {f.name}</span>{/each}
      </p>
    </section>

    <section>
      <h2>Playlists</h2>
      <p class="figures">
        <span>{plural(p.playlists, 'playlist')}</span>
        <span>{pct(p.library_tracks_in_playlists, l.tracks)} of your tracks are in a playlist</span>
        <span>{plural(p.albums_without_playlist, 'album')} in no playlist</span>
        <span>{plural(p.in_one_playlist, 'track')} in only one</span>
        {#if p.missing}<span>{plural(p.missing, 'missing track')}</span>{/if}
      </p>
      {#if p.most_shared.length}
        <h3>In the most playlists</h3>
        <ol class="plain">
          {#each p.most_shared as t, i (i)}
            <li><span>{t.title}{#if t.artist}<span class="muted"> · {t.artist}</span>{/if}</span><span class="n">in {t.playlists}</span></li>
          {/each}
        </ol>
      {/if}
    </section>

    {#snippet bars(title: string, rows: Count[], unit: string)}
      {#if rows.length}
        <section>
          <h2>{title}</h2>
          <ul class="bars">
            {#each rows as r (r.name)}
              <li>
                <span class="label">{r.name}</span>
                <span class="bar"><i style:width={`${(100 * r.n) / widest(rows)}%`}></i></span>
                <span class="n">{plural(r.n, unit)}</span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/snippet}
    {@render bars('Genres', l.genres, 'album')}
    {@render bars('Decades', l.decades, 'album')}
    {@render bars('Artists', l.top_artists, 'album')}
  {:else if data.proximity.playlists.length < 2}
    <p class="empty">Related playlists need at least two playlists.</p>
  {:else if !anyOn}
    <p class="empty">Turn on at least one of shared tracks, genre or artists.</p>
  {:else}
    {#key data}
      <ProximityGraph data={data.proximity} signals={prefs.signals} bind:selected />
    {/key}
    {#if selected !== null}
      {@const me = data.proximity.playlists[selected]}
      <section class="detail">
        <h2>{me.name} <span class="n">{plural(me.tracks, 'track')}</span></h2>
        {#if me.genres.length}<p class="muted">{me.genres.join(', ')}</p>{/if}
        {#if closest.length}
          <ul class="plain">
            {#each closest as c (c.other)}
              <li>
                <button class="link" onclick={() => (selected = c.other)}>{data.proximity.playlists[c.other].name}</button>
                <span class="muted">
                  {[
                    c.l.shared_tracks ? plural(c.l.shared_tracks, 'shared track') : null,
                    c.l.genres.length ? c.l.genres.join(', ') : null,
                    c.l.shared_artists ? plural(c.l.shared_artists, 'shared artist') : null,
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted">Not close to another playlist with these switches.</p>
        {/if}
      </section>
    {:else}
      <p class="hint">Tap a playlist to see its closest playlists. Pinch or scroll to zoom.</p>
    {/if}
  {/if}
</main>

<style>
  main {
    max-width: 900px;
    padding: 16px clamp(12px, 2vw, 32px) 48px;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
    margin-bottom: 8px;
  }

  .seg {
    display: flex;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
  }

  button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--surface);
    border: 0;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .seg button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .switches {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    min-height: 36px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--muted);
  }

  .chip[aria-pressed='true'] {
    border-color: var(--accent);
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 18%, var(--surface));
  }

  h2 {
    margin: 24px 0 8px;
    font-size: 1rem;
  }

  h3 {
    margin: 14px 0 6px;
    font-size: 0.9rem;
    color: var(--muted);
  }

  .figures {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    margin: 0 0 6px;
    font-variant-numeric: tabular-nums;
  }

  .muted,
  .hint,
  .empty {
    color: var(--muted);
  }

  .hint,
  .empty {
    font-size: 0.9rem;
  }

  .empty {
    margin-top: 32px;
  }

  .error {
    color: var(--danger);
  }

  .n {
    color: var(--muted);
    font-weight: 400;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .plain {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .plain li {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
    overflow-wrap: anywhere;
  }

  .bars {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 6px;
  }

  .bars li {
    display: grid;
    grid-template-columns: minmax(90px, 30%) 1fr auto;
    align-items: center;
    gap: 10px;
    font-size: 0.9rem;
  }

  .bars .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bar {
    height: 10px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    border-radius: 5px;
    overflow: hidden;
  }

  .bar i {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: 5px;
  }

  .detail h2 {
    margin-top: 16px;
  }

  .detail .plain li {
    justify-content: flex-start;
    flex-wrap: wrap;
  }

  button.link {
    min-height: 32px;
    padding: 0;
    background: none;
    color: var(--accent);
    font-weight: 600;
  }
</style>
