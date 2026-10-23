<script module lang="ts">
  /** The spread's scale (decision 0041): 5-BPM bands from 60 to 180, always the same, so two spreads compare. */
  export const LO = 60;
  export const HI = 180;
  export const BAND = 5;
  export const BANDS = (HI - LO) / BAND;
  /** Several series (playlists selected together) stack in these; past them the rest is one grey layer. */
  export const PALETTE = ['#12a36a', '#5aa9e6', '#f2b84b', '#e66a8f', '#a98be8'];
  export const REST = 'color-mix(in srgb, var(--text) 30%, transparent)';
  /** A series' colour where several stack: its own up to the palette's last but one, then the rest's grey. */
  export const layerColor = (i: number, n: number) => (n <= PALETTE.length || i < PALETTE.length - 1 ? PALETTE[i] : REST);

  /** The band a tempo falls in: one under 60 counts in the first, one from 180 up in the last. */
  export const bandOf = (bpm: number) => Math.min(BANDS - 1, Math.max(0, Math.floor((bpm - LO) / BAND)));

  /** How many of these tempos fall in each band (tracks without one are not counted). */
  export function bandsOf(bpms: readonly (number | null | undefined)[]): number[] {
    const c: number[] = new Array(BANDS).fill(0);
    for (const b of bpms) if (b) c[bandOf(b)]++;
    return c;
  }

  /** Whether a tempo is in a range of band edges (`[from, to]` BPM); every track, with or without one, when there is
   *  no range. */
  export const within = (bpm: number | null | undefined, range: readonly [number, number] | null) =>
    range === null || (!!bpm && bandOf(bpm) >= (range[0] - LO) / BAND && bandOf(bpm) < (range[1] - LO) / BAND);
</script>

<script lang="ts">
  import SpreadScale from './SpreadScale.svelte';

  // The tempo spread (decision 0041): how many tracks fall in each 5-BPM band, 60 to 180 BPM, as bars growing upward in
  // the waveform's look (lib/Waveform: thin rounded bars, muted, the chosen ones in the accent). A track without a tempo
  // is not a bar. Several series (playlists selected together) stack, each in its colour (`layerColor`); their names are
  // where they already show (the selection's sections carry the colour as a dot, the owner), not in a legend here.
  // `range` lights the bands a selector keeps (lib/TempoRange); without one every bar is muted. `axis`: its scale under
  // it (a selector puts the scale under its handles instead).
  let {
    series,
    range = null,
    height = 56,
    axis = true,
  }: { series: readonly (readonly number[])[]; range?: readonly [number, number] | null; height?: number; axis?: boolean } = $props();

  const stacked = $derived(series.length > 1);
  const totals = $derived(new Array(BANDS).fill(0).map((_, b) => series.reduce((n, s) => n + (s[b] ?? 0), 0)));
  const top = $derived(Math.max(1, ...totals));
  const kept = (i: number) => range !== null && i >= (range[0] - LO) / BAND && i < (range[1] - LO) / BAND;

  // For a screen reader: the bands holding the middle half of the tempos.
  const summary = $derived.by(() => {
    const n = totals.reduce((a, b) => a + b, 0);
    if (!n) return 'No tempos';
    const at = (q: number) => {
      let seen = 0;
      for (let b = 0; b < BANDS; b++) if ((seen += totals[b]) >= q * n) return LO + b * BAND;
      return HI;
    };
    return `Tempo: half the tracks between ${at(0.25)} and ${at(0.75) + BAND} BPM`;
  });
</script>

<div class="spread" role="img" aria-label={summary}>
  <svg viewBox="0 0 {BANDS} 100" preserveAspectRatio="none" style:height="{height}px">
    {#each totals as total, i (i)}
      {#if !total}
        <rect x={i + 0.15} y="98" width="0.7" height="2" rx="0.35" class:out={range !== null && !kept(i)} />
      {:else}
        <!-- Each series' part of the band, stacked from the baseline; the bar keeps its height as one would. -->
        {@const scale = Math.max(5, (total / top) * 100) / total}
        {#each series as s, k (k)}
          {#if s[i]}
            {@const below = series.slice(0, k).reduce((h, x) => h + (x[i] ?? 0), 0) * scale}
            {@const h = s[i] * scale}
            <rect
              x={i + 0.15}
              y={100 - below - h}
              width="0.7"
              height={h}
              rx={below + h >= total * scale - 0.001 ? 0.35 : 0}
              class:kept={kept(i)}
              class:out={range !== null && !kept(i)}
              style:fill={stacked ? layerColor(k, series.length) : undefined}
            />
          {/if}
        {/each}
      {/if}
    {/each}
  </svg>
  {#if axis}<SpreadScale />{/if}
</div>

<style>
  /* Read at a glance: on a wide panel it keeps a chart's width, not the panel's (a selector's handles keep it too). */
  .spread {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-width: 480px;
  }

  svg {
    display: block;
    width: 100%;
    fill: color-mix(in srgb, var(--text) 26%, transparent);
  }

  rect.kept {
    fill: var(--accent);
  }

  rect.out {
    fill: color-mix(in srgb, var(--text) 12%, transparent);
  }
</style>
