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
  import { barTracks, gridBars, placeOf, planStrip, playedRuns, trackBars, type GridStrip, type StripTrack } from './waveStrip';
  import { markShows } from './waveView';

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
    ahead = false,
    landing = null,
    track = null,
    left = null,
    back = false,
    wide = null,
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
    /** Difference (Level the volume › Waveform): the heard part ahead of the playhead in its own tint, between the
     *  paused playing colour and the rest (the owner, 2026-10-28). */
    ahead?: boolean;
    /** A hold's landing if let go now (0..1 of the track): that tint between the playhead and it, and only there. */
    landing?: number | null;
    /** Which track it shows; `left`: how much of the one before was played when it was left (else its last shown). The
     *  pieces still showing the track before, until the next one's front has reached them, keep that colour (the owner,
     *  2026-10-28: Next's wind to the end left its ends looking unplayed). */
    track?: string | null;
    left?: number | null;
    /** Playing backwards (the knob, a hold back): a track change's colours run from the right instead (the owner,
     *  2026-10-28: the colouring goes the way it plays; the pieces' own fronts still from the left). */
    back?: boolean;
    /** The tracks around this one (lib/SeekWave's wide view: a hold's look-ahead in another track), each with its own
     *  bins (lib/waveStrip), the stretch shown (`from`..`to`), this track's place in it (`cur0`..`cur1`) and the
     *  boundaries between them (`marks`), all in s of the hold's frame; each track's colour (`colours`, by its place in
     *  `tracks`). Places given in this track are drawn where it stands. */
    wide?: { tracks: StripTrack[]; from: number; to: number; cur0: number; cur1: number; marks: number[]; colours?: (string | null)[] } | null;
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
  const shownAt = (x: number) => {
    if (wide) return (wide.cur0 + x * (wide.cur1 - wide.cur0) - wide.from) / Math.max(1e-6, wide.to - wide.from);
    return view ? (x - view.from) / Math.max(1e-6, view.to - view.from) : x;
  };
  const done = $derived(Math.min(1, Math.max(0, shownAt(progress))));
  // The track before: how far it was played, and since when the next one's front runs (null: not yet begun).
  let shownTrack: string | null = null;
  let lastDone = 0;
  let oldDone = $state(0);
  let changed = false;
  let oldFrontAt = $state<number | null>(null);
  /** The way the last track change's colours run: from the right where it was playing backwards then. */
  let fromRight = $state(false);
  $effect(() => {
    const k = track;
    const p = done;
    const l = left;
    untrack(() => {
      // Through a wide view (a continuous hold) the tracks run on as one: no track change.
      if (k !== shownTrack && wide) shownTrack = k;
      if (k !== shownTrack) {
        if (shownTrack !== null) {
          fromRight = back;
          oldDone = l ?? lastDone;
          changed = true;
          oldFrontAt = null;
        }
        shownTrack = k;
      }
      lastDone = p;
    });
  });
  /** The track before's colour, as the next one's front reaches each piece, fades into the unplayed tint over this long,
   *  the pieces keeping their size (the owner, 2026-10-28: a colour going, left to right, not a shrinking). */
  const OLD_FADE_MS = 320;
  /** How far the next track's front has come (0..1): the pieces past it still show the track before. */
  const oldFrom = $derived.by(() => {
    void frame;
    if (changed) return 0;
    if (oldFrontAt === null) return 1;
    return Math.min(1, (performance.now() - oldFrontAt) / SPREAD_MS);
  });
  /** The in-between tint's stretch (shown 0..1): to a hold's landing either way, else (Difference) all that is ahead. */
  const band = $derived.by((): [number, number] | null => {
    const clamp = (x: number) => Math.min(1, Math.max(0, x));
    if (landing !== null) {
      const l = clamp(shownAt(landing));
      return l >= done ? [done, l] : [l, done];
    }
    return ahead ? [done, 1] : null;
  });
  /** The plain bar's line (4 px), in the drawing's units. */
  const line = $derived((100 * 4) / Math.max(4, height));
  /** Each bar's rise above the quietest, for the waveform shown: by the same rule as a hold's strip (lib/waveStrip's
   *  trackBars), so a hold ending on the track's own view hands over to it without a step (0.12). */
  const heights = $derived(bins ? trackBars(bins, n).map((v) => Math.max(LOW, v * 100) - LOW) : null);

  // A hold's zoom (`view`) or wide view (`wide`) as strips three times the stretch shown (lib/waveStrip): their bars
  // drawn once, slid and scaled by the compositor as the stretch moves, drawn anew only once the stretch leaves them
  // (the owner, 2026-10-29: a held wind went in strides; the tablet redrew every bar of four layers each frame). The
  // pieces' own motion has no part in it: a hold's bars stand where the stretch puts them (its window's easing is its
  // motion, the playhead its focal point).
  // The same number of bars across it at every moment, each the music's under it (0.12, the owner: "the waveform is no
  // longer pure through zoom!!! Not the same number of sections in the viewport permanently"; 0.11.1's power-of-two
  // levels showed between n and 2n, blended as the zoom went): one strip on one grid (lib/waveStrip's planStrip), each
  // track against its own loudest, the level as heard kept from the hold's start (each track's own came in as the tape
  // reached it and rescaled them all), drawn in place (never a strip swapped for another, nor one shown blank).
  /** The stretch shown, in its frame: a track's share of itself (a zoom), or s of the hold's frame through the queue (a
   *  wide view); the tracks in it, and the playing track's own view there (`home`: its grid the zoom comes back to). */
  const stretch = $derived(
    want !== 'wave'
      ? null
      : wide
        ? { tracks: wide.tracks, from: wide.from, to: wide.to, marks: wide.marks, home: { from: wide.cur0, to: wide.cur1 } }
        : bins && view
          ? { tracks: [{ key: '', start: 0, d: 1, bins }], from: view.from, to: view.to, marks: [] as number[], home: { from: 0, to: 1 } }
          : null,
  );
  /** A strip as drawn (lib/waveStrip's GridStrip): what it was drawn from (`src`), its paths as made and as heard (at
   *  `level`), the boundaries in it, and its heard path track by track (`played`: each its own colour, by the track's
   *  key, none crossing a boundary; `id` the track's own, lib/waveStrip's playedRuns). */
  type Strip = GridStrip & { id: number; src: string; level: number; made: string; heard: string; marks: number[]; played: { key: string; id: string; d: string }[] };
  /** The strip shown (null: none, the plain waveform). */
  let shownStrip = $state<Strip | null>(null);
  /** The level as heard while a hold's strips show: the one it began with (null: none showing). */
  let holdLevel: number | null = null;
  let strips = 0;
  /** A strip's piece, open and risen to its height (the plain draw's at gap 1), at `level`. */
  const piece = (i: number, rise: number, level: number) => {
    const h = LOW + rise * level;
    return `M${f(i + 0.15)} ${f((100 - h) / 2)}h0.7v${f(h)}h-0.7z`;
  };
  const stripPath = (hs: number[], level: number) => hs.map((h, i) => piece(i, h, level)).join('');
  /** The heard pieces track by track: a piece belongs to the track its middle is in; each track's path its own, never
   *  another's once the strip is drawn anew (its fill eases over a ramp: handed on, it showed the track before's colour
   *  on the playing one's bars, 0.12.4). */
  function byTrack(hs: number[], level: number, owners: number[], tracks: readonly StripTrack[]): { key: string; id: string; d: string }[] {
    return playedRuns(owners, tracks).map((r) => {
      let d = '';
      for (let i = r.from; i < r.to; i++) d += piece(i, hs[i], level);
      return { key: tracks[r.owner]?.key ?? '', id: r.id, d };
    });
  }
  /** A track's colour in a wide view (lib/waveView's waveColours): another track's own; none for the track the player
   *  shows and in a zoom on one track, which have the player's, as the waveform has it anyway (0.10.1). */
  const colourOf = (key: string) => {
    const k = wide ? wide.tracks.findIndex((t) => t.key === key) : -1;
    return (k >= 0 ? wide?.colours?.[k] : null) ?? undefined;
  };
  function drawStrip(c: GridStrip & { src: string }, tracks: readonly StripTrack[], marks: number[], level: number): Strip {
    const hs = gridBars(tracks, c).map((v) => Math.max(LOW, v * 100) - LOW);
    return { ...c, id: ++strips, level, made: stripPath(hs, 1), heard: stripPath(hs, level), marks, played: byTrack(hs, level, barTracks(tracks, c), tracks) };
  }
  $effect.pre(() => {
    const s = stretch;
    const pieces = n;
    const level = heard;
    untrack(() => {
      if (!s) {
        // The hold over: the plain waveform takes over where the strip was, at once (its pieces had stood still under
        // the strip since it began, on the track it began on).
        if (shownStrip) settlePlain(holdLevel ?? level);
        holdLevel = null;
        shownStrip = null;
        return;
      }
      holdLevel ??= level;
      const plan = planStrip(shownStrip, s.tracks, s.from, s.to, pieces, s.home);
      if (plan !== shownStrip) shownStrip = drawStrip(plan, s.tracks, s.marks, holdLevel!);
    });
  });
  /** Where a strip stands now: slid and scaled so that the stretch shown fills the waveform. */
  const atOf = (d: GridStrip | null) => {
    if (!d || !stretch) return null;
    const { x, k } = placeOf(d, stretch.from, stretch.to, width);
    return `translateX(${f(x)}px) scaleX(${k.toFixed(6)})`;
  };

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

  /** The plain pieces where the strip left them: the track's own bars at the hold's level, at once; then on to the level
   *  as heard now, as any goal goes (only where the hold ended in a track of another level). */
  function settlePlain(level: number) {
    clearTimeout(pending);
    changed = false;
    oldFrontAt = -Infinity;
    goals = [{ at: -Infinity, w: want, heights, heard: level }];
    scale.fill(level);
    for (let i = 0; i < n; i++) {
      gap[i] = gapWanted(i, want);
      rise[i] = riseWanted(i, goals[0]);
      aim[i] = rise[i];
      span[i] = 0;
    }
    frame++;
    if (Math.abs(level - heard) > 0.01 && !reducedMotion()) {
      goals = [...goals, { at: performance.now(), w: want, heights, heard }];
      run();
    }
  }

  // Every goal runs from left to right on its own (the owner, 2026-10-26: "every new intention should run from left to
  // right purely"): a front that reaches each piece SPREAD_MS / n after the one to its left; a piece goes for the goal
  // of the newest front that has reached it, the pieces to the right of a front still for the one before. A goal starts
  // GOAL_DELAY_MS after it is asked for, one asked for meanwhile in its place (a waveform in within that time: no
  // goal toward the wait first).
  type Goal = { at: number; w: typeof want; heights: number[] | null; heard: number };
  let goals: Goal[] = [];
  const GOAL_DELAY_MS = 100;
  const LEVEL_WAIT_MS = 3000;
  let pending = 0;
  function goalOf(i: number, t: number): Goal {
    for (let k = goals.length - 1; k > 0; k--) if (t >= goals[k].at + (i / n) * SPREAD_MS) return goals[k];
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
    while (k > 0 && t < goals[k].at + SPREAD_MS) k--;
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
    // The track before's colour still shrinking somewhere.
    if (oldFrontAt !== null && t < oldFrontAt + SPREAD_MS + OLD_FADE_MS) moving = true;
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
      // A wide view's bars are its own (the zoom's goal below); another track's coming in is no goal of its own.
      if (wide) return;
      if (reducedMotion()) {
        changed = false;
        oldFrontAt = -Infinity;
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
        // The same goal again (it was asked for and taken back within the delay): no new front (nor a track before).
        if (g && g.w === want && g.heights === heights && Math.abs(g.heard - heard) < 0.01) {
          changed = false;
          oldFrontAt = -Infinity;
          return;
        }
        goals = [...goals, { at: performance.now(), w: want, heights, heard }];
        // The next track's first goal: its front from here.
        if (changed) {
          changed = false;
          oldFrontAt = goals.at(-1)!.at;
        }
        run();
      }, known ? GOAL_DELAY_MS : LEVEL_WAIT_MS);
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
  /** The track before's colour, a mask over its played pieces: gone left of where the next one's front came
   *  OLD_FADE_MS ago, whole right of the front, fading in between (each piece OLD_FADE_MS from its front on). Null: gone. */
  const oldMask = $derived.by(() => {
    void frame;
    // A wide view runs on through the tracks as one: no change of track to colour.
    if (wide || oldFrontAt === -Infinity || oldDone <= 0) return null;
    if (changed || oldFrontAt === null) return 'none';
    const e = performance.now() - oldFrontAt;
    const from = (e - OLD_FADE_MS) / SPREAD_MS;
    if (from >= oldDone) return null;
    return `linear-gradient(to ${fromRight ? 'left' : 'right'}, transparent ${f(from * 100)}%, #000 ${f((e / SPREAD_MS) * 100)}%)`;
  });
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
  {#snippet layers(s: Strip, at: string)}
    <div class="layer"><svg class="strip" viewBox="0 0 {s.count} 100" preserveAspectRatio="none" style:transform={at}><path d={output ? s.heard : s.made} /></svg></div>
    <div class="layer" style:clip-path={`inset(0 ${(1 - done) * 100}% 0 0)`}>
      <svg class="strip played" viewBox="0 0 {s.count} 100" preserveAspectRatio="none" style:transform={at}>
        {#each s.played as p (p.id)}<path d={p.d} style:--seg={colourOf(p.key)} />{/each}
      </svg>
    </div>
    {#if band && band[1] > band[0]}
      <div class="layer" style:clip-path={`inset(0 ${(1 - band[1]) * 100}% 0 ${band[0] * 100}%)`}>
        <svg class="strip ahead" viewBox="0 0 {s.count} 100" preserveAspectRatio="none" style:transform={at}><path d={s.heard} /></svg>
      </div>
    {/if}
    {#if s.marks.length && stretch}
      <!-- Placed across the waveform as it shows now, not in the strip: a boundary at an edge (resting on a wall) shows
           no marker, one coming in fades in (lib/waveView's markShows). -->
      <div class="layer marks">
        <div class="strip-marks">
          {#each s.marks as m (m)}
            {@const x = (m - stretch.from) / Math.max(1e-9, stretch.to - stretch.from)}
            {@const o = markShows(x, n)}
            {#if o > 0}<span class="edge" style:left="{x * 100}%" style:opacity={o < 1 ? o.toFixed(3) : null}></span>{/if}
          {/each}
        </div>
      </div>
    {/if}
  {/snippet}
  {#if shownStrip && stretch}
    <!-- A hold's zoom or wide view: its strip slides and scales under clips that stand still (the played colour to the
         playhead, the landing's tint), its boundaries going with it; drawn anew in place, never swapped. -->
    {@const at = atOf(shownStrip)}
    {#if at}<div class="strips">{@render layers(shownStrip, at)}</div>{/if}
  {:else}
  <!-- The track as made, in the unplayed tint; over it, as far as it has played, the track as heard in the playing
       colour (the owner, 2026-10-27): levelled, what the colour does not reach keeps the unplayed tint, and levelling
       switched off the colour grows into it while those tips shrink to nothing; ahead of the playhead it never moves. -->
  <svg viewBox="0 0 {n} 100" preserveAspectRatio="none"><path d={output ? d : made} /></svg>
  <svg class="played" viewBox="0 0 {n} 100" preserveAspectRatio="none" style:clip-path={wide ? `inset(0 ${(1 - done) * 100}% 0 0)` : fromRight ? `inset(0 ${(1 - done) * 100}% 0 ${Math.min(1, 1 - oldFrom) * 100}%)` : `inset(0 ${(1 - Math.min(done, oldFrom)) * 100}% 0 0)`}>
    <path {d} />
  </svg>
  <!-- The track before, where its pieces still stand: as far as it was played. -->
  {#if oldMask}
    <svg
      class="played"
      viewBox="0 0 {n} 100"
      preserveAspectRatio="none"
      style:clip-path={`inset(0 ${(1 - oldDone) * 100}% 0 0)`}
      style:mask-image={oldMask}
      style:-webkit-mask-image={oldMask}
    >
      <path {d} />
    </svg>
  {/if}
  <!-- What is heard next: ahead of the playhead (Difference), or to where a hold let go now would land (over the played
       colour too, going back). -->
  {#if band && band[1] > band[0]}
    <svg class="ahead" viewBox="0 0 {n} 100" preserveAspectRatio="none" style:clip-path={`inset(0 ${(1 - band[1]) * 100}% 0 ${band[0] * 100}%)`}>
      <path {d} />
    </svg>
  {/if}
  <!-- The boundaries between the tracks in a wide view: a tick below each. -->
  {#if wide}
    {#each wide.marks as m (m)}
      {@const x = (m - wide.from) / Math.max(1e-6, wide.to - wide.from)}
      {@const o = markShows(x, n)}
      {#if o > 0}<span class="edge" style:left="{x * 100}%" style:opacity={o < 1 ? o.toFixed(3) : null}></span>{/if}
    {/each}
  {/if}
  {/if}
  {#if now !== null}<span class="now" style:left="{Math.min(1, Math.max(0, shownAt(now))) * 100}%"></span>{/if}
  <!-- The look-ahead: where a hold let go now lands, the same white line (the owner, 2026-10-28: its tint alone is hard to
       see on a thin waveform). -->
  {#if landing !== null}<span class="now" style:left="{Math.min(1, Math.max(0, shownAt(landing))) * 100}%"></span>{/if}
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

  /* Between the paused playing colour and the rest (the owner, 2026-10-28). */
  svg.ahead {
    fill: color-mix(in srgb, var(--muted) 50%, color-mix(in srgb, var(--text) 22%, transparent));
  }

  svg.played {
    fill: var(--art-accent, var(--accent));
    /* Over a Start and stop ramp (lib/SeekWave's --ramp-ms), in step with it; else at once. */
    transition: fill var(--ramp-ms, 0ms) linear;
  }

  .paused svg.played {
    fill: var(--muted);
  }

  /* A hold's strip: three times the waveform's width, slid and scaled from its left on a layer of its own; each layer as
     wide as the waveform, whatever its clip. */
  .layer {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }

  .strips {
    position: absolute;
    inset: 0;
  }

  svg.strip {
    width: 100%;
    transform-origin: 0 0;
    will-change: transform;
  }

  /* Each track's played pieces in its own colour (--seg, a wide view's); the player's where none is given. */
  svg.strip.played path {
    fill: var(--seg, var(--art-accent, var(--accent)));
    transition: fill var(--ramp-ms, 0ms) linear;
  }

  .paused svg.strip.played path {
    fill: var(--muted);
  }

  /* The boundaries' ticks sit below the waveform: their layer reaches down to them. */
  .layer.marks {
    bottom: -8px;
  }

  .strip-marks {
    position: absolute;
    inset: 0 auto 8px 0;
    width: 100%;
  }

  /* A boundary between two tracks in a wide view (lib/SeekWave): a small tick below the waveform. */
  .edge {
    position: absolute;
    bottom: -6px;
    width: 2px;
    height: 4px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--muted);
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
