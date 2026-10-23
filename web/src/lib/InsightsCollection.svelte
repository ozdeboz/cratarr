<script lang="ts">
  // Insights' Collection (decision 0065): what you have, the figures as charts by what each says (the dataviz method):
  // the headline figures as stat tiles, shares as meters (never pies), decades as columns, genres and artists as bars,
  // one series each in the accent, labelled directly; the playlists' figures as tiles and the tracks in the most
  // playlists as a list. Four views, chosen in the tabs' pill: Overview (the figures, quality and decades), Genres and
  // Artists (all of them), Tracks (those in playlists).
  import type { Count, Insights, Layered } from './api';
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
  // The overview beyond the files (the owner's exploration, 2026-10-23): Incoming, growth, what was heard.
  const o = $derived(data.overview);
  const waited = $derived(o?.incoming_since ? Math.max(0, Math.floor((Date.now() / 1000 - o.incoming_since) / 86_400)) : null);
  // One grid of tiles, no half-empty rows (the owner, 2026-10-23): the collection's, then what waits in Incoming.
  const tiles = $derived<{ value: string; label: string; href?: string }[]>([
    { value: num(l.tracks), label: 'tracks' },
    { value: num(l.albums), label: 'albums' },
    { value: num(l.artists), label: 'artists' },
    { value: num(p.playlists), label: 'playlists' },
    { value: num(Math.round(l.duration_ms / 3_600_000)), label: 'hours' },
    { value: size(l.bytes), label: 'on disk' },
    // From Tracks' old tiles, what is worth a look (the owner, 2026-10-23): tracks a playlist names but no file has.
    ...(p.missing ? [{ value: num(p.missing), label: 'missing from playlists' }] : []),
    ...(o?.incoming_albums
      ? [
          { value: num(o.incoming_albums), label: `${o.incoming_albums === 1 ? 'album' : 'albums'} in Incoming ›`, href: '#/library' },
          ...(waited !== null ? [{ value: plural(waited, 'day'), label: 'the oldest has waited' }] : []),
        ]
      : []),
  ]);
  const formats = $derived([...l.formats].sort((a, b) => b.n - a.n));
  const meters = $derived([
    { label: 'Lossless', of: `${num(l.lossless_albums)} of ${plural(l.albums, 'album')}`, share: share(l.lossless_albums, l.albums) },
    { label: 'Below 320 kbps', of: `${num(l.below_320_albums)} of ${plural(l.albums, 'album')}`, share: share(l.below_320_albums, l.albums) },
  ]);
  /** At most this many bars on Genres and Artists. */
  const SHOWN = 200;
  const mostPlays = $derived(Math.max(1, ...(data.overview?.plays ?? []).map((c) => c.n)));
  const inPlaylists = $derived(share(p.library_tracks_in_playlists, l.tracks));
  // Decades, each a button that opens its ten years (the owner, 2026-10-23: "zoom in on a decade"); the title then a
  // breadcrumb back. Not remembered: the page opens on the decades.
  let decade = $state<string | null>(null);
  const columns = $derived.by((): Count[] => {
    if (!decade) return l.decades;
    const from = Number(decade.slice(0, 4));
    return Array.from({ length: 10 }, (_, i) => ({ name: String(from + i), n: l.years.find((y) => y.name === String(from + i))?.n ?? 0 }));
  });
  const tallest = $derived(widest(columns));
  // Growth's columns, against the most added in a month.
  const mostAdded = $derived(Math.max(1, ...(o?.added ?? []).map((m) => m.n)));
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

