<script lang="ts">
  // The seekbar's waveform (decision 0035): the track's loudness along it as bars, what has played in the accent
  // (`progress`, 0..1), the rest muted. As many bars as fit at 3 px each. The seek bar around it takes the pointer.
  // `now` (0..1): where it plays while the handle is held elsewhere, a thin line (the owner, 2026-10-24).
  // While it loads (`bins` null; the owner, 2026-10-26: no pop from line to waveform): the plain bar's line, which gets
  // the bars' gaps from the left as far as it has loaded (`loading`, 0..1: a progress bar of its own), each piece where
  // its bar will stand and as wide; once in, the pieces grow up and down into the bars.
  // Each layer is one path (the owner, 2026-10-26: hundreds of shapes, each animated, ran poorly), redrawn per frame
  // only while the bars grow.
  import { untrack } from 'svelte';
  import { reducedMotion } from './motion';
  import { bars } from './waveforms.svelte';

  let {
    bins,
    progress,
    paused = false,
    now = null,
    loading = null,
  }: { bins: Uint8Array | null; progress: number; paused?: boolean; now?: number | null; loading?: number | null } = $props();

  // The flow (the owner, 2026-10-26): a front runs along the line, left to right. Behind it each piece opens its gap and
  // slims to the thinnest bar's height (the line is thicker than the quietest bars), over RAMP pieces; then each bar
  // grows to its height, HOLD_MS after it slimmed and never before its turn (the last SPREAD_MS after the first). While
  // it loads, the front stands as far as it has loaded (a progress bar); once in, it runs on to the end in CUT_MS.
  const CUT_MS = 360;
  const HOLD_MS = 80;
  const GROW_MS = 320;
  const SPREAD_MS = 480;
  /** The quietest bar's height, which the pieces slim to. */
  const LOW = 6;
  const ALL = Math.max(CUT_MS, SPREAD_MS) + HOLD_MS + GROW_MS;
  let width = $state(0);
  let height = $state(28);
  const n = $derived(Math.max(16, Math.min(512, Math.floor(width / 3))));
  const shown = $derived(bins ? bars(bins, n) : null);
  const done = $derived(Math.min(1, Math.max(0, progress)));
  /** The plain bar's line (4 px), in the drawing's units. */
  const line = $derived((100 * 4) / Math.max(4, height));
  /** How many pieces the slimming takes behind the front. */
  const ramp = $derived(Math.max(4, Math.round(n * 0.06)));
  // How long since the waveform came in (ms): all done at once where it was in from the start.
  let grown = $state(untrack(() => (bins ? ALL : 0)));
  /** Where the front stood when it came in (pieces). */
  let frontAt = $state(0);
  $effect(() => {
    if (!bins) return void (grown = 0);
    if (untrack(() => grown) >= ALL) return;
    if (untrack(() => grown) === 0) frontAt = untrack(() => Math.min(1, Math.max(0, loading ?? 0)) * n);
    if (reducedMotion()) return void (grown = ALL);
    const start = performance.now();
    let r = 0;
    const step = (t: number) => {
      grown = Math.min(ALL, t - start);
      if (grown < ALL) r = requestAnimationFrame(step);
    };
    r = requestAnimationFrame(step);
    return () => cancelAnimationFrame(r);
  });
  /** The front (pieces): as far as it has loaded; once in, running on past the end. */
  const front = $derived(
    shown ? (grown >= ALL ? n + ramp : frontAt + (n + ramp - frontAt) * Math.min(1, grown / CUT_MS)) : Math.min(1, Math.max(0, loading ?? 0)) * n,
  );
  /** When the front has piece `i` slimmed, ms after it came in. */
  const slimAt = (i: number) => Math.max(0, ((i + ramp - frontAt) / Math.max(1, n + ramp - frontAt)) * CUT_MS);
  const f = (x: number) => Math.round(x * 100) / 100;
  /** Every piece as one path: in the line (full width, the line's height), slimming behind the front, growing. */
  const d = $derived.by(() => {
    let s = '';
    for (let i = 0; i < n; i++) {
      const g = Math.min(1, Math.max(0, (front - i) / ramp));
      let h = line + (LOW - line) * g;
      if (shown && g >= 1) {
        const own = Math.min(1, Math.max(0, (grown - Math.max(slimAt(i), (i / n) * SPREAD_MS) - HOLD_MS) / GROW_MS));
        h = LOW + (Math.max(LOW, shown[i] * 100) - LOW) * (1 - (1 - own) ** 2);
      }
      const w = 1 - 0.3 * g;
      s += `M${f(i + (1 - w) / 2)} ${f((100 - h) / 2)}h${f(w)}v${f(h)}h${f(-w)}z`;
    }
    return s;
  });
</script>

<div class="wave" class:paused bind:clientWidth={width} bind:clientHeight={height} aria-hidden="true">
  {#each [false, true] as played (played)}
    <svg class:played viewBox="0 0 {n} 100" preserveAspectRatio="none" style:clip-path={played ? `inset(0 ${(1 - done) * 100}% 0 0)` : undefined}>
      <path {d} />
    </svg>
  {/each}
  {#if now !== null}<span class="now" style:left="{Math.min(1, Math.max(0, now)) * 100}%"></span>{/if}
</div>

<style>
  .wave {
    position: relative;
    flex: 1;
    height: 28px;
  }

  svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    fill: color-mix(in srgb, var(--text) 22%, transparent);
  }

  svg.played {
    fill: var(--art-accent, var(--accent));
  }

  .paused svg.played {
    fill: var(--muted);
  }

  .now {
    position: absolute;
    top: -3px;
    bottom: -3px;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--text);
  }
</style>
