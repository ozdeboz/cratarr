<script lang="ts">
  // The rating page (decision 0046, amended; the visuals with ?rate=1): two behaviours of the visuals on the music that
  // plays, A for twenty seconds, then B, then the owner's choice (A, B, the same, neither), kept on the server with what
  // each did (its meters, as the loop measured them) for the governor's calibration; then the next pair (rating.ts).
  // The trial is drawn in place of the settings and never kept; stopping hands the visuals back as they were.
  import { onDestroy } from 'svelte';
  import Icon from '../Icon.svelte';
  import { playing } from '../remote.svelte';
  import type { MeterValues } from './meters';
  import { described, pairFor, settingsFor, type Pair } from './rating';
  import { ASPECTS, defaultRoutes } from './routes';
  import { BANDS, DEFAULTS, visualSettings, type VisualSettings } from './settings.svelte';
  import { trial } from './trial.svelte';

  /** Each behaviour is shown this long (s). */
  const SEGMENT_S = 20;
  const SERIES = 'cratarr-visuals-rating';

  // The series: its base and how far it got, kept on this device so a series is taken up where it was left.
  const kept = (() => {
    try {
      const v = JSON.parse(localStorage.getItem(SERIES) ?? 'null');
      if (v && typeof v.base === 'number' && typeof v.n === 'number') return v as { base: number; n: number };
    } catch {
      /* none kept */
    }
    return { base: Math.floor(Math.random() * 0x100000000), n: 0 };
  })();
  const keep = () => {
    try {
      localStorage.setItem(SERIES, JSON.stringify(kept));
    } catch {
      /* not kept: the next series starts anew */
    }
  };

  let pair = $state<Pair>(pairFor(kept.n, kept.base));
  let phase = $state<'a' | 'b' | 'ask' | 'saving' | 'stopped'>('a');
  let left = $state(SEGMENT_S);
  let failed = $state(false);
  let samples: { a: MeterValues[]; b: MeterValues[] } = { a: [], b: [] };
  let segmentStart = { a: 0, b: 0 };

  /** Production's behaviour, on this device's own look and settings. */
  function standard(): VisualSettings {
    const s = $state.snapshot(visualSettings) as VisualSettings;
    return {
      ...s,
      routes: defaultRoutes(BANDS),
      flash: DEFAULTS.flash,
      speed: DEFAULTS.speed,
      intensity: DEFAULTS.intensity,
      fluid: DEFAULTS.fluid,
      smoothing: DEFAULTS.smoothing,
      floor: DEFAULTS.floor,
      depth: DEFAULTS.depth,
      lighting: [...DEFAULTS.lighting],
      evolution: [...DEFAULTS.evolution],
      continuity: [...DEFAULTS.continuity],
    };
  }

  function show(which: 'a' | 'b') {
    phase = which;
    left = SEGMENT_S;
    samples[which] = [];
    segmentStart[which] = Date.now();
    trial.settings = settingsFor(pair[which], standard(), ASPECTS);
  }

  /** Each meter's mean and most over a segment. */
  function summary(list: MeterValues[]): Record<string, { mean: number; most: number }> {
    const out: Record<string, { mean: number; most: number }> = {};
    if (!list.length) return out;
    for (const key of Object.keys(list[0]) as (keyof MeterValues)[]) {
      const values = list.map((m) => Number(m[key]));
      out[key] = { mean: values.reduce((s, v) => s + v, 0) / values.length, most: Math.max(...values) };
    }
    return out;
  }

  async function choose(choice: 'a' | 'b' | 'same' | 'neither') {
    phase = 'saving';
    const np = playing();
    const body = {
      a: { ...described(pair.a, ASPECTS), meters: summary(samples.a), from: segmentStart.a },
      b: { ...described(pair.b, ASPECTS), meters: summary(samples.b), from: segmentStart.b },
      choice,
      track: np ? `${np.source}/${np.id}` : null,
      context: { version: 1, n: pair.n, kind: pair.kind, swapped: pair.swapped, seconds: SEGMENT_S, series: kept.base },
    };
    try {
      const r = await fetch('/api/visuals/ratings', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
      if (!r.ok) throw new Error(String(r.status));
      failed = false;
    } catch {
      failed = true;
      phase = 'ask';
      return;
    }
    kept.n++;
    keep();
    pair = pairFor(kept.n, kept.base);
    show('a');
  }

  function stop() {
    phase = 'stopped';
    trial.settings = null;
    const url = new URL(location.href);
    url.searchParams.delete('rate');
    history.replaceState(history.state, '', url);
  }

  // Each quarter second: the meters sampled into the segment shown, the clock run down, A handed on to B, B to the
  // question (B stays drawn while it is asked).
  const timer = setInterval(() => {
    if (phase !== 'a' && phase !== 'b') return;
    if (trial.meters) samples[phase].push({ ...trial.meters });
    left = Math.max(0, left - 0.25);
    if (left > 0) return;
    if (phase === 'a') show('b');
    else phase = 'ask';
  }, 250);

  show('a');
  onDestroy(() => {
    clearInterval(timer);
    trial.settings = null;
  });

  const stopTap = (e: Event) => e.stopPropagation();
</script>

{#if phase !== 'stopped'}
  <div class="rate" onpointerdown={stopTap} onpointerup={stopTap} role="group" aria-label="Rating">
    <div class="top">
      <span class="which" aria-live="polite">{phase === 'a' ? 'A' : phase === 'b' ? 'B' : ''}</span>
      {#if phase === 'a' || phase === 'b'}
        <span class="bar"><i style:width="{(1 - left / SEGMENT_S) * 100}%"></i></span>
      {/if}
      <span class="count">{pair.n + 1}</span>
      <button class="stop" aria-label="Stop rating" onclick={stop}><Icon name="close" size={20} /></button>
    </div>
    {#if phase === 'ask' || phase === 'saving'}
      <div class="ask">
        <div class="choices">
          <button disabled={phase === 'saving'} onclick={() => choose('a')}>A</button>
          <button disabled={phase === 'saving'} onclick={() => choose('b')}>B</button>
          <button disabled={phase === 'saving'} onclick={() => choose('same')}>Same</button>
          <button disabled={phase === 'saving'} onclick={() => choose('neither')}>Neither</button>
        </div>
        {#if failed}<p class="failed">Not saved</p>{/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .rate {
    position: absolute;
    inset: 0;
    z-index: 5;
    pointer-events: none;
    color: #fff;
  }
  .top {
    position: absolute;
    top: max(12px, env(safe-area-inset-top));
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 8px 6px 16px;
    border-radius: 999px;
    background: rgb(20 20 24 / 0.8);
    pointer-events: auto;
  }
  .which {
    min-width: 1.2em;
    font-size: 20px;
    font-weight: 600;
    text-align: center;
  }
  .bar {
    width: 120px;
    height: 4px;
    border-radius: 2px;
    background: rgb(255 255 255 / 0.2);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: #3fd18f;
  }
  .count {
    font-size: 13px;
    opacity: 0.7;
    font-variant-numeric: tabular-nums;
  }
  .stop {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: #fff;
    cursor: pointer;
  }
  .ask {
    position: absolute;
    left: 50%;
    bottom: max(24px, env(safe-area-inset-bottom));
    transform: translateX(-50%);
    padding: 12px;
    border-radius: 16px;
    background: rgb(20 20 24 / 0.9);
    pointer-events: auto;
  }
  .choices {
    display: flex;
    gap: 10px;
  }
  .choices button {
    min-width: 72px;
    padding: 12px 16px;
    border: 0;
    border-radius: 12px;
    background: rgb(255 255 255 / 0.12);
    color: #fff;
    font-size: 17px;
    cursor: pointer;
  }
  .choices button:disabled {
    opacity: 0.5;
  }
  .failed {
    margin: 8px 0 0;
    text-align: center;
    font-size: 13px;
    color: #f88;
  }
</style>
