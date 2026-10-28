<script lang="ts">
  // The seekbar's waveform (decision 0035): the track's loudness along it as bars, what has played in the accent
  // (`progress`, 0..1), the rest muted. As many bars as fit at 3 px each. The seek bar around it takes the pointer.
  // `now` (0..1): where it plays while the handle is held elsewhere, a thin line (the owner, 2026-10-24).
  //
  // Every change of it moves (the owner, 2026-10-26: "ultra smooth waveforms throughout any transition"): each piece
  // of it goes toward what it should be, each goal a front from the left (each piece SPREAD_MS / n after the one
  // before), and turns around from wherever it is when a newer front reaches it. A piece is the plain bar's line (its gap closed), slim (its gap open, the
  // quietest bar's height) or a bar (slim and risen to its height); it rises only with its gap open, and closes its gap
  // only once it is down. So:
  //   - `want` 'wait' (its waveform loading): the bars shrink to slim pieces (the owner, 2026-10-26: shrinking is the
  //     goal while it loads); the line gets its gaps as far as it has loaded (`loading`, 0..1: a progress bar of its
  //     own), pieces with their gaps already keep them;
  //   - 'wave': into the bars (`bins`); from another track's straight to this one's, each bar up or down (the owner,
  //     2026-10-26: no need to go back to zero);
  //   - 'line' (none): down, then the gaps close into the line; `onflat` once all of it is the line.
  // One path per layer, redrawn per frame only while something moves (hundreds of animated shapes ran poorly).
  import { untrack } from 'svelte';
  import { reducedMotion } from './motion';
  import { bars, barsIn } from './waveforms.svelte';

  let {
    bins,
    want,
    progress,
    paused = false,
    now = null,
    loading = null,
    heard = 1,
    heardKnown = true,
    output = false,
    view = null,
    onflat,
  }: {
    bins: Uint8Array | null;
    want: 'wave' | 'wait' | 'line';
    progress: number;
    paused?: boolean;
    now?: number | null;
    loading?: number | null;
    /** As heard (the owner, 2026-10-26, 0.9.8: levelling you can see): the bars scaled by the level the player applies
     *  (0..1), the original heights a ghost behind them. */
    heard?: number;
    /** As the output (Level the volume › Waveform): the whole lowered to what is heard, not only the playing colour. */
    output?: boolean;
    /** Whether `heard` is the track's own level, not a guess while its gain comes: a goal waits for it (up to
     *  LEVEL_WAIT_MS), so the waveform never shows the guess and then the level (the owner, 2026-10-28). */
    heardKnown?: boolean;
    /** A stretch of the track shown (0..1, `from` to `to`), its bars remapped onto the same pieces: a hold's zoom on the
     *  playhead (lib/SeekWave). Its goal reaches every piece at once (the owner, 2026-10-28: a non-staggered goal). */
    view?: { from: number; to: number } | null;
    onflat?: () => void;
  } = $props();

  /** One piece's gap opening or closing; its rise or fall, however far (the owner, 2026-10-26: the pieces' movement
   *  left to right purely in turn; at one speed a tall bar finished after the short ones to its right); the last
   *  piece's start after the first's. */
  const GAP_MS = 160;
  const RISE_MS = 320;
  const SPREAD_MS = 420;
  /** The quietest bar's height, which the pieces slim to. */
  const LOW = 6;

  let width = $state(0);
  let height = $state(28);
  const n = $derived(Math.max(16, Math.min(512, Math.floor(width / 3))));
  /** Where a place of the track (0..1) is drawn: within the view. */
  const shownAt = (x: number) => (view ? (x - view.from) / Math.max(1e-6, view.to - view.from) : x);
  const done = $derived(Math.min(1, Math.max(0, shownAt(progress))));
  /** The plain bar's line (4 px), in the drawing's units. */
  const line = $derived((100 * 4) / Math.max(4, height));
  /** Each bar's rise above the quietest, for the waveform shown. */
  const heights = $derived(bins ? bars(bins, n).map((v) => Math.max(LOW, v * 100) - LOW) : null);
  /** The view's, zoomed: the same pieces, the stretch remapped onto them. */
  const zoomed = $derived(bins && view && want === 'wave' ? barsIn(bins, n, view.from, view.to, 1 - (view.to - view.from)).map((v) => Math.max(LOW, v * 100) - LOW) : null);

  // Where each piece is (not reactive: drawn on `frame`).
  let gap = new Float32Array(0);
  let rise = new Float32Array(0);
  /** Each piece's rise or fall under way: toward what, and how far it had to go from where it turned. */
  let aim = new Float32Array(0);
  let span = new Float32Array(0);
  /** Each piece's scale as heard (the level), going to its goal's with the front as its rise does (the owner,
   *  2026-10-27: levelling switched is another goal of the waveform's, not a jump of the whole). */
  let scale = new Float32Array(0);
  let frame = $state(0);

  // Every goal runs from left to right on its own (the owner, 2026-10-26: "every new intention should run from left to
  // right purely"): a front that reaches each piece SPREAD_MS / n after the one to its left; a piece goes for the goal
  // of the newest front that has reached it, the pieces to the right of a front still for the one before. A goal starts
  // GOAL_DELAY_MS after it is asked for, one asked for meanwhile in its place (a waveform in within that time: no
  // goal toward the wait first).
  /** `flat`: reaching every piece at once (a zoom), not a front from the left. */
  type Goal = { at: number; w: typeof want; heights: number[] | null; heard: number; flat?: boolean };
  let goals: Goal[] = [];
  const GOAL_DELAY_MS = 100;
  const LEVEL_WAIT_MS = 3000;
  let pending = 0;
  function goalOf(i: number, t: number): Goal {
    for (let k = goals.length - 1; k > 0; k--) if (t >= goals[k].at + (goals[k].flat ? 0 : (i / n) * SPREAD_MS)) return goals[k];
    return goals[0];
  }

  const gapWanted = (i: number, w: typeof want) =>
    w === 'wave' ? 1 : w === 'line' ? 0 : gap[i] > 0 || i < Math.min(1, Math.max(0, loading ?? 0)) * n ? 1 : 0;
  // Waiting for the next waveform, the bars shrink (its heights not known yet; the owner, 2026-10-26), then rise to it,
  // or close back into the line where there is none.
  const riseWanted = (i: number, g: Goal) => (g.w === 'wave' && g.heights ? (g.heights[i] ?? 0) : 0);

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
    // The fronts that have passed every piece: only the newest of them still counts.
    let k = goals.length - 1;
    while (k > 0 && t < goals[k].at + (goals[k].flat ? 0 : SPREAD_MS)) k--;
    if (k > 0) goals = goals.slice(k);
    let moving = goals.length > 1;
    for (let i = 0; i < n; i++) {
      const g = goalOf(i, t);
      // Its level: toward the goal's, as fast as a full rise.
      if (scale[i] !== g.heard) {
        const by = dt / RISE_MS;
        scale[i] = g.heard > scale[i] ? Math.min(g.heard, scale[i] + by) : Math.max(g.heard, scale[i] - by);
        moving = true;
      }
      const tr = riseWanted(i, g);
      const tg = gapWanted(i, g.w);
      // Down first where it must close its gap, else toward its height.
      const goal = rise[i] > tr || gap[i] === tg || gap[i] >= 1 ? tr : 0;
      if (aim[i] !== goal) {
        aim[i] = goal;
        span[i] = Math.abs(goal - rise[i]);
      }
      const by = (Math.max(span[i], 1) * dt) / RISE_MS;
      if (rise[i] > tr) {
        rise[i] = Math.max(tr, rise[i] - by);
        moving = true;
      } else if (gap[i] !== tg) {
        gap[i] = tg > gap[i] ? Math.min(tg, gap[i] + dt / GAP_MS) : Math.max(tg, gap[i] - dt / GAP_MS);
        moving = true;
      } else if (rise[i] < tr && gap[i] >= 1) {
        rise[i] = Math.min(tr, rise[i] + by);
        moving = true;
      }
    }
    frame++;
    if (moving) raf = requestAnimationFrame(step);
    else if (goals[0].w === 'line' && gap.every((g) => g === 0)) onflat?.();
  }

  // As many pieces as fit: (again) where they should be, at once.
  $effect(() => {
    const count = n;
    untrack(() => {
      gap = new Float32Array(count);
      rise = new Float32Array(count);
      aim = new Float32Array(count);
      span = new Float32Array(count);
      scale = new Float32Array(count).fill(heard);
      goals = [{ at: -Infinity, w: want, heights, heard }];
      for (let i = 0; i < count; i++) {
        gap[i] = gapWanted(i, want);
        rise[i] = riseWanted(i, goals[0]);
      }
      frame++;
    });
  });
  // What it should be changed (another track, its waveform in, none): a new front from the left, a moment later.
  $effect(() => {
    void want;
    void heights;
    void heard;
    const known = heardKnown;
    untrack(() => {
      clearTimeout(pending);
      if (reducedMotion()) {
        goals = [{ at: -Infinity, w: want, heights, heard }];
        scale.fill(heard);
        for (let i = 0; i < n; i++) {
          gap[i] = gapWanted(i, want);
          rise[i] = riseWanted(i, goals[0]);
        }
        frame++;
        if (want === 'line') onflat?.();
        return;
      }
      pending = window.setTimeout(() => {
        const g = goals.at(-1);
        // The same goal again (it was asked for and taken back within the delay): no new front.
        if (g && g.w === want && g.heights === heights && Math.abs(g.heard - heard) < 0.01) return;
        goals = [...goals, { at: performance.now(), w: want, heights, heard }];
        run();
      }, known ? GOAL_DELAY_MS : LEVEL_WAIT_MS);
    });
  });
  // Zoomed (a hold): every piece at once toward the stretch shown, the goal kept up to date as it slides and scales; back
  // to the whole at once when it ends.
  $effect(() => {
    const z = zoomed;
    untrack(() => {
      const top = goals.at(-1);
      if (!top) return;
      if (z) {
        if (top.flat && top.heights !== heights) top.heights = z;
        else goals = [...goals, { at: performance.now(), w: 'wave', heights: z, heard: top.heard, flat: true }];
      } else if (top.flat && top.heights !== heights) goals = [...goals, { at: performance.now(), w: want, heights, heard: top.heard, flat: true }];
      else return;
      run();
    });
  });
  // More of it loaded: the gaps run on with it (no waiting for their turn).
  $effect(() => {
    void loading;
    untrack(run);
  });
  $effect(() => () => {
    cancelAnimationFrame(raf);
    clearTimeout(pending);
  });

  const f = (x: number) => Math.round(x * 100) / 100;
  /** Every piece as one path: in the line (full width, the line's height), slim, or risen. */
  /** The pieces drawn with their rise as heard (each piece's level) or as made (`made`: the ghost). */
  const draw = (made: boolean) => {
    let s = '';
    for (let i = 0; i < gap.length; i++) {
      const g = gap[i];
      const h = line + (LOW - line) * g + rise[i] * (made ? 1 : (scale[i] ?? 1));
      const w = 1 - 0.3 * g;
      s += `M${f(i + (1 - w) / 2)} ${f((100 - h) / 2)}h${f(w)}v${f(h)}h${f(-w)}z`;
    }
    return s;
  };
  /** Levelled visibly somewhere: a piece's scale a step from 1 (a fraction of a dB shows nothing). */
  const d = $derived.by(() => {
    void frame;
    return draw(false);
  });
  /** The track as made: the base, in the unplayed tint, always (the owner, 2026-10-27). */
  const made = $derived.by(() => {
    void frame;
    return draw(true);
  });
</script>

<div class="wave" class:paused bind:clientWidth={width} bind:clientHeight={height} aria-hidden="true">
  <!-- The track as made, in the unplayed tint; over it, as far as it has played, the track as heard in the playing
       colour (the owner, 2026-10-27): levelled, what the colour does not reach keeps the unplayed tint, and levelling
       switched off the colour grows into it while those tips shrink to nothing; ahead of the playhead it never moves. -->
  <svg viewBox="0 0 {n} 100" preserveAspectRatio="none"><path d={output ? d : made} /></svg>
  <svg class="played" viewBox="0 0 {n} 100" preserveAspectRatio="none" style:clip-path={`inset(0 ${(1 - done) * 100}% 0 0)`}>
    <path {d} />
  </svg>
  {#if now !== null}<span class="now" style:left="{Math.min(1, Math.max(0, shownAt(now))) * 100}%"></span>{/if}
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
    /* Over a Start and stop ramp (lib/SeekWave's --ramp-ms), in step with it; else at once. */
    transition: fill var(--ramp-ms, 0ms) linear;
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
