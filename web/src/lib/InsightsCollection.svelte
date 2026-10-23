<script lang="ts">
  // Insights' Collection (decision 0065): what you have, the figures as charts by what each says (the dataviz method):
  // the headline figures as stat tiles, shares as meters (never pies), decades as columns, genres and artists as bars,
  // one series each in the accent, labelled directly; the playlists' figures as tiles and the tracks in the most
  // playlists as a list. Four views, chosen in the tabs' pill: Overview (the top few genres and artists), Genres and
  // Artists (all of them), Playlists.
  import type { Count, Insights } from './api';
  import { plural } from './insights.svelte';
  import { prefs } from './prefs.svelte';

  let { data }: { data: Insights } = $props();

  const num = (n: number) => n.toLocaleString();
  const size = (bytes: number) => (bytes >= 1e12 ? `${(bytes / 1e12).toFixed(1)} TB` : `${Math.round(bytes / 1e9)} GB`);
  const share = (part: number, whole: number) => (whole ? part / whole : 0);
  const pct = (s: number) => `${Math.round(100 * s)}%`;
  const widest = (rows: Count[]) => Math.max(1, ...rows.map((r) => r.n));

  const l = $derived(data.library);
  const p = $derived(data.playlists);
  const tiles = $derived([
    { value: num(l.tracks), label: 'tracks' },
    { value: num(l.albums), label: 'albums' },
    { value: num(l.artists), label: 'artists' },
    { value: num(Math.round(l.duration_ms / 3_600_000)), label: 'hours' },
    { value: size(l.bytes), label: 'on disk' },
  ]);
  const formats = $derived([...l.formats].sort((a, b) => b.n - a.n));
  const meters = $derived([
    { label: 'Lossless', of: `${num(l.lossless_albums)} of ${plural(l.albums, 'album')}`, share: share(l.lossless_albums, l.albums) },
    { label: 'Below 320 kbps', of: `${num(l.below_320_albums)} of ${plural(l.albums, 'album')}`, share: share(l.below_320_albums, l.albums) },
  ]);
  const playlistTiles = $derived([
    { value: num(p.playlists), label: 'playlists' },
    { value: num(p.albums_without_playlist), label: 'albums in none' },
    { value: num(p.in_one_playlist), label: 'tracks in only one' },
    ...(p.missing ? [{ value: num(p.missing), label: 'missing tracks' }] : []),
  ]);
  const inPlaylists = $derived(share(p.library_tracks_in_playlists, l.tracks));
  const tallest = $derived(widest(l.decades));
</script>

{#snippet meter(label: string, of: string, s: number)}
  <div class="meter">
    <div class="meter-head"><span>{label}</span><b>{pct(s)}</b></div>
    <div class="track" role="meter" aria-label={label} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(100 * s)}>
      <i style:width={`${100 * s}%`}></i>
    </div>
    <small>{of}</small>
  </div>
{/snippet}

