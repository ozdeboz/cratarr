<script lang="ts">
  // Insights' Listening (decision 0065): the play log (0064) over a week, a month or a year (chosen in the tabs' pill): the time listened as the
  // headline and as columns per day (per week for a year); taste over time as the period's top genres stacked per
  // column, the rest as Other (the dataviz method: at most six series, the categorical palette in its fixed order,
  // checked for both themes; a legend with each genre's time, as light mode's lighter hues need words beside them; a
  // tooltip on every column); and the period's top artists, albums and tracks. The log starts empty (filled from
  // ListenBrainz where that kept the history): what is not there yet is not made up.
  import { albumCoverUrl, coverUrl, getListening, type Listening, type ListeningTop } from './api';
  import Cover from './Cover.svelte';
  import PageLoading from './PageLoading.svelte';
  import { hiddenLibraries } from './listFilter.svelte';
  import { prefs } from './prefs.svelte';

  let data = $state<Listening | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    const p = prefs.insightsPeriod;
    const [source, without] = [prefs.source || undefined, hiddenLibraries()];
    let gone = false;
    data = null;
    error = null;
    getListening(p, source, without).then(
      (d) => !gone && (data = d),
      (e) => !gone && (error = e instanceof Error ? e.message : 'Unexpected error'),
    );
    return () => (gone = true);
  });

  /** "12 h 30 min", "45 min". */
  function duration(secs: number) {
    const m = Math.round(secs / 60);
    const h = Math.floor(m / 60);
    return h ? `${h} h${m % 60 ? ` ${m % 60} min` : ''}` : `${m} min`;
  }
  const day = (start: number, opts: Intl.DateTimeFormatOptions) => new Date(start * 1000).toLocaleDateString(undefined, opts);

  /** A column's label under the axis: a week's weekday letters, a month's every fifth day, a year's month where one
   *  begins. */
  function label(i: number, start: number) {
    if (!data) return '';
    if (data.period === 'week') return day(start, { weekday: 'narrow' });
    if (data.period === 'month') return i % 5 === 0 ? day(start, { day: 'numeric' }) : '';
    const d = new Date(start * 1000);
    const before = new Date((start - 7 * 86400) * 1000);
    // All, over more than a year of weeks: the year where one begins.
    if (data.time.length > 60) return i === 0 || d.getFullYear() !== before.getFullYear() ? day(start, { year: '2-digit' }) : '';
    return i === 0 || d.getMonth() !== before.getMonth() ? day(start, { month: 'narrow' }) : '';
  }
  const when = (start: number) => (data?.period === 'year' || data?.period === 'all' ? `Week of ${day(start, { day: 'numeric', month: 'short' })}` : day(start, { weekday: 'short', day: 'numeric', month: 'short' }));

  const tallest = $derived(Math.max(1, ...(data?.time.map((t) => t.secs) ?? [])));
  // Taste over time: one line per genre (the owner: "the way last.fm does it"), its share of each step's listening.
  const W = 360;
  const H = 140;
  const pct = (s: number) => `${Math.round(100 * s)}%`;
  /** The top of the scale: the highest share, up to the next 10%. */
  const peak = $derived(Math.max(0.1, Math.ceil(10 * Math.max(0, ...(data?.taste.shares.flat() ?? []))) / 10));
  const x = (i: number) => (data && data.taste.starts.length > 1 ? (i * W) / (data.taste.starts.length - 1) : W / 2);
  const y = (s: number) => H - (s / peak) * H;
  const stepName = (st: number) =>
    data?.taste.step === 'year'
      ? day(st, { year: 'numeric' })
      : data?.taste.step === 'month'
        ? day(st, { month: 'long', year: 'numeric' })
        : data?.taste.step === 'day'
          ? day(st, { weekday: 'long', day: 'numeric', month: 'short' })
          : `Week of ${day(st, { day: 'numeric', month: 'short' })}`;
  /** Under the axis: every year, every other month, every week, every day's letter. */
  function stepLabel(i: number, st: number) {
    const step = data?.taste.step;
    if (step === 'year') return day(st, { year: 'numeric' });
    if (step === 'month') return i % 2 === 0 ? day(st, { month: 'short' }) : '';
    if (step === 'day') return day(st, { weekday: 'narrow' });
    return day(st, { day: 'numeric', month: 'short' });
  }
  /** A genre's share over the steps that had listening. */
  function average(j: number) {
    const heardSteps = (data?.taste.shares ?? []).filter((row) => row.some((v) => v > 0));
    return heardSteps.length ? heardSteps.reduce((a, row) => a + (row[j] ?? 0), 0) / heardSteps.length : 0;
  }
  const heard = $derived(!!data && data.total_secs > 0);
  const logged = $derived(data?.log_from ? day(data.log_from, { day: 'numeric', month: 'long', year: 'numeric' }) : null);
