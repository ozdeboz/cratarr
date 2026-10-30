<script lang="ts">
  // Listening's Profile (the owner, 2026-10-23): your taste as Cratarr reads it, from the one similarity's profile
  // What you like (favourites, playlists, plays) and, apart, what you skip or exclude. Words as two lists side by
  // side (more of, netted; less of), numbers as where your taste sits on the library's range (a band from the middle's
  // spread, less of under it), moods by how much more of them than in the library (a bar each way from the library's
  // middle).
  import { ApiError, getTasteProfile, type ProfileMood, type ProfileNumber, type ProfileWord, type TasteProfile } from './api';
  import { hiddenLibraries } from './listFilter.svelte';
  import PageLoading from './PageLoading.svelte';
  import { prefs } from './prefs.svelte';

  let data = $state<TasteProfile | null>(null);
  let error = $state<string | null>(null);
  // Of the libraries switched on and the period (Insights' own, every tab's), from the sources on (its panel).
  $effect(() => {
    const [source, without, period] = [prefs.source || undefined, hiddenLibraries(), prefs.insightsPeriod];
    const off = Object.entries(prefs.profileSources).flatMap(([k, on]) => (on ? [] : [k]));
    let gone = false;
    getTasteProfile(source, without, period, off).then(
      (d) => !gone && ((data = d), (error = null)),
      (e) => !gone && (error = e instanceof ApiError ? e.message : 'Unexpected error'),
    );
    return () => (gone = true);
  });

  const num = (n: number) => n.toLocaleString();
  const pct = (s: number) => `${Math.round(100 * s)}%`;
  const widest = (rows: ProfileWord[]) => Math.max(1e-9, ...rows.map((r) => r.w));
  /** Where a value sits on its number's range (0 to 100). */
  const at = (n: ProfileNumber, v: number) => Math.min(100, Math.max(0, (100 * (v - n.lo)) / (n.hi - n.lo || 1)));
  const shown = (n: ProfileNumber, v: number) => (n.unit === 'share' ? pct(v) : Math.round(v).toString());
  /** A mood's lift as a bar from the middle: how far, either way, of the widest. */
  const widestLift = (moods: ProfileMood[]) => Math.max(0.1, ...moods.map((m) => Math.abs(m.lift)));
  const lifted = (l: number) => `${l >= 0 ? '+' : '−'}${Math.abs(l).toFixed(1)}`;
</script>

