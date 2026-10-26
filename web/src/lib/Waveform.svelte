<script lang="ts">
  // The seekbar's waveform (decision 0035): the track's loudness along it as bars, what has played in the accent
  // (`progress`, 0..1), the rest muted. As many bars as fit at 3 px each. The seek bar around it takes the pointer.
  // `now` (0..1): where it plays while the handle is held elsewhere, a thin line (the owner, 2026-10-24).
  //
  // Every change of it moves (the owner, 2026-10-26: "ultra smooth waveforms throughout any transition"): each piece
  // of it goes toward what it should be, the leftmost first (each SPREAD_MS / n after the one before), and turns around
  // from wherever it is when that changes. A piece is the plain bar's line (its gap closed), slim (its gap open, the
  // quietest bar's height) or a bar (slim and risen to its height); it rises only with its gap open, and closes its gap
  // only once it is down. So:
  //   - `want` 'wait' (its waveform loading): the bars stay; the line gets its gaps as far as it has loaded
  //     (`loading`, 0..1: a progress bar of its own), pieces with their gaps already keep them;
  //   - 'wave': into the bars (`bins`); from another track's straight to this one's, each bar up or down (the owner,
  //     2026-10-26: no need to go back to zero);
  //   - 'line' (none): down, then the gaps close into the line; `onflat` once all of it is the line.
  // One path per layer, redrawn per frame only while something moves (hundreds of animated shapes ran poorly).
  import { untrack } from 'svelte';
  import { reducedMotion } from './motion';
  import { bars } from './waveforms.svelte';

  let {
    bins,
    want,
    progress,
    paused = false,
    now = null,
    loading = null,
    onflat,
  }: {
    bins: Uint8Array | null;
    want: 'wave' | 'wait' | 'line';
    progress: number;
    paused?: boolean;
    now?: number | null;
    loading?: number | null;
    onflat?: () => void;
  } = $props();

  /** One piece's gap opening or closing; the tallest bar's rise or fall; the last piece's start after the first's. */
  const GAP_MS = 160;
  const RISE_MS = 320;
  const SPREAD_MS = 420;
  /** The quietest bar's height, which the pieces slim to. */
  const LOW = 6;

  let width = $state(0);
  let height = $state(28);
  const n = $derived(Math.max(16, Math.min(512, Math.floor(width / 3))));
  const done = $derived(Math.min(1, Math.max(0, progress)));
  /** The plain bar's line (4 px), in the drawing's units. */
  const line = $derived((100 * 4) / Math.max(4, height));
  /** Each bar's rise above the quietest, for the waveform shown. */
  const heights = $derived(bins ? bars(bins, n).map((v) => Math.max(LOW, v * 100) - LOW) : null);

  // Where each piece is (not reactive: drawn on `frame`), and when it may start moving.
  let gap = new Float32Array(0);
  let rise = new Float32Array(0);
  let from = new Float64Array(0);
  let frame = $state(0);

  const gapWanted = (i: number, w: typeof want) =>
    w === 'wave' ? 1 : w === 'line' ? 0 : gap[i] > 0 || i < Math.min(1, Math.max(0, loading ?? 0)) * n ? 1 : 0;
  // Waiting for the next waveform, the bars stay as they are (its heights not known yet; the owner, 2026-10-26: as the
  // cover stays until the next one is in), then go straight to it, or back into the line where there is none.
  const riseWanted = (i: number, w: typeof want) => (w === 'wave' && heights ? (heights[i] ?? 0) : w === 'wait' ? rise[i] : 0);

  let raf = 0;
  let last = 0;
  function run() {
    if (raf) return;
    last = performance.now();
    raf = requestAnimationFrame(step);
  }
  function step(t: number) {
    raf = 0;
    const dt = Math.min(64, t - last);
    last = t;
    const w = want;
    let moving = false;
    for (let i = 0; i < n; i++) {
      if (t < from[i]) {
        moving = true;
        continue;
      }
      const tr = riseWanted(i, w);
      const tg = gapWanted(i, w);
      if (rise[i] > tr) {
        rise[i] = Math.max(tr, rise[i] - (dt * 100) / RISE_MS);
        moving = true;
      } else if (gap[i] !== tg) {
        gap[i] = tg > gap[i] ? Math.min(tg, gap[i] + dt / GAP_MS) : Math.max(tg, gap[i] - dt / GAP_MS);
        moving = true;
      } else if (rise[i] < tr && gap[i] >= 1) {
        rise[i] = Math.min(tr, rise[i] + (dt * 100) / RISE_MS);
        moving = true;
      }
    }
    frame++;
    if (moving) raf = requestAnimationFrame(step);
    else if (w === 'line' && gap.every((g) => g === 0)) onflat?.();
  }
  /** Each piece may start a little after the one to its left. */
  function stagger(t: number) {
    for (let i = 0; i < n; i++) from[i] = t + (i / n) * SPREAD_MS;
  }

  // As many pieces as fit: (again) where they should be, at once.
  $effect(() => {
    const count = n;
    untrack(() => {
      gap = new Float32Array(count);
      rise = new Float32Array(count);
      from = new Float64Array(count);
      for (let i = 0; i < count; i++) {
        gap[i] = gapWanted(i, want);
        rise[i] = riseWanted(i, want);
      }
      frame++;
    });
  });
  // What it should be changed (another track, its waveform in, none): every piece on its way, the leftmost first.
  $effect(() => {
    void want;
    void bins;
    untrack(() => {
      if (reducedMotion()) {
        for (let i = 0; i < n; i++) {
          gap[i] = gapWanted(i, want);
          rise[i] = riseWanted(i, want);
        }
        frame++;
        if (want === 'line') onflat?.();
        return;
      }
      stagger(performance.now());
      run();
    });
  });
  // More of it loaded: the gaps run on with it (no waiting for their turn).
  $effect(() => {
    void loading;
    untrack(run);
  });
  $effect(() => () => cancelAnimationFrame(raf));

  const f = (x: number) => Math.round(x * 100) / 100;
  /** Every piece as one path: in the line (full width, the line's height), slim, or risen. */
  const d = $derived.by(() => {
    void frame;
    let s = '';
    for (let i = 0; i < gap.length; i++) {
      const g = gap[i];
      const h = line + (LOW - line) * g + rise[i];
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
