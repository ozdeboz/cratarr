<script module lang="ts">
  /** The spread's scale (decision 0041): 5-BPM bands from 60 to 180, always the same, so two spreads compare. */
  export const LO = 60;
  export const HI = 180;
  export const BAND = 5;
  export const BANDS = (HI - LO) / BAND;

  /** The band a tempo falls in: one under 60 counts in the first, one from 180 up in the last. */
  export const bandOf = (bpm: number) => Math.min(BANDS - 1, Math.max(0, Math.floor((bpm - LO) / BAND)));

  /** Whether a tempo is in a range of band edges (`[from, to]` BPM); every track, with or without one, when there is
   *  no range. */
  export const within = (bpm: number | null | undefined, range: readonly [number, number] | null) =>
    range === null || (!!bpm && bandOf(bpm) >= (range[0] - LO) / BAND && bandOf(bpm) < (range[1] - LO) / BAND);
</script>

<script lang="ts">
  import SpreadScale from './SpreadScale.svelte';

  // The tempo spread (decision 0041): how many tracks fall in each 5-BPM band, 60 to 180 BPM, as bars growing upward in
  // the waveform's look (lib/Waveform: thin rounded bars, muted, the chosen ones in the accent). A track without a tempo
  // is not a bar. `range` lights the bands a selector keeps (lib/TempoRange); without one every bar is muted.
  // `axis`: its scale under it (a selector puts the scale under its handles instead).
  let {
    bpms,
    range = null,
    height = 56,
    axis = true,
  }: { bpms: readonly (number | null | undefined)[]; range?: readonly [number, number] | null; height?: number; axis?: boolean } = $props();

  const counts = $derived.by(() => {
    const c: number[] = new Array(BANDS).fill(0);
    for (const b of bpms) if (b) c[bandOf(b)]++;
    return c;
  });
  const top = $derived(Math.max(1, ...counts));
  const kept = (i: number) => range !== null && i >= (range[0] - LO) / BAND && i < (range[1] - LO) / BAND;

  // For a screen reader: where the middle half of the tempos lies.
  const summary = $derived.by(() => {
    const s = bpms.filter((b): b is number => !!b).sort((a, b) => a - b);
    if (!s.length) return 'No tempos';
    const at = (q: number) => Math.round(s[Math.min(s.length - 1, Math.floor(q * s.length))]);
    return `Tempo: half the tracks between ${at(0.25)} and ${at(0.75)} BPM`;
  });
</script>

<div class="spread" role="img" aria-label={summary}>
  <svg viewBox="0 0 {BANDS} 100" preserveAspectRatio="none" style:height="{height}px">
    {#each counts as c, i (i)}
      {@const h = c ? Math.max(5, (c / top) * 100) : 2}
      <rect x={i + 0.15} y={100 - h} width="0.7" height={h} rx="0.35" class:kept={kept(i)} class:out={range !== null && !kept(i)} />
    {/each}
  </svg>
  {#if axis}<SpreadScale />{/if}
</div>

<style>
  .spread {
    display: flex;
    flex-direction: column;
    gap: 4px;
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