{#snippet bars(title: string, rows: Count[], unit: string)}
  {#if rows.length}
    <section>
      <h2>{title}</h2>
      <ul class="bars">
        {#each rows as r (r.name)}
          <li title={`${r.name}: ${plural(r.n, unit)}`}>
            <span class="label">{r.name}</span>
            <span class="bar"><i style:width={`${(100 * r.n) / widest(rows)}%`}></i></span>
            <span class="n">{num(r.n)}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
{/snippet}

<div class="collection">
  {#if prefs.insightsCollection === 'overview'}
  <section aria-label="Your music">
    <div class="tiles">
      {#each tiles as t (t.label)}
        <div class="tile"><b>{t.value}</b><span>{t.label}</span></div>
      {/each}
    </div>
  </section>

  <section>
    <h2>Quality</h2>
    <div class="meters">
      {#each meters as m (m.label)}{@render meter(m.label, m.of, m.share)}{/each}
    </div>
    {#if formats.length}<p class="formats">{formats.map((f) => `${num(f.n)} ${f.name}`).join(' · ')}</p>{/if}
  </section>

  {#if l.decades.length}
    <section>
      <h2>Decades</h2>
      <div class="columns" role="list">
        {#each l.decades as d (d.name)}
          <div class="column" role="listitem" title={`${d.name}: ${plural(d.n, 'album')}`}>
            <span class="v">{num(d.n)}</span>
            <span class="col"><i style:height={`${(100 * d.n) / tallest}%`}></i></span>
            <span class="x">{d.name.replace(/^(\d\d)(\d\d)s$/, "'$2s")}</span>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {@render bars('Genres', l.genres.slice(0, 8), 'album')}
  {@render bars('Artists', l.top_artists.slice(0, 8), 'album')}
  {:else if prefs.insightsCollection === 'genres'}
  {@render bars('Genres', l.genres, 'album')}
  {:else if prefs.insightsCollection === 'artists'}
  {@render bars('Artists', l.top_artists, 'album')}
  {:else}
  <section>
    <h2>Playlists</h2>
    <div class="tiles">
      {#each playlistTiles as t (t.label)}
        <div class="tile"><b>{t.value}</b><span>{t.label}</span></div>
      {/each}
    </div>
    <div class="meters">{@render meter('Tracks in a playlist', `${num(p.library_tracks_in_playlists)} of ${plural(l.tracks, 'track')}`, inPlaylists)}</div>
    {#if p.most_shared.length}
      <h3>In the most playlists</h3>
      <ol class="shared">
        {#each p.most_shared as t, i (i)}
          <li><span class="t">{t.title}{#if t.artist}<span class="muted"> · {t.artist}</span>{/if}</span><span class="n">in {t.playlists}</span></li>
        {/each}
      </ol>
    {/if}
  </section>
  {/if}
</div>

<style>
  .collection {
    display: grid;
    gap: 28px;
  }

  h2 {
    margin: 0 0 12px;
    font-size: 1.1rem;
  }

  h3 {
    margin: 18px 0 6px;
    color: var(--muted);
    font-size: 0.85rem;
    font-weight: 600;
  }

  /* Stat tiles: the figure large, what it counts under it. */
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: 8px;
  }

  .tile {
    display: grid;
    gap: 2px;
    padding: 12px 14px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .tile b {
    font-size: 1.45rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    line-height: 1.15;
  }

  .tile span {
    color: var(--muted);
    font-size: 0.85rem;
  }

  /* A share as a meter: the track in the same hue, faint; the part filled. */
  .meters {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 14px 20px;
    margin-top: 4px;
  }

  .tiles + .meters {
    margin-top: 14px;
  }

  .meter-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 6px;
  }

  .meter-head b {
    font-variant-numeric: tabular-nums;
  }

  .track {
    height: 8px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    overflow: hidden;
  }

  .track i {
    display: block;
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
  }

  .meter small,
  .formats {
    color: var(--muted);
    font-size: 0.85rem;
  }

  .meter small {
    display: block;
    margin-top: 4px;
  }

  .formats {
    margin: 12px 0 0;
  }

  /* Decades as columns from a shared baseline, their figure above, the decade under. */
  .columns {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    height: 170px;
    padding-bottom: 2px;
    border-bottom: 1px solid color-mix(in srgb, var(--text) 14%, transparent);
  }

  .column {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: grid;
    grid-template-rows: auto 1fr;
    justify-items: center;
    gap: 4px;
    position: relative;
  }

  .column .v {
    color: var(--muted);
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
  }

  .col {
    width: 100%;
    max-width: 34px;
    display: flex;
    align-items: flex-end;
  }

  .col i {
    display: block;
    width: 100%;
    min-height: 2px;
    border-radius: 4px 4px 0 0;
    background: var(--accent);
  }

  .column .x {
    position: absolute;
    bottom: -20px;
    color: var(--muted);
    font-size: 0.75rem;
  }

  .columns + :global(*) {
    margin-top: 24px;
  }

  section:has(.columns) {
    padding-bottom: 20px;
  }

  /* Bars: the name, the bar from a shared start, the figure. */
  .bars {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .bars li {
    display: grid;
    grid-template-columns: minmax(0, 9rem) 1fr auto;
    align-items: center;
    gap: 10px;
  }

  .bars .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bar {
    height: 10px;
  }

  .bar i {
    display: block;
    height: 100%;
    min-width: 2px;
    border-radius: 0 4px 4px 0;
    background: var(--accent);
  }

  .n {
    color: var(--muted);
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .shared {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .shared li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--text) 8%, transparent);
  }

  .shared .t {
    min-width: 0;
  }

  .muted {
    color: var(--muted);
  }
</style>