</script>

{#snippet top(title: string, list: ListeningTop[], kind: 'artist' | 'album' | 'track')}
  {#if list.length}
    <section>
      <h2>{title}</h2>
      <ol class="top">
        {#each list as t, i (t.id)}
          <li class:plain={kind === 'artist'}>
            <span class="rank">{i + 1}</span>
            {#if kind !== 'artist'}<span class="cover"><Cover src={kind === 'album' ? albumCoverUrl('cratarr', t.id) : coverUrl({ source: 'cratarr', id: t.id })} /></span>{/if}
            <span class="what"><b>{t.name}</b>{#if t.by}<small>{t.by}</small>{/if}</span>
            <span class="n">{t.plays}×</span>
          </li>
        {/each}
      </ol>
    </section>
  {/if}
{/snippet}

<div class="listening">
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !data}
    <PageLoading />
  {:else if !heard}
    <p class="empty">Nothing heard {data.period === 'all' ? 'yet' : `this ${data.period}`}{#if logged}; kept since {logged}{/if}.</p>
  {:else}
    <section aria-label="Listening time">
      <div class="tiles">
        <div class="tile"><b>{duration(data.total_secs)}</b><span>listened</span></div>
        <div class="tile"><b>{data.plays.toLocaleString()}</b><span>{data.plays === 1 ? 'track' : 'tracks'}</span></div>
      </div>
      <div class="columns" role="list" aria-label="Time listened per {data.period === 'year' || data.period === 'all' ? 'week' : 'day'}">
        {#each data.time as t, i (t.start)}
          <div class="column" role="listitem" title={`${when(t.start)}: ${duration(t.secs)}`}>
            <span class="col"><i style:height={`${(100 * t.secs) / tallest}%`}></i></span>
            <span class="x">{label(i, t.start)}</span>
          </div>
        {/each}
      </div>
      {#if logged && data.log_from && data.log_from > data.from}<p class="note">Kept since {logged}.</p>{/if}
    </section>

    {#if data.taste.genres.length}
      {@const t = data.taste}
      <section>
        <h2>Taste over time</h2>
        <div class="lines">
          <span class="y top">{pct(peak)}</span>
          <span class="y mid">{pct(peak / 2)}</span>
          <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label="Each genre's share of your listening per {t.step}">
            <line class="grid" x1="0" x2={W} y1={y(peak / 2)} y2={y(peak / 2)} />
            <line class="grid" x1="0" x2={W} y1={y(peak)} y2={y(peak)} /><line class="grid base" x1="0" x2={W} y1={H} y2={H} />
            {#each t.genres as g, j (g)}
              <polyline points={t.shares.map((row, i) => `${x(i)},${y(row[j] ?? 0)}`).join(' ')} style:stroke={`var(--viz-${j + 1})`} />
            {/each}
            <!-- Hover: each step's shares. -->
            {#each t.starts as st, i (st)}
              <rect x={x(i) - W / t.starts.length / 2} y="0" width={W / t.starts.length} height={H}>
                <title>{[stepName(st), ...t.genres.map((g, j) => `${g}: ${pct(t.shares[i][j] ?? 0)}`)].join('\n')}</title>
              </rect>
            {/each}
          </svg>
          <div class="xs">
            {#each t.starts as st, i (st)}<span>{stepLabel(i, st)}</span>{/each}
          </div>
        </div>
        <ul class="legend">
          {#each t.genres as g, j (g)}
            <li><i style:background={`var(--viz-${j + 1})`}></i>{g}<span class="n">{pct(average(j))}</span></li>
          {/each}
        </ul>
      </section>
    {/if}

    {@render top('Artists', data.artists, 'artist')}
    {@render top('Albums', data.albums, 'album')}
    {@render top('Tracks', data.tracks, 'track')}
  {/if}
</div>

<style>
  /* The categorical slots (the dataviz reference palette, checked against both surfaces), and Other's neutral. */
  .listening {
    --viz-1: #3987e5;
    --viz-2: #d95926;
    --viz-3: #199e70;
    --viz-4: #c98500;
    --viz-5: #d55181;
    display: grid;
    gap: 28px;
  }

  @media (prefers-color-scheme: light) {
    :global(:root:not([data-theme='dark'])) .listening {
      --viz-1: #2a78d6;
      --viz-2: #eb6834;
      --viz-3: #1baf7a;
      --viz-4: #eda100;
      --viz-5: #e87ba4;
    }
  }

  :global(:root[data-theme='light']) .listening {
    --viz-1: #2a78d6;
    --viz-2: #eb6834;
    --viz-3: #1baf7a;
    --viz-4: #eda100;
    --viz-5: #e87ba4;
  }

  h2 {
    margin: 0 0 12px;
    font-size: 1.1rem;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 8px;
    margin-bottom: 16px;
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

  /* Columns from a shared baseline; 2 px apart; a label under some. */
  .columns {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 150px;
    margin-bottom: 22px;
    border-bottom: 1px solid color-mix(in srgb, var(--text) 14%, transparent);
  }

  .column {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: center;
  }

  .col {
    display: flex;
    flex-direction: column-reverse;
    width: 100%;
    max-width: 40px;
    height: 100%;
    justify-content: flex-start;
  }

  .columns .col {
    flex-direction: column;
    justify-content: flex-end;
  }

  .columns .col i {
    display: block;
    width: 100%;
    min-height: 0;
    border-radius: 4px 4px 0 0;
    background: var(--accent);
  }

  /* Taste over time: thin lines from a recessive grid, the scale's top and middle named at the left. */
  .lines {
    position: relative;
    padding-left: 36px;
    margin-bottom: 14px;
  }

  .lines svg {
    display: block;
    width: 100%;
    height: 140px;
    overflow: visible;
  }

  .lines polyline {
    fill: none;
    stroke-width: 2px;
    stroke-linejoin: round;
    stroke-linecap: round;
    vector-effect: non-scaling-stroke;
  }

  .lines .grid {
    stroke: color-mix(in srgb, var(--text) 12%, transparent);
    stroke-width: 1px;
    vector-effect: non-scaling-stroke;
  }

  .lines .base {
    stroke: color-mix(in srgb, var(--text) 24%, transparent);
  }

  .lines rect {
    fill: transparent;
  }

  .lines rect:hover {
    fill: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .y {
    position: absolute;
    left: 0;
    color: var(--muted);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
    transform: translateY(-50%);
  }

  .y.top {
    top: 0;
  }

  .y.mid {
    top: 70px;
  }

  .xs {
    display: flex;
    justify-content: space-between;
    margin-top: 6px;
    color: var(--muted);
    font-size: 0.75rem;
  }

  .xs span {
    flex: 1;
    text-align: center;
    white-space: nowrap;
  }

  .xs span:first-child {
    text-align: left;
  }

  .xs span:last-child {
    text-align: right;
  }

  .x {
    position: absolute;
    bottom: -20px;
    color: var(--muted);
    font-size: 0.75rem;
  }

  .note,
  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .note {
    margin: 4px 0 0;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 0.9rem;
  }

  .legend li {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .legend i {
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }

  .n {
    color: var(--muted);
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
  }

  /* The top lists: rank, cover (albums and tracks), name and by whom, how often. */
  .top {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .top li {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 48px;
  }

  /* An artist has no cover: a shorter row. */
  .top li.plain {
    min-height: 36px;
  }

  .rank {
    width: 1.4rem;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .cover {
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 6px;
    overflow: hidden;
  }

  .what {
    flex: 1;
    min-width: 0;
    display: grid;
  }

  .what b,
  .what small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .what b {
    font-weight: 500;
  }

  .what small {
    color: var(--muted);
    font-size: 0.85rem;
  }

  .error {
    color: var(--danger-ink);
  }
</style>