{#snippet words(rows: ProfileWord[], less: boolean)}
  <ul class="bars" class:less>
    {#each rows as r (r.name)}
      <li title={r.name}>
        <span class="label">{r.name}</span>
        <span class="bar"><i style:width={`${(100 * r.w) / widest(rows)}%`}></i></span>
      </li>
    {/each}
  </ul>
{/snippet}

<div class="profile">
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if !data}
    <PageLoading />
  {:else}
    {@const t = data.taste}
    {@const a = data.avoid}
    <section aria-label="Built from">
      <div class="tiles">
        <div class="tile"><b>{num(t.sources.favourites ?? 0)}</b><span>favourites</span></div>
        <div class="tile"><b>{num(t.sources.playlists ?? 0)}</b><span>in playlists</span></div>
        <div class="tile"><b>{num(t.sources.plays ?? 0)}</b><span>played</span></div>
        <div class="tile less"><b>{num(a.sources.skipped ?? 0)}</b><span>skipped</span></div>
        <div class="tile less"><b>{num(a.sources.excluded ?? 0)}</b><span>excluded</span></div>
      </div>
      {#if t.tracks}<p class="note">Sound and moods from the {pct(t.heard / t.tracks)} AudioMuse has heard.</p>{/if}
    </section>

    {#each t.words as k (k.kind)}
      {@const avoid = a.words.find((x) => x.kind === k.kind)?.top ?? []}
      <section>
        <h2>{k.title}</h2>
        <div class="sides">
          <div><h3>More of</h3>{@render words(k.top, false)}</div>
          {#if avoid.length}<div><h3>Less of</h3>{@render words(avoid, true)}</div>{/if}
        </div>
      </section>
    {/each}

    <section>
      <h2>Sound</h2>
      <ul class="ranges">
        {#each t.numbers as n (n.name)}
          {@const less = a.numbers.find((x) => x.name === n.name)}
          <li>
            <div class="range-head"><span>{n.name}</span><b>{shown(n, n.mid)}</b></div>
            <div class="range" title={`${n.name}: ${shown(n, n.mid - n.spread)}–${shown(n, n.mid + n.spread)}${less ? `; less of ${shown(n, less.mid - less.spread)}–${shown(n, less.mid + less.spread)}` : ''}`}>
              <span class="line"></span>
              {#if less}<span class="band less" style:left={`${at(n, less.mid - less.spread)}%`} style:right={`${100 - at(n, less.mid + less.spread)}%`}></span>{/if}
              <span class="band" style:left={`${at(n, n.mid - n.spread)}%`} style:right={`${100 - at(n, n.mid + n.spread)}%`}></span>
              <span class="dot" style:left={`${at(n, n.mid)}%`}></span>
            </div>
            <div class="ends"><span>{shown(n, n.lo)}</span><span>{shown(n, n.hi)}</span></div>
          </li>
        {/each}
      </ul>
    </section>

    {#if t.moods.length}
      {@const most = widestLift([...t.moods, ...a.moods])}
      <section>
        <div class="head"><h2>Moods</h2><span class="unit">compared with your library</span></div>
        <ul class="bars moods">
          {#each t.moods as m (m.name)}
            {@const less = a.moods.find((x) => x.name === m.name)}
            <li title={`${m.name}: ${lifted(m.lift)} of the library's spread${less ? `; less of ${lifted(less.lift)}` : ''}`}>
              <span class="label">{m.name}</span>
              <span class="lift">
                <span class="zero"></span>
                <i class:under={m.lift < 0} style:width={`${(50 * Math.abs(m.lift)) / most}%`}></i>
                {#if less}<s style:left={`${50 + (50 * less.lift) / most}%`}></s>{/if}
              </span>
              <span class="n">{lifted(m.lift)}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>

<style>
  .profile {
    display: grid;
    gap: 28px;
  }

  h2 {
    margin: 0 0 12px;
    font-size: 1.1rem;
  }

  h3 {
    margin: 0 0 8px;
    color: var(--muted);
    font-size: 0.85rem;
    font-weight: 600;
  }

  .note,
  .error {
    margin: 10px 0 0;
    color: var(--muted);
    font-size: 0.85rem;
  }

  /* As Collection's stat tiles; what pulls away in muted figures. */
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

  .tile.less b {
    color: var(--muted);
  }

  .tile span {
    color: var(--muted);
    font-size: 0.85rem;
  }

  /* More of and less of side by side where there is room, one under the other on a phone. */
  .sides {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 16px 28px;
  }

  /* Bars as Collection's: the name, the bar from a shared start; less of in the muted ink. */
  .bars {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .bars li {
    display: grid;
    grid-template-columns: minmax(0, 8.5rem) 1fr;
    align-items: center;
    gap: 10px;
  }

  .bars.moods li {
    grid-template-columns: minmax(0, 8.5rem) 1fr 3rem;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bar {
    position: relative;
    height: 10px;
  }

  .bar i {
    display: block;
    height: 100%;
    min-width: 2px;
    border-radius: 0 4px 4px 0;
    background: var(--accent);
  }

  .bars.less .bar i {
    background: color-mix(in srgb, var(--text) 38%, transparent);
  }

  /* A mood: a bar from the library's middle, right for more, left (muted) for less; less of a mark where it sits. */
  .head {
    display: flex;
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

  .lift {
    position: relative;
    height: 10px;
  }

  .lift .zero {
    position: absolute;
    left: 50%;
    top: -3px;
    bottom: -3px;
    width: 1px;
    background: color-mix(in srgb, var(--text) 30%, transparent);
  }

  .lift i {
    position: absolute;
    left: 50%;
    top: 0;
    height: 100%;
    border-radius: 0 4px 4px 0;
    background: var(--accent);
  }

  .lift i.under {
    left: auto;
    right: 50%;
    border-radius: 4px 0 0 4px;
    background: color-mix(in srgb, var(--text) 38%, transparent);
  }

  .lift s {
    position: absolute;
    top: -3px;
    bottom: -3px;
    width: 2px;
    margin-left: -1px;
    background: var(--text);
    opacity: 0.55;
  }

  .n {
    color: var(--muted);
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  /* A number: the library's range as a line, your middle as a dot, its spread as a band; less of a band under it. */
  .ranges {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 18px 28px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .range-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 6px;
  }

  .range-head b {
    font-variant-numeric: tabular-nums;
  }

  .range {
    position: relative;
    height: 22px;
  }

  .range .line {
    position: absolute;
    left: 0;
    right: 0;
    top: 7px;
    height: 2px;
    background: color-mix(in srgb, var(--text) 14%, transparent);
  }

  .band {
    position: absolute;
    top: 3px;
    height: 10px;
    border-radius: 5px;
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }

  .band.less {
    top: 15px;
    height: 6px;
    border-radius: 3px;
    background: color-mix(in srgb, var(--text) 30%, transparent);
  }

  .dot {
    position: absolute;
    top: 0;
    width: 16px;
    height: 16px;
    margin-left: -8px;
    border: 2px solid var(--surface);
    border-radius: 50%;
    background: var(--accent);
  }

  .ends {
    display: flex;
    justify-content: space-between;
    color: var(--muted);
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
  }
</style>