<!-- A genre's or an artist's tracks, or those of them in a playlist (the panel's Count; the owner, 2026-10-23: two
     scales, two rankings, so one at a time), the unit said over the bars. -->
{#snippet bars(title: string, all: Layered[])}
  {@const listed = prefs.insightsCount === 'listed'}
  <!-- Every one comes (the server ranks none away): each order is the top of the whole set; the first SHOWN of it. -->
  {@const counted = listed ? all.filter((r) => r.listed > 0).sort((x, y) => y.listed - x.listed || x.name.localeCompare(y.name)) : all}
  {@const rows = (prefs.insightsSort === 'az' ? [...counted].sort((x, y) => x.name.localeCompare(y.name)) : counted).slice(0, SHOWN)}
  {@const value = (r: Layered) => (listed ? r.listed : r.n)}
  {@const most = Math.max(1, ...rows.map(value))}
  {#if rows.length}
    <section>
      <div class="head">
        <h2>{title}</h2>
        <span class="unit">{listed ? 'tracks in a playlist' : 'tracks'}</span>
      </div>
      <ul class="bars">
        {#each rows as r (r.name)}
          <li title={`${r.name}: ${listed ? `${num(r.listed)} of ${plural(r.n, 'track')} in a playlist` : plural(r.n, 'track')}`}>
            <span class="label">{r.name}</span>
            <span class="bar"><i style:width={`${(100 * value(r)) / most}%`}></i></span>
            <span class="n">{num(value(r))}</span>
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
        {#if t.href}<a class="tile go" href={t.href}><b>{t.value}</b><span>{t.label}</span></a>{:else}<div class="tile"><b>{t.value}</b><span>{t.label}</span></div>{/if}
      {/each}
    </div>
  </section>

  {#if o}

    <section>
      <div class="head"><h2>Growth</h2><span class="unit">albums added per month</span></div>
      <div class="columns months" role="list">
        {#each o.added as m, i (m.name)}
          <div class="column" role="listitem" title={`${m.name}: ${plural(m.n, 'album')}`}>
            <span class="v"></span>
            <span class="col"><i style:height={`${(100 * m.n) / mostAdded}%`}></i></span>
            <span class="x">{i % 6 === 0 ? new Date(`${m.name}-01`).toLocaleDateString(undefined, { month: 'short', year: '2-digit' }) : ''}</span>
          </div>
        {/each}
      </div>
    </section>

    <section>
      <h2>Heard</h2>
      <div class="meters">
        {@render meter('Heard at least once', `${num(o.tracks_heard)} of ${plural(o.tracks, 'track')}`, share(o.tracks_heard, o.tracks))}
        {@render meter('Albums never played', `${num(o.albums_never)} of ${plural(o.albums, 'album')}`, share(o.albums_never, o.albums))}
      </div>
      <p class="formats">{plural(o.favourites, 'favourite')}</p>
    </section>
  {/if}

  <section>
    <h2>Quality</h2>
    <div class="meters">
      {#each meters as m (m.label)}{@render meter(m.label, m.of, m.share)}{/each}
    </div>
    {#if formats.length}<p class="formats">{formats.map((f) => `${num(f.n)} ${f.name}`).join(' · ')}</p>{/if}
  </section>

  {#if l.decades.length}
    <section>
      <h2>
        {#if decade}
          <nav class="crumbs" aria-label="Decades"><button onclick={() => (decade = null)}>Decades</button><span aria-hidden="true">›</span><span aria-current="page">{decade}</span></nav>
        {:else}Decades{/if}
      </h2>
      <div class="columns" role="list">
        {#each columns as d (d.name)}
          <div class="column" role="listitem" title={`${d.name}: ${plural(d.n, 'album')}`}>
            <span class="v">{d.n ? num(d.n) : ''}</span>
            {#if decade}
              <span class="col"><i style:height={`${(100 * d.n) / tallest}%`}></i></span>
            {:else}
              <button class="col" aria-label={`${d.name}: ${plural(d.n, 'album')}`} onclick={() => (decade = d.name)}><i style:height={`${(100 * d.n) / tallest}%`}></i></button>
            {/if}
            <span class="x">{decade ? `'${d.name.slice(2)}` : d.name.replace(/^(\d\d)(\d\d)s$/, "'$2s")}</span>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {:else if prefs.insightsCollection === 'genres'}
  {@render bars('Genres', l.genres)}
  {:else if prefs.insightsCollection === 'artists'}
  {@render bars('Artists', l.top_artists)}
  {:else}
  <!-- Tracks (the owner, 2026-10-23): in playlists or played, one at a time as Genres and Artists count (the panel's
       Count): a meter, then the tracks most in each (the panel's sort); the plays also as columns by how often. -->
  {#if prefs.tracksCount === 'listed' || !data.overview}
  <section>
    <h2>In playlists</h2>
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
  {:else}
    {@const o = data.overview}
    <section>
      <h2>Plays</h2>
      <div class="meters">{@render meter('Played at least once', `${num(o.tracks_heard)} of ${plural(o.tracks, 'track')}`, share(o.tracks_heard, o.tracks))}</div>
      <h3>Tracks by plays</h3>
      <div class="columns" role="list">
        {#each o.plays as c (c.name)}
          <div class="column" role="listitem" title={`${plural(c.n, 'track')} played ${c.name === '1' ? 'once' : `${c.name} times`}`}>
            <span class="v">{num(c.n)}</span>
            <span class="col"><i style:height={`${(100 * c.n) / mostPlays}%`}></i></span>
            <span class="x">{c.name}×</span>
          </div>
        {/each}
      </div>
      {#if o.most_played.length}
        <h3>Most played</h3>
        <ol class="shared">
          {#each o.most_played as t, i (i)}
            <li><span class="t">{t.title}{#if t.artist}<span class="muted"> · {t.artist}</span>{/if}</span><span class="n">{t.plays}×</span></li>
          {/each}
        </ol>
      {/if}
    </section>
  {/if}
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

  /* A decade's column opens its years: the whole column's height is the target, not only its bar. */
  button.col {
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }

  button.col:hover i,
  button.col:focus-visible i {
    filter: brightness(1.15);
  }

  .crumbs {
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
  }

  .crumbs button {
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }

  .crumbs button:hover {
    color: var(--text);
  }

  .crumbs > span[aria-hidden] {
    color: var(--muted);
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

  /* Months: thin columns close together, a label every six. */
  .columns.months {
    gap: 3px;
    height: 120px;
  }

  .columns.months .col i {
    border-radius: 2px 2px 0 0;
  }

  .columns.months .x {
    white-space: nowrap;
  }

  a.tile.go {
    color: inherit;
    text-decoration: none;
  }

  a.tile.go:hover {
    background: color-mix(in srgb, var(--text) 10%, transparent);
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

  .head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: baseline;
    gap: 4px 16px;
    margin-bottom: 12px;
  }

  .head h2 {
    margin: 0;
  }

  .unit {
    color: var(--muted);
    font-size: 0.85rem;
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
