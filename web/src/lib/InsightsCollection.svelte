<script lang="ts">
  // Insights' Collection (decision 0065): what you have, the figures as charts by what each says (the dataviz method):
  // the headline figures as stat tiles, shares as meters (never pies), decades as columns, genres and artists as bars,
  // one series each in the accent, labelled directly; the playlists' figures as tiles and the tracks in the most
  // playlists as a list. Five views, chosen in the tabs' pill: Overview (the figures, quality and decades), Genres and
  // Artists (all of them), Tracks (those in playlists), Discovery (what Discover found, the owner 2026-10-25).
  import { matchesSearch } from './pageView.svelte';
  import { getDiscover, getDiscoverExcluded, getDiscoverMissing, type Count, type DiscoverView, type Insights, type Layered } from './api';
  import Icon from './Icon.svelte';
  import PageLoading from './PageLoading.svelte';
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
          // A number only (the owner, 2026-10-24): the Library's own filter could hide them there.
          { value: num(o.incoming_albums), label: `${o.incoming_albums === 1 ? 'album' : 'albums'} in Incoming` },
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
  // Growth's columns (within the period, its steps), against the most added in one; a label where it fits.
  function growthLabel(name: string, i: number) {
    const at = new Date(name.length === 4 ? `${name}-01-01T00:00` : name.length === 7 ? `${name}-01T00:00` : `${name}T00:00`);
    const step = o?.added_step;
    if (step === 'year') return name;
    if (step === 'month') return i % 3 === 0 ? at.toLocaleDateString(undefined, { month: 'short' }) : '';
    if (step === 'week') return at.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
    return at.toLocaleDateString(undefined, { weekday: 'narrow' });
  }
  const mostAdded = $derived(Math.max(1, ...(o?.added ?? []).map((m) => m.n)));

  // Discovery (the owner, 2026-10-25: "how many releases and various other information"): Discover's list as the last
  // look found it, read when the view opens.
  let disc = $state<DiscoverView | null>(null);
  let discError = $state<string | null>(null);
  // Its two lists' sizes (0.9.3's server): a tile each, one level deeper.
  let missingN = $state<number | null>(null);
  let excludedN = $state<number | null>(null);
  $effect(() => {
    if (prefs.insightsCollection !== 'discover') return;
    getDiscoverMissing().then((r) => (missingN = r.albums.length), () => {});
    getDiscoverExcluded().then((r) => (excludedN = r.albums.length + r.artists.length), () => {});
  });
  $effect(() => {
    if (prefs.insightsCollection !== 'discover') return;
    getDiscover().then(
      (v) => (disc = v),
      (e) => (discError = e instanceof Error ? e.message : 'Discover could not be read'),
    );
  });
  /** Each name's count, the most first. */
  const counted = (names: string[]): Count[] => {
    const n = new Map<string, number>();
    for (const x of names) n.set(x, (n.get(x) ?? 0) + 1);
    return [...n.entries()].map(([name, c]) => ({ name, n: c })).sort((a, b) => b.n - a.n || a.name.localeCompare(b.name));
  };
  const found = $derived(disc?.found?.albums ?? []);
  const asked = $derived(Object.values(disc?.requests ?? {}).filter((r) => r.status !== 'cancelled' && r.status !== 'declined'));
  const thisYear = new Date().getFullYear();
  const discTiles = $derived.by(() => {
    const at = disc?.found?.at;
    const hours = at ? Math.floor((Date.now() / 1000 - at) / 3600) : null;
    return [
      { value: num(found.length), label: 'suggestions', href: '#/discover' },
      { value: num(new Set(found.map((a) => a.artist)).size), label: 'artists' },
      { value: num(new Set(found.filter((a) => a.new_artist).map((a) => a.artist)).size), label: 'new to you' },
      { value: num(found.filter((a) => a.bandcamp).length), label: 'play on Bandcamp' },
      { value: num(found.filter((a) => (a.year ?? 0) >= thisYear - 1).length), label: `from ${thisYear - 1} and ${thisYear}` },
      { value: num(asked.length), label: 'asked for', href: '#/requests' },
      // The two lists one level deeper (the owner, 2026-10-25), where the server keeps them.
      ...(missingN !== null ? [{ value: num(missingN), label: 'missing albums', href: '#/insights?view=insights%3Adiscover-missing' }] : []),
      ...(excludedN !== null ? [{ value: num(excludedN), label: 'excluded', href: '#/insights?view=insights%3Adiscover-excluded' }] : []),
      ...(hours !== null ? [{ value: hours < 1 ? 'now' : hours < 48 ? `${hours} h` : plural(Math.floor(hours / 24), 'day'), label: 'since the last look' }] : []),
    ];
  });
  const discSources = $derived(counted(found.flatMap((a) => a.sources ?? [])));
  const discGaps = $derived(
    counted(found.flatMap((a) => a.why.flatMap((w) => /^Missing from (.+)'s albums$/.exec(w)?.[1] ?? []))).slice(0, 12),
  );
  const discSeeds = $derived(
    counted(found.flatMap((a) => a.why.flatMap((w) => /^Like (.+)$/.exec(w)?.[1].split(' and ').slice(0, 1) ?? []).filter((x) => x !== 'what you play'))).slice(0, 12),
  );
  const discGenres = $derived(counted(found.flatMap((a) => a.genres ?? [])).slice(0, 12));
  const discKinds = $derived(counted(found.map((a) => a.kind ?? 'Unknown')));
  const discDecades = $derived.by(() => {
    const d = counted(found.flatMap((a) => (a.year ? [`${Math.floor(a.year / 10) * 10}s`] : [])));
    return d.sort((a, b) => a.name.localeCompare(b.name));
  });
  const discTallest = $derived(widest(discDecades));
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
  {@const rows = (prefs.insightsSort === 'az' ? [...counted].sort((x, y) => x.name.localeCompare(y.name)) : counted).filter((r) => matchesSearch(r.name)).slice(0, SHOWN)}
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

<!-- Plain bars: each name's count (Discovery's sources, artists, genres). -->
{#snippet plainBars(title: string, unit: string, rows: Count[])}
  {@const shown = rows.filter((r) => matchesSearch(r.name))}
  {@const most = widest(shown)}
  {#if shown.length}
    <section>
      <div class="head"><h2>{title}</h2><span class="unit">{unit}</span></div>
      <ul class="bars">
        {#each shown as r (r.name)}
          <li title={`${r.name}: ${num(r.n)}`}>
            <span class="label">{r.name}</span>
            <span class="bar"><i style:width={`${(100 * r.n) / most}%`}></i></span>
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
        {#if t.href}<a class="tile go" href={t.href}><b>{t.value}</b><span>{t.label}</span><i class="go-arrow" aria-hidden="true"><Icon name="arrowRight" size={16} /></i></a>{:else}<div class="tile"><b>{t.value}</b><span>{t.label}</span></div>{/if}
      {/each}
    </div>
  </section>

  {#if o}

    <section>
      <div class="head"><h2>Growth</h2><span class="unit">albums added per {o.added_step}</span></div>
      <div class="columns months" role="list">
        {#each o.added as m, i (m.name)}
          <div class="column" role="listitem" title={`${m.name}: ${plural(m.n, 'album')}`}>
            <span class="v"></span>
            <span class="col"><i style:height={`${(100 * m.n) / mostAdded}%`}></i></span>
            <span class="x">{growthLabel(m.name, i)}</span>
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

  <!-- Listed (the owner, 2026-10-23): how much of it a list holds, a playlist or one of your crates (Unlisted is about
       the rest, so not one of them). -->
  <section>
    <h2>Listed</h2>
    <div class="meters">
      {@render meter('Tracks in a playlist', `${num(p.library_tracks_in_playlists)} of ${plural(l.tracks, 'track')}`, inPlaylists)}
      {@render meter('Tracks in a crate', `${num(p.library_tracks_in_crates ?? 0)} of ${plural(l.tracks, 'track')}`, share(p.library_tracks_in_crates ?? 0, l.tracks))}
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

  {:else if prefs.insightsCollection === 'discover'}
  {#if discError}
    <p class="formats" role="alert">{discError}</p>
  {:else if !disc}
    <PageLoading />
  {:else if !found.length}
    <p class="formats">Discover has found nothing yet: <a href="#/discover">look on its page</a>.</p>
  {:else}
    <section aria-label="Discovery">
      <div class="tiles">
        {#each discTiles as t (t.label)}
          {#if t.href}<a class="tile go" href={t.href}><b>{t.value}</b><span>{t.label}</span><i class="go-arrow" aria-hidden="true"><Icon name="arrowRight" size={16} /></i></a>{:else}<div class="tile"><b>{t.value}</b><span>{t.label}</span></div>{/if}
        {/each}
      </div>
    </section>
    <section>
      <h2>Shares</h2>
      <div class="meters">
        {@render meter('Playable on Bandcamp', `${num(found.filter((a) => a.bandcamp).length)} of ${plural(found.length, 'suggestion')}`, share(found.filter((a) => a.bandcamp).length, found.length))}
        {@render meter('By artists new to you', `${num(found.filter((a) => a.new_artist).length)} of ${plural(found.length, 'suggestion')}`, share(found.filter((a) => a.new_artist).length, found.length))}
        {@render meter('Asked for', `${num(asked.length)} of ${plural(found.length, 'suggestion')}`, share(asked.length, found.length))}
      </div>
      <p class="formats">{discKinds.map((k) => `${num(k.n)} ${k.name}`).join(' · ')}</p>
    </section>
    {#if discDecades.length}
      <section>
        <div class="head"><h2>Decades</h2><span class="unit">suggestions</span></div>
        <div class="columns" role="list">
          {#each discDecades as d (d.name)}
            <div class="column" role="listitem" title={`${d.name}: ${plural(d.n, 'suggestion')}`}>
              <span class="v">{num(d.n)}</span>
              <span class="col"><i style:height={`${(100 * d.n) / discTallest}%`}></i></span>
              <span class="x">{d.name.replace(/^(\d\d)(\d\d)s$/, "'$2s")}</span>
            </div>
          {/each}
        </div>
      </section>
    {/if}
    {@render plainBars('Sources', 'suggestions', discSources)}
    {@render plainBars('Discography gaps', 'albums missing', discGaps)}
    {@render plainBars('Because you play', 'suggestions like them', discSeeds)}
    {@render plainBars('Genres', 'suggestions', discGenres)}
  {/if}
  {:else if prefs.insightsCollection === 'genres'}
  {@render bars('Genres', l.genres)}
  {:else if prefs.insightsCollection === 'artists'}
  {@render bars('Artists', l.top_artists)}
  {:else}
  <!-- Tracks (the owner, 2026-10-23): in playlists or played, one at a time as Genres and Artists count (the panel's
       Count): the tracks most in each; the plays also as columns by how often. Their shares are Overview's (Listed,
       Heard). -->
  {#if prefs.tracksCount === 'listed' || !data.overview}
  <section>
    <h2>In playlists</h2>
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

  /* A tile that leads somewhere: a highlighted arrow in its top right (the owner, 2026-10-25). */
  .go-arrow {
    position: absolute;
    top: 10px;
    right: 10px;
    display: grid;
    color: var(--accent-ink);
  }

  a.tile.go {
    position: relative;
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
