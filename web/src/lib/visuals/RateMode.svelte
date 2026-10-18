<script lang="ts">
  // The rating page (decision 0046, amended; the visuals with ?rate=1): two behaviours of the visuals on the music that
  // plays, A for twenty seconds, then B, then the owner's choice (A, B, the same, neither), kept on the server with what
  // each did (its meters, as the loop measured them) for the governor's calibration; then the next pair (rating.ts).
  // The trial is drawn in place of the settings and never kept; stopping hands the visuals back as they were.
  import { onDestroy } from 'svelte';
  import Icon from '../Icon.svelte';
  import { paused, playing, position } from '../remote.svelte';
  import type { MeterValues } from './meters';
  import type { TrialLoop } from './trial.svelte';
  import { described, pairFor, settingsFor, type Pair } from './rating';
  import { ASPECTS, defaultRoutes } from './routes';
  import { BANDS, DEFAULTS, visualSettings, type VisualSettings } from './settings.svelte';
  import { trial } from './trial.svelte';

  /** Each behaviour is shown this long (s) while the music plays, its first `SETTLE_S` not measured (the picture going
   *  over from the one before). */
  const SEGMENT_S = 20;
  const SETTLE_S = 2;
  /** A jump of the position by more than this (s) beside the time played is a seek: the pair starts over. */
  const JUMP_S = 1.5;
  /** The look's settings that only a track, the dice, a recall or another device in the group change (not the walk of
   *  Drift, nor the Symmetry moments, which are how a behaviour behaves): one changed starts the pair over. */
  const STEADY = ['form', 'final', 'family', 'colours', 'pins'] as const;

  let { onstop }: { onstop?: () => void } = $props();
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
  // What else each segment was shown with (the music's activity, the flash rule's hold, the device's pace), and when the
  // question came (how long the answer took).
  let loops: { a: TrialLoop[]; b: TrialLoop[] } = { a: [], b: [] };
  let segmentStart = { a: 0, b: 0 };
  let asked = 0;
  // Where each segment was shown (the track, the position it started at), what the look was as it started, the position
  // a quarter second ago, and how often this pair started over.
  let segmentAt: { a: { track: string | null; position: number }; b: { track: string | null; position: number } } = {
    a: { track: null, position: 0 },
    b: { track: null, position: 0 },
  };
  let steady = '';
  let lastPosition = 0;
  let restarts = 0;
  const trackNow = () => {
    const np = playing();
    return np ? `${np.source}/${np.id}` : null;
  };
  const steadyNow = () => JSON.stringify(STEADY.map((k) => visualSettings[k]));

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
    loops[which] = [];
    segmentStart[which] = Date.now();
    lastPosition = position();
    segmentAt[which] = { track: trackNow(), position: lastPosition };
    if (which === 'a') steady = steadyNow();
    trial.settings = settingsFor(pair[which], standard(), ASPECTS);
  }

  /** Each of the loop's numbers' mean and least over a segment, and the device it was on. */
  function loopSummary(list: TrialLoop[]): Record<string, unknown> {
    if (!list.length) return {};
    const out: Record<string, unknown> = { device: list[list.length - 1].device };
    for (const key of ['intensity', 'bps', 'hitsPerSecond', 'swing', 'points', 'steps', 'fps'] as const) {
      const values = list.map((l) => l[key]);
      out[key] = { mean: values.reduce((s, v) => s + v, 0) / values.length, least: Math.min(...values) };
    }
    return out;
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
      a: { ...described(pair.a, ASPECTS), meters: summary(samples.a), loop: loopSummary(loops.a), from: segmentStart.a, ...segmentAt.a },
      b: { ...described(pair.b, ASPECTS), meters: summary(samples.b), loop: loopSummary(loops.b), from: segmentStart.b, ...segmentAt.b },
      choice,
      track: np ? `${np.source}/${np.id}` : null,
      context: { version: 3, n: pair.n, kind: pair.kind, swapped: pair.swapped, seconds: SEGMENT_S, settle: SETTLE_S, restarts, series: kept.base, answerMs: Date.now() - asked },
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
    restarts = 0;
    show('a');
  }

  function stop() {
    phase = 'stopped';
    trial.settings = null;
    const url = new URL(location.href);
    url.searchParams.delete('rate');
    history.replaceState(history.state, '', url);
    onstop?.();
  }

  // Each quarter second while the music plays (paused, the clock waits): the pair started over where what it is
  // compared on changed under it (another track, a seek, another look), else the meters sampled into the segment shown
  // (after its first seconds), the clock run down, A handed on to B, B to the question (B stays drawn while it is asked).
  const timer = setInterval(() => {
    if (phase !== 'a' && phase !== 'b') return;
    const at = position();
    if (!playing() || paused()) {
      lastPosition = at;
      return;
    }
    const jumped = Math.abs(at - lastPosition - 0.25) > JUMP_S;
    lastPosition = at;
    if (trackNow() !== segmentAt[phase].track || jumped || steadyNow() !== steady) {
      restarts++;
      show('a');
      return;
    }
    if (SEGMENT_S - left >= SETTLE_S) {
      if (trial.meters) samples[phase].push({ ...trial.meters });
      if (trial.loop) loops[phase].push({ ...trial.loop });
    }
    left = Math.max(0, left - 0.25);
    if (left > 0) return;
    if (phase === 'a') show('b');
    else {
      phase = 'ask';
      asked = Date.now();
    }
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
