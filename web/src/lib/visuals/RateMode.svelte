<script lang="ts">
  // The rating page (decision 0046, amended; from the visuals' Rate, or ?rate=1): two behaviours of the visuals on the
  // music that plays, A for twenty seconds, then B, then the owner's choice (A, B, the same, neither), kept on the server
  // with what each did (its meters, as the loop measured them, and the same in the governor's own terms) for the
  // governor's calibration; then the next pair (rating.ts). The trial is drawn in place of the settings and never kept;
  // stopping hands the visuals back as they were.
  import { onDestroy } from 'svelte';
  import Icon from '../Icon.svelte';
  import { paused, playing, position } from '../remote.svelte';
  import { BUDGETED, budgetOf } from './budget';
  import type { MeterValues } from './meters';
  import { described, pairFor, settingsFor, type Pair } from './rating';
  import { ASPECTS, defaultRoutes } from './routes';
  import { BANDS, DEFAULTS, visualSettings, type VisualSettings } from './settings.svelte';
  import { rating, trial, type TrialLoop } from './trial.svelte';

  /** Each behaviour is shown this long (s) while the music plays, its first `SETTLE_S` not measured (the picture going
   *  over from the one before). */
  const SEGMENT_S = 20;
  const SETTLE_S = 2;
  /** The position moving by more than this (s) beside the time that went by is a seek: the pair starts over. More than
   *  the 1.5 s by which a player's clock sets itself right (remote.svelte), less than the visuals' 10 s taps. */
  const JUMP_S = 5;
  /** A pause longer than this (s) starts the pair over as the music comes back (A is weighed against B from memory); a
   *  shorter one is waited out. */
  const PAUSE_S = 5;
  /** The page not running for longer than this (s) between its quarter seconds (hidden, or the device stalled): what
   *  played was not seen, so the pair starts over. */
  const GAP_S = 1;
  /** The picture's size changing by more than this share (Full screen, turning the device) starts the pair over; less
   *  (a browser's address bar coming and going) does not. */
  const RESIZE = 0.1;
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

  type Side = 'a' | 'b';
  let pair = $state<Pair>(pairFor(kept.n, kept.base));
  let phase = $state<Side | 'ask' | 'saving' | 'stopped'>('a');
  let left = $state(SEGMENT_S);
  let failed = $state(false);
  // Each segment's quarter seconds after its first ones: the meters, the same in the governor's terms (each budgeted
  // meter over its budget at the music's activity then, Wild halfway: the scale the governor's limits are on), and what
  // else the loop knew (the music's activity, the flash rule's hold, the device's pace).
  let samples: Record<Side, MeterValues[]> = { a: [], b: [] };
  let relative: Record<Side, Record<string, number>[]> = { a: [], b: [] };
  let loops: Record<Side, TrialLoop[]> = { a: [], b: [] };
  // When and where each segment was shown (the track, the position it started at), and when the question came.
  let segmentStart = { a: 0, b: 0 };
  let segmentAt: Record<Side, { track: string | null; position: number }> = { a: { track: null, position: 0 }, b: { track: null, position: 0 } };
  let asked = 0;
  // What the pair is compared on, as A started: the look's outside changes until then and the picture's size. The
  // position and the time at the last quarter second, when the music paused, whether the page was away; how often this
  // pair was started over after some of it had been measured.
  let looks: number | null = null;
  let view: [number, number] | null = null;
  let lastPosition = 0;
  let lastTick = 0;
  let pausedAt: number | null = null;
  let away = false;
  let restarts = 0;
  // The trial this page set, so a page gone (the visuals moved, drawn anew with a page of their own) takes back only
  // its own.
  let mine: VisualSettings | null = null;
  const trackNow = () => {
    const np = playing();
    return np ? `${np.source}/${np.id}` : null;
  };

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

  /** Shows `which` from its start (the loop starts its meters and the governor anew for each trial set). */
  function show(which: Side) {
    phase = which;
    left = SEGMENT_S;
    samples[which] = [];
    relative[which] = [];
    loops[which] = [];
    segmentStart[which] = Date.now();
    lastPosition = position();
    lastTick = performance.now();
    segmentAt[which] = { track: trackNow(), position: lastPosition };
    if (which === 'a') {
      looks = trial.loop?.looks ?? null;
      view = trial.loop?.view ?? null;
    }
    trial.settings = settingsFor(pair[which], standard(), ASPECTS);
    mine = trial.settings;
  }

  /** The pair from A again: what it was to be compared on changed under it. */
  function restart() {
    if (phase === 'b' || samples.a.length) restarts++;
    show('a');
  }

  const resized = (now: [number, number]) => !!view && (Math.abs(now[0] / view[0] - 1) > RESIZE || Math.abs(now[1] / view[1] - 1) > RESIZE);

  /** Each of the loop's numbers' mean and least over a segment, and the device it was on. */
  function loopSummary(list: TrialLoop[]): Record<string, unknown> {
    if (!list.length) return {};
    const out: Record<string, unknown> = { device: list[list.length - 1].device };
    for (const key of ['intensity', 'bps', 'hitsPerSecond', 'activity', 'swing', 'points', 'steps', 'fps'] as const) {
      const values = list.map((l) => l[key]);
      out[key] = { mean: values.reduce((s, v) => s + v, 0) / values.length, least: Math.min(...values) };
    }
    return out;
  }

  /** Each value's mean and most over a segment. */
  function summary(list: Record<string, unknown>[]): Record<string, { mean: number; most: number }> {
    const out: Record<string, { mean: number; most: number }> = {};
    if (!list.length) return out;
    for (const key of Object.keys(list[0])) {
      const values = list.map((m) => Number(m[key]));
      out[key] = { mean: values.reduce((s, v) => s + v, 0) / values.length, most: Math.max(...values) };
    }
    return out;
  }

  const side = (which: Side) => ({
    ...described(pair[which], ASPECTS),
    meters: summary(samples[which]),
    relative: summary(relative[which]),
    loop: loopSummary(loops[which]),
    from: segmentStart[which],
    ...segmentAt[which],
  });

  async function choose(choice: 'a' | 'b' | 'same' | 'neither') {
    phase = 'saving';
    const np = playing();
    const body = {
      a: side('a'),
      b: side('b'),
      choice,
      track: np ? `${np.source}/${np.id}` : null,
      context: { version: 4, n: pair.n, kind: pair.kind, swapped: pair.swapped, seconds: SEGMENT_S, settle: SETTLE_S, restarts, series: kept.base, answerMs: Date.now() - asked },
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
    rating.run = 0;
  }

  // Each quarter second of A or B: the pair started over where what it is compared on changed under it (another track,
  // a seek, another look, another size, the page away or the music paused long), the clock held while the music is
  // paused (or nothing plays) and while a look from outside blends in (A shown anew each time, so its meters start
  // after it); else the meters sampled into the segment shown (after its first seconds), the clock run down by the time
  // that went by, A handed on to B, B to the question (B stays drawn while it is asked).
  const timer = setInterval(() => {
    const now = performance.now();
    const dt = (now - lastTick) / 1000;
    lastTick = now;
    if (phase !== 'a' && phase !== 'b') return;
    const at = position();
    const moved = at - lastPosition;
    lastPosition = at;
    if (document.hidden || dt > GAP_S) away = true;
    if (document.hidden) return;
    if (!playing() || paused()) {
      pausedAt ??= now;
      return;
    }
    const pausedLong = pausedAt !== null && (now - pausedAt) / 1000 > PAUSE_S;
    const resumed = pausedAt !== null;
    pausedAt = null;
    const loop = trial.loop;
    const changed =
      away ||
      pausedLong ||
      trackNow() !== segmentAt[phase].track ||
      (!resumed && Math.abs(moved - dt) > JUMP_S) ||
      (!!loop && looks !== null && loop.looks !== looks) ||
      (!!loop && resized(loop.view));
    if (changed || loop?.settling) {
      away = false;
      if (changed) restart();
      else show('a');
      return;
    }
    if (looks === null && loop) {
      looks = loop.looks;
      view = loop.view;
    }
    if (SEGMENT_S - left >= SETTLE_S && trial.meters && loop) {
      samples[phase].push({ ...trial.meters });
      const budget = budgetOf(1, 0.5, loop.activity);
      relative[phase].push(Object.fromEntries(BUDGETED.map((k) => [k, Number(trial.meters![k]) / budget])));
      loops[phase].push({ ...loop });
    }
    left = Math.max(0, left - dt);
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
    if (trial.settings === mine) trial.settings = null;
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
