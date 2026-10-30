<script lang="ts">
  // The track's sound from low to high as it flows past, drawn straight on the player's glass (the owner, 2026-10-24):
  // no box, the ends tapering or fading, its light added to what is behind it (plus-lighter). From the track's spectrum
  // (lib/bandWaves: 32 measured bands, 20 frames a second), drawn in `count` bands (3 to 32; neighbours merged by their
  // mean power), no live audio needed, so it moves on every device. The settings, for every look (Settings › Display ›
  // Player visualisation, or a tap on them: lib/BandSheet):
  // - Window: the waves' width is that many seconds of music, now in the middle, sliding as it plays (time-locked to the
  //   playback), so a wider window shows more of each band's cycles; the meter takes in that many seconds up to now.
  // - Speed: how fast the waves travel and ripple besides; how fast the meter's bars and peaks fall back.
  // - Cycles: times each band's cycles a second. Height: times how tall loudness draws. Bands: how many.
  // - Gap: the bands drawn apart, low at the foot and high at the top (the meter: its bars apart, sideways).
  // - Direction: which way the music flows (the meter: which side the bass is on). Ends: taper to nothing, or fade.
  // The looks:
  // - Aurora: each band a curtain of light rising from the foot, a fine bright edge on top.
  // - Sirius: Siri's ribbons, a band each (two each up to six bands), mirrored about the middle.
  // - Hairlines: each band a fine glowing line through the middle (a fainter one beside it up to six bands).
  // - Meter: a bar of small segments for each band, its peak falling back over it (Winamp's, the owner).
  // Low to high runs coral, mint, sky: they differ in lightness too, not in hue alone. A track's sound comes in softly
  // over a second when it arrives (silence is drawn until then). Still when paused or when the device asks for reduced
  // motion.
  import { onDestroy } from 'svelte';
  import type { BandLook, Spectrum } from './bandWaves.svelte';
  import { reducedMotion } from './motion';

  let {
    spectrum,
    clock,
    duration,
    paused,
    look,
    speed = 1.6,
    window = 3,
    cycles = 1,
    height = 1,
    count = 3,
    gap = 0,
    direction = 'left',
    engine = 'canvas',
    tapering = 1,
    fading = 0,
  }: {
    spectrum: Spectrum;
    /** Where the music is now (s), to the frame, at `at` (performance.now()): lib/trackPlace placeNow. */
    clock: (at?: number) => number;
    duration: number;
    paused: boolean;
    look: BandLook;
    speed?: number;
    window?: number;
    cycles?: number;
    height?: number;
    /** How many bands it is drawn in. */
    count?: number;
    /** How far apart the bands are drawn, 0..1. */
    gap?: number;
    /** Which way the music flows: to the left, what comes entering on the right; or the other way. */
    direction?: 'left' | 'right';
    /** How it is drawn: SVG, every effect as designed, or a canvas, one bitmap a frame (lib/prefs). */
    engine?: 'svg' | 'canvas';
    /** How much it tapers towards the ends (0..1): Siri's attenuation, wholly at 1. */
    tapering?: number;
    /** How much the ends fade (0..1): the share of the room they fade over, an ease in and out. */
    fading?: number;
  } = $props();

  const H = 132;
  const uid = $props.id();
  /** How far the drawing may reach past its box, above and below, without the player's layout moving (the owner,
   *  2026-10-24: swung or spread lines were cut off at the box's foot). */
  const OVER = 40;
  const FLOOR_DB = 60;

  const n = $derived(Math.max(1, Math.round(count)));
  /** A band's place from low (0) to high (1). */
  const place = (j: number) => (n > 1 ? j / (n - 1) : 0.5);
  /** Low to high: coral, mint, sky. */
  const STOPS: [number, number, number][] = [
    [0xff, 0x7d, 0x6e],
    [0x6f, 0xf0, 0xb4],
    [0x86, 0xc6, 0xff],
  ];
  /** A band's colour at `f` (0 low, 1 high), `white` of the way to white, as `rgba(…)` at `alpha`. */
  function hue(f: number, white = 0, alpha = 1): string {
    const x = f * (STOPS.length - 1);
    const i = Math.min(STOPS.length - 2, Math.floor(x));
    const c = STOPS[i].map((v, k) => v + (STOPS[i + 1][k] - v) * (x - i)).map((v) => Math.round(v + (255 - v) * white));
    return `rgba(${c[0]},${c[1]},${c[2]},${alpha})`;
  }
  const colors = $derived(Array.from({ length: n }, (_, j) => hue(place(j))));
  const edges = $derived(Array.from({ length: n }, (_, j) => hue(place(j), 0.45)));
  /** Each band's cycles a second of music at Cycles 1, low slow to high quick, and its own pace. */
  const cyclesOf = (j: number) => 0.35 * 4 ** place(j);
  const paceOf = (j: number) => 1.1 * (3.6 / 1.1) ** place(j);

  // The spectrum in `n` bands, a frame after another, two ways: the meter's decibels over the floor (0..1, a meter's
  // scale), and the waves' amplitude against the loudest (0..1, as the first three bands were: decibels crowd near the
  // top, so the waves stood at their tallest and Height had nothing left to do), smoothed over a quarter second
  // (frame-to-frame jitter would show as kinks).
  const grouped = $derived.by(() => {
    const { bands, frames, data } = spectrum;
    const db = new Float32Array(frames * n);
    const amp = new Float32Array(frames * n);
    for (let j = 0; j < n; j++) {
      const from = Math.floor((j * bands) / n);
      const to = Math.max(from + 1, Math.floor(((j + 1) * bands) / n));
      for (let f = 0; f < frames; f++) {
        let power = 0;
        for (let b = from; b < to; b++) power += 10 ** ((data[f * bands + b] / 255 - 1) * (FLOOR_DB / 10));
        power /= to - from;
        db[f * n + j] = Math.max(0, Math.min(1, 1 + (10 * Math.log10(power)) / FLOOR_DB));
        amp[f * n + j] = Math.sqrt(power);
      }
    }
    return { db, amp };
  });
  /** Each band's amplitude summed up to each frame (frames + 1 rows): any stretch's mean in two lookups (`loud`). */
  const sums = $derived.by(() => {
    const frames = spectrum.frames;
    const amp = grouped.amp;
    const out = new Float64Array((frames + 1) * n);
    for (let f = 0; f < frames; f++) for (let j = 0; j < n; j++) out[(f + 1) * n + j] = out[f * n + j] + amp[f * n + j];
    return out;
  });

  let width = $state(0);
  /** Where the music is now (s), read each frame from the one clock (lib/trackPlace, as the visuals' Music reads it): no
   *  clock of its own, so through a wind, a brake or a rewind it moves as the music does. */
  // Its first frame sets it (the tick, or still: the effect below).
  let now = $state(0);
  /** The waves' own travel, run by `speed` while it plays. */
  let flow = $state(0);
  /** How much of the track's sound is let in: from nothing up when it arrives (a soft takeover, the owner). */
  let input = $state(1);
  /** +1: later music to the right (it flows left); -1: the other way. The waves' own travel goes the same way. */
  const way = $derived(direction === 'right' ? -1 : 1);

  // A new track's sound (or the first): let in from nothing.
  let heard: Spectrum | null = null;
  $effect(() => {
    if (spectrum !== heard) {
      heard = spectrum;
      input = 0;
    }
  });

  // The meter: each bar as shown (falling back no faster than Speed lets it) and its peak, held a moment, then falling.
  let bars = $state<number[]>([]);
  let peaks = $state<{ at: number; held: number; speed: number }[]>([]);

  let frame = 0;
  let last = 0;
  function tick(t: number) {
    const dt = last ? Math.min(0.1, (t - last) / 1000) : 0;
    last = t;
    flow += dt * speed;
    input = Math.min(1, input + dt / 0.9);
    now = clock(t);
    if (look === 'meter') fall(dt);
    frame = requestAnimationFrame(tick);
  }
  $effect(() => {
    if (paused || reducedMotion()) {
      now = clock();
      input = 1;
      if (look === 'meter') fall(0);
      return;
    }
    last = 0;
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  });
  onDestroy(() => cancelAnimationFrame(frame));

  /** The meter's bars now: each band's mean over the window up to now. */
  function meterNow(): number[] {
    const { frames, fps } = spectrum;
    const out = new Array(n).fill(0);
    if (!frames) return out;
    const to = Math.min(frames - 1, Math.floor(now * fps));
    const from = Math.max(0, Math.min(to, Math.floor((now - window) * fps) + 1));
    if (to < 0) return out;
    for (let f = from; f <= to; f++) for (let j = 0; j < n; j++) out[j] += grouped.db[f * n + j];
    return out.map((v) => Math.min(1, (v / (to - from + 1)) * height * input));
  }
  function fall(dt: number) {
    const target = meterNow();
    if (bars.length !== n) {
      bars = target;
      peaks = target.map((v) => ({ at: v, held: 0, speed: 0 }));
      return;
    }
    // Bars rise at once and fall back at Speed's pace; peaks hold a third of a second, then fall faster and faster.
    bars = bars.map((v, j) => Math.max(target[j], v - dt * 1.4 * speed));
    peaks = peaks.map((p, j) => {
      if (bars[j] >= p.at) return { at: bars[j], held: 0, speed: 0 };
      if (p.held < 0.35) return { ...p, held: p.held + dt };
      const s = p.speed + dt * 2.2 * speed;
      return { at: Math.max(bars[j], p.at - s * dt), held: p.held, speed: s };
    });
  }

  /** A soft ceiling: `v` as it is well below `top`, bending into it rather than cut off flat (a flat cut leaves a straight
   *  piece with a corner at each end; the owner: "unsmooth parts"). */
  const ceiling = (v: number, top: number) => (v <= top * 0.6 ? v : top * 0.6 + top * 0.4 * Math.tanh((v - top * 0.6) / (top * 0.4)));

  // Speed (the owner, 2026-10-24: the tablet held 34 frames a second, the main thread 97 % busy): everything a frame
  // reads that is reactive (props, $state, $derived) is read once into a plain local before the loops, as each read
  // through Svelte's getters cost more than the drawing; each band's loudness along the width is worked out once a frame
  // (`envelopes`), each frame's mean once, and shared by a band's two waves; the paths' numbers are written without
  // toFixed.

  /** Points across the width: where each is (`x`), how far along (`at`, 0..1) and from the middle (`u`, -1..1). */
  const moments = $derived.by(() => {
    if (!width || look === 'meter') return { x: new Float32Array(0), at: new Float32Array(0), u: new Float32Array(0) };
    // Eight points a cycle of the quickest wave, so many cycles across do not alias into a moiré; at most one every
    // pixel and a half (every two and a half with many bands to draw); at least one every three pixels (five with many
    // bands), so a tall crest turning fast stays round (the owner: one point every 14 px left it pointed).
    const fewest = Math.ceil(width / (n > 8 ? 5 : 3));
    const k = Math.max(fewest, Math.min(Math.ceil(width / (n > 8 ? 2.5 : 1.5)), Math.ceil(window * cyclesOf(n - 1) * cycles * 1.15 * 8)));
    const x = new Float32Array(k + 1), at = new Float32Array(k + 1), u = new Float32Array(k + 1);
    for (let i = 0; i <= k; i++) {
      at[i] = i / k;
      x[i] = at[i] * width;
      u[i] = at[i] * 2 - 1;
    }
    return { x, at, u };
  });
  /** Siri's attenuation, (K / (K + x^4))^K with K = 2: full in the middle, tapering to nothing at the ends (the owner:
   *  they should taper off to the ends), as much as Taper says. `u` is -1..1 across the width. */
  const taper = (u: number, reach: number) => 1 - tapering * (1 - Math.pow(2 / (2 + (u * reach) ** 4), 2));
  /** Each point's taper, for the curtains (reach 1.5) and the waves (reach 2): made again only for a new width or Taper. */
  const tapers = $derived.by(() => {
    const { u } = moments;
    const curtains = new Float32Array(u.length), lines = new Float32Array(u.length);
    for (let i = 0; i < u.length; i++) {
      curtains[i] = taper(u[i], 1.5);
      lines[i] = taper(u[i], 2);
    }
    return { curtains, lines };
  });
  /** Each band's place, cycles a second, pace, lift and middle line, once for its count and Gap. */
  const bandsOf = $derived(
    Array.from({ length: n }, (_, j) => ({
      place: place(j),
      cycles: cyclesOf(j),
      pace: paceOf(j),
      lift: place(j) * gap * (H * 0.62),
      middle: H / 2 + (0.5 - place(j)) * gap * (H * 0.62),
    })),
  );

  /** Each band's loudness at each point now, 0..1: the mean over the stretch the point stands for (at least a quarter
   *  second, and at least a third of the band's own wave: loudness changing faster than the wave it rides on pushes it
   *  up and down mid-swing), taken at each frame once and joined by a curve (Catmull-Rom), so it turns no corner at a
   *  frame (the owner: "rough peaks", "unsmooth parts", "still not awesome"). */
  const envelopes = $derived.by((): Float32Array[] => {
    const { at } = moments;
    const k = at.length;
    if (!k) return [];
    const { frames, fps } = spectrum;
    const S = sums, count = n, t0 = now, w = window, dir = way, dur = duration, gain = input, times = cycles, bands = bandsOf;
    const span = k > 1 ? w / (k - 1) : 0;
    const out: Float32Array[] = [];
    for (let j = 0; j < count; j++) {
      const env = new Float32Array(k);
      out.push(env);
      if (!frames) continue;
      // A third of the wave at most half a second each side: slow waves would otherwise average many seconds away, the
      // peaks with them (the owner: the height had to go to its top to reach what it reached before).
      const half = Math.max(0.125, span, Math.min(0.5, 0.35 / (bands[j].cycles * times))) * fps;
      const total = (f: number) => {
        const c = f < 0 ? 0 : f > frames ? frames : f;
        const i = Math.floor(c);
        const below = S[i * count + j];
        return i < frames ? below + (S[(i + 1) * count + j] - below) * (c - i) : below;
      };
      // Each frame's mean over the window's frames, once.
      const lo = Math.floor((t0 - w / 2) * fps) - 3, hi = Math.ceil((t0 + w / 2) * fps) + 3;
      const means = new Float32Array(hi - lo + 1);
      for (let i = lo; i <= hi; i++) means[i - lo] = (total(i + 0.5 + half) - total(i + 0.5 - half)) / (2 * half);
      for (let p = 0; p < k; p++) {
        const t = t0 + dir * (at[p] - 0.5) * w;
        if (t < 0 || t > dur) continue;
        const f = t * fps - 0.5;
        const i = Math.floor(f), x = f - i, b = i - lo;
        const p0 = means[b - 1], p1 = means[b], p2 = means[b + 1], p3 = means[b + 2];
        const y = p1 + 0.5 * x * (p2 - p0 + x * (2 * p0 - 5 * p1 + 4 * p2 - p3 + x * (3 * (p1 - p2) + p3 - p0)));
        env[p] = (y < 0 ? 0 : y > 1 ? 1 : y) * gain;
      }
    }
    return out;
  });

  /** A smooth curve through points, Catmull-Rom as cubic Béziers, forwards or backwards: the one curve both engines draw,
   *  its first point to `start`, each piece's two controls and its end to `piece`. */
  function through(xs: Float32Array, ys: Float32Array, backwards: boolean, start: (x: number, y: number) => void, piece: (x1: number, y1: number, x2: number, y2: number, x: number, y: number) => void) {
    const k = xs.length;
    const X = (i: number) => xs[backwards ? k - 1 - i : i], Y = (i: number) => ys[backwards ? k - 1 - i : i];
    start(X(0), Y(0));
    for (let i = 0; i < k - 1; i++) {
      const a = i > 0 ? i - 1 : 0, d = i + 2 < k ? i + 2 : k - 1;
      piece(X(i) + (X(i + 1) - X(a)) / 6, Y(i) + (Y(i + 1) - Y(a)) / 6, X(i + 1) - (X(d) - X(i)) / 6, Y(i + 1) - (Y(d) - Y(i)) / 6, X(i + 1), Y(i + 1));
    }
  }
  /** The curve onto `g`'s path (the canvas engine): from a move, or on from where the path is. */
  function trace(g: CanvasRenderingContext2D, xs: Float32Array, ys: Float32Array, move = true, backwards = false) {
    through(xs, ys, backwards, (x, y) => (move ? g.moveTo(x, y) : g.lineTo(x, y)), (x1, y1, x2, y2, x, y) => g.bezierCurveTo(x1, y1, x2, y2, x, y));
  }
  /** Easy ease (the owner): in and out softly, 0..1 to 0..1. */
  const ease = (x: number) => x * x * (3 - 2 * x);
  /** Many bands drawn over each other are each drawn lighter. */
  const share = $derived(Math.min(1, Math.sqrt(3 / n)));

  // Aurora: each band a curtain whose top is its loudness along the window, rippled by two waves of its own cycles.
  const aurora = $derived.by(() => {
    const env = envelopes;
    if (look !== 'aurora' || !env.length) return [];
    const { x, at } = moments;
    const k = at.length, t0 = now, w = window, dir = way, f0 = flow, times = cycles, tall = height;
    const cut = tapers.curtains;
    return bandsOf.map((band, j) => {
      const top = (shift: number, scale: number) => {
        const ys = new Float32Array(k);
        const travel = f0 * (0.6 + band.place * 0.8) + shift, travel2 = f0 * 1.3 + shift;
        const roof = H - 6 - band.lift;
        for (let p = 0; p < k; p++) {
          const c = 2 * Math.PI * band.cycles * times * (t0 + dir * (at[p] - 0.5) * w);
          const ripple = 0.74 + 0.16 * Math.sin(c + travel) + 0.1 * Math.sin(2.1 * c + travel2);
          ys[p] = H - 2 - band.lift - ceiling((6 + env[j][p] * 104 * tall) * scale * ripple, roof) * cut[p];
        }
        return ys;
      };
      return { j, front: top(j * 1.3, 1), back: top(j * 1.3 + 2.1, 0.8) };
    });
  });

  // Sirius and Hairlines: each band a wave of its own cycles (and a fainter one beside it, up to six bands), as tall as
  // the band is loud at each moment.
  type Wave = { j: number; rate: number; pace: number; shift: number; opacity: number; size: number };
  const waves = $derived<Wave[]>(
    bandsOf
      .map((band, j) => {
        const main = { j, rate: 1, pace: band.pace, shift: j * 1.3, opacity: 0.42, size: 1 };
        return n <= 6 ? [main, { j, rate: 1.15, pace: band.pace * 0.78, shift: j * 1.3 + 1.7, opacity: 0.3, size: 0.62 }] : [main];
      })
      .flat(),
  );
  /** Each wave's height above (and below) its middle line at each point now. */
  const swings = $derived.by((): Float32Array[] => {
    const env = envelopes;
    if ((look !== 'sirius' && look !== 'hairlines') || !env.length) return [];
    const { at } = moments;
    const k = at.length, t0 = now, w = window, dir = way, f0 = flow, times = cycles, tall = height, bands = bandsOf;
    const [base, scale] = look === 'sirius' ? [4, 56] : [3, 58];
    const cut = tapers.lines;
    return waves.map((wv) => {
      const ys = new Float32Array(k);
      const e = env[wv.j], rate = 2 * Math.PI * bands[wv.j].cycles * times * wv.rate, phase = f0 * wv.pace + wv.shift;
      for (let p = 0; p < k; p++) {
        const t = t0 + dir * (at[p] - 0.5) * w;
        ys[p] = ceiling((base + e[p] * scale * tall) * wv.size, H / 2 - 3) * cut[p] * Math.sin(rate * t + phase);
      }
      return ys;
    });
  });
  // The meter's layout: `n` bars with a gap between, each of small segments from the foot up. Its level runs up, so its
  // ends are its tops (the owner, 2026-10-24): Taper narrows the segments towards the top, Fade fades them out over
  // that share of the height.
  /** A segment's width at row `r` (of `rows`): whole at the foot, narrowing evenly up the bar, to an eighth at the top
   *  at full Taper (the owner: thinner bars at the top end; the waves' curve stays flat too long for a bar). */
  const segment = (w: number, r: number) => w * (1 - tapering * 0.88 * (r / (rows - 1)));
  /** A segment's opacity share at row `r`: fading out, eased, over the top `fading` share of the height. */
  const towardsTop = (r: number) => ease(Math.min(1, (1 - r / rows) / Math.max(0.01, fading)));
  const SEGMENT = 4;
  const STEP = 6;
  const rows = Math.floor((H - 2) / STEP);
  const meter = $derived.by(() => {
    if (look !== 'meter' || !width || bars.length !== n) return [];
    // Gap spreads the bars apart: at 1, the gaps as wide as the bars.
    const space = Math.max(2, Math.min(6, (width / n) * 0.22)) * (1 - gap) + (width / n) * 0.5 * gap;
    const w = (width - space * (n - 1)) / n;
    return Array.from({ length: n }, (_, j) => {
      // Bass on the left when the music flows left, on the right the other way.
      const slot = way === 1 ? j : n - 1 - j;
      const x = slot * (w + space);
      const lit = Math.round(bars[j] * rows);
      const peak = Math.min(rows - 1, Math.round(peaks[j].at * rows));
      return { j, x, w, lit, peak, showPeak: peaks[j].at > 0.02 };
    });
  });

  // The drawing, on a canvas (2026-10-24: the SVG repainted itself in a pass of its own for each effect, the blur, the
  // blend, the masks: 35 frames a second on the tablet): one bitmap a frame, the bands' light added ('lighter'), then
  // both sides faded in one wipe (the thin lines' 8 %, or as much as Fade says, eased).
  let canvas = $state<HTMLCanvasElement>();
  const TALL = H + 2 * OVER;
  $effect(() => {
    const cv = canvas;
    const W = width;
    if (engine !== 'canvas' || !cv || !W) return;
    const ratio = globalThis.devicePixelRatio || 1;
    const pw = Math.round(W * ratio), ph = Math.round(TALL * ratio);
    if (cv.width !== pw) cv.width = pw;
    if (cv.height !== ph) cv.height = ph;
    const g = cv.getContext('2d');
    if (!g) return;
    g.setTransform(ratio, 0, 0, ratio, 0, OVER * ratio);
    g.globalCompositeOperation = 'source-over';
    g.globalAlpha = 1;
    g.clearRect(0, -OVER, W, TALL);
    g.globalCompositeOperation = 'lighter';
    g.lineJoin = 'round';
    g.lineCap = 'round';
    const { x } = moments;
    const light = share, bands = bandsOf;
    if (look === 'aurora') {
      for (const c of aurora) {
        const f = bands[c.j].place;
        const fill = g.createLinearGradient(0, 0, 0, H);
        fill.addColorStop(0, hue(f, 0, 0.68 * light));
        fill.addColorStop(1, hue(f, 0, 0));
        g.fillStyle = fill;
        for (const [ys, alpha] of [[c.back, 0.35], [c.front, 1]] as const) {
          g.globalAlpha = alpha;
          g.beginPath();
          trace(g, x, ys);
          g.lineTo(W, H);
          g.lineTo(0, H);
          g.closePath();
          g.fill();
        }
        g.globalAlpha = 0.88 * light;
        g.strokeStyle = edges[c.j];
        g.lineWidth = 1.2;
        g.beginPath();
        trace(g, x, c.front);
        g.stroke();
      }
    } else if (look === 'sirius' || look === 'hairlines') {
      const s = swings, list = waves;
      if (look === 'sirius') {
        g.globalAlpha = 0.18;
        g.strokeStyle = '#fff';
        g.lineWidth = 1;
        g.beginPath();
        g.moveTo(0, H / 2);
        g.lineTo(W, H / 2);
        g.stroke();
      }
      list.forEach((wv, i) => {
        const sw = s[i];
        if (!sw) return;
        const c = bands[wv.j].middle;
        const up = new Float32Array(sw.length);
        for (let p = 0; p < sw.length; p++) up[p] = c - sw[p];
        if (look === 'sirius') {
          const down = new Float32Array(sw.length);
          for (let p = 0; p < sw.length; p++) down[p] = c + sw[p];
          g.globalAlpha = wv.opacity * light;
          g.fillStyle = colors[wv.j];
          g.beginPath();
          trace(g, x, up);
          trace(g, x, down, false, true);
          g.closePath();
          g.fill();
        } else {
          const main = wv.size === 1;
          g.strokeStyle = edges[wv.j];
          g.beginPath();
          trace(g, x, up);
          // Its glow: a soft, wider stroke under it.
          g.globalAlpha = (main ? 0.16 : 0.08) * light;
          g.lineWidth = main ? 6 : 4;
          g.stroke();
          g.globalAlpha = (main ? 0.95 : 0.4) * light;
          g.lineWidth = main ? 1.5 : 0.9;
          g.stroke();
        }
      });
    } else if (look === 'meter') {
      for (const b of meter) {
        for (let r = 0; r < rows; r++) {
          const lit = r < b.lit, peak = !lit && r === b.peak && b.showPeak;
          const w = segment(b.w, r);
          g.globalAlpha = (lit ? 0.85 : peak ? 0.95 : 0.05) * towardsTop(r);
          g.fillStyle = lit ? colors[b.j] : peak ? edges[b.j] : '#fff';
          g.fillRect(b.x + (b.w - w) / 2, H - 1 - (r + 1) * STEP + (STEP - SEGMENT), w, SEGMENT);
        }
      }
    }
    // Both sides fade: the thin lines' 8 % (the meter's 6 %), or over as much as Fade says, eased.
    g.globalCompositeOperation = 'destination-in';
    g.globalAlpha = 1;
    const reach = look === 'meter' ? 0.06 : Math.max(0.08, fading * 0.45);
    const wipe = g.createLinearGradient(0, 0, W, 0);
    for (let k = 0; k <= 8; k++) {
      const o = look !== 'meter' && fading > 0 ? ease(k / 8) : k / 8;
      wipe.addColorStop((k / 8) * reach, `rgba(0,0,0,${o})`);
      wipe.addColorStop(1 - (k / 8) * reach, `rgba(0,0,0,${o})`);
    }
    g.fillStyle = wipe;
    g.fillRect(0, -OVER, W, TALL);
    g.globalCompositeOperation = 'source-over';
  });

  // The SVG engine (Quality): the same shapes as paths, with their true blur, blend and masks.
  /** A number for a path, to a tenth. */
  const r1 = (v: number) => Math.round(v * 10) / 10;
  /** The curve as path text (the SVG engine), from `M` (or `L` to go on). */
  function curve(xs: Float32Array, ys: Float32Array, start = 'M', backwards = false): string {
    const parts: (string | number)[] = [];
    through(xs, ys, backwards, (x, y) => parts.push(start, r1(x), r1(y)), (x1, y1, x2, y2, x, y) => parts.push('C', r1(x1), r1(y1), r1(x2), r1(y2), r1(x), r1(y)));
    return parts.join(' ');
  }
  /** Fade's ramp at each side: over `fading` × 45 % of the width, eased, as gradient stops. */
  const ramp = $derived(Array.from({ length: 9 }, (_, k) => ({ at: (k / 8) * fading * 0.45, opacity: ease(k / 8) })));
  type SvgPaths = {
    aurora: { j: number; edge: string; fill: string; back: string }[];
    ribbons: { d: string; color: string; opacity: number }[];
    hairlines: { d: string; color: string; main: boolean }[];
  };
  const paths = $derived.by((): SvgPaths => {
    const none: SvgPaths = { aurora: [], ribbons: [], hairlines: [] };
    if (engine !== 'svg') return none;
    const { x } = moments;
    const W = width, bands = bandsOf, light = share;
    if (look === 'aurora')
      return {
        ...none,
        aurora: aurora.map((c) => {
          const edge = curve(x, c.front);
          return { j: c.j, edge, fill: `${edge} L ${W} ${H} L 0 ${H} Z`, back: `${curve(x, c.back)} L ${W} ${H} L 0 ${H} Z` };
        }),
      };
    const lines = (sign: number) =>
      waves.map((wv, i) => {
        const sw = swings[i] ?? new Float32Array(0), c = bands[wv.j].middle;
        const ys = new Float32Array(sw.length);
        for (let p = 0; p < sw.length; p++) ys[p] = c + sign * sw[p];
        return ys;
      });
    if (look === 'sirius' && swings.length) {
      const up = lines(-1), down = lines(1);
      return { ...none, ribbons: waves.map((wv, i) => ({ d: `${curve(x, up[i])} ${curve(x, down[i], 'L', true)} Z`, color: colors[wv.j], opacity: wv.opacity * light })) };
    }
    if (look === 'hairlines' && swings.length) {
      const up = lines(-1);
      return { ...none, hairlines: waves.map((wv, i) => ({ d: curve(x, up[i]), color: edges[wv.j], main: wv.size === 1 })) };
    }
    return none;
  });
</script>

<div class="glass" bind:clientWidth={width} aria-hidden="true">
  {#if width && engine === 'canvas'}
    <canvas bind:this={canvas} style:width="{width}px" style:height="{TALL}px"></canvas>
  {:else if width}
    <svg class="svg" {width} height={TALL} viewBox="0 {-OVER} {width} {TALL}">
      <!-- The thin lines' ends always fade (the owner: never a hard edge); Fade fades the whole drawing, eased. -->
      <defs>
        <linearGradient id="{uid}-fade" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#fff" stop-opacity="0" />
          <stop offset="0.08" stop-color="#fff" stop-opacity="1" />
          <stop offset="0.92" stop-color="#fff" stop-opacity="1" />
          <stop offset="1" stop-color="#fff" stop-opacity="0" />
        </linearGradient>
        <mask id="{uid}-ends" maskUnits="userSpaceOnUse" x="0" y={-OVER} width={width} height={TALL}>
          <rect x="0" y={-OVER} width={width} height={TALL} fill="url(#{uid}-fade)" />
        </mask>
        <linearGradient id="{uid}-wide" x1="0" y1="0" x2="1" y2="0">
          {#each ramp as r, k (k)}<stop offset={r.at} stop-color="#fff" stop-opacity={r.opacity} />{/each}
          {#each [...ramp].reverse() as r, k (k)}<stop offset={1 - r.at} stop-color="#fff" stop-opacity={r.opacity} />{/each}
        </linearGradient>
        <mask id="{uid}-faded" maskUnits="userSpaceOnUse" x="0" y={-OVER} width={width} height={TALL}>
          <rect x="0" y={-OVER} width={width} height={TALL} fill="url(#{uid}-wide)" />
        </mask>
      </defs>
      <g mask={fading > 0 && look !== 'meter' ? `url(#${uid}-faded)` : undefined}>
        {#if look === 'aurora'}
          <defs>
            {#each bandsOf as _, j (j)}
              <linearGradient id="{uid}-c{j}" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stop-color={colors[j]} stop-opacity={0.68 * share} />
                <stop offset="1" stop-color={colors[j]} stop-opacity="0" />
              </linearGradient>
            {/each}
            <filter id="{uid}-soft" x="-5%" y="-30%" width="110%" height="160%"><feGaussianBlur stdDeviation="1.4" /></filter>
            <filter id="{uid}-haze" x="-5%" y="-30%" width="110%" height="160%"><feGaussianBlur stdDeviation="4" /></filter>
          </defs>
          <g filter="url(#{uid}-haze)" opacity="0.35">
            {#each paths.aurora as c (c.j)}<path d={c.back} fill="url(#{uid}-c{c.j})" />{/each}
          </g>
          <g filter="url(#{uid}-soft)">
            {#each paths.aurora as c (c.j)}<path d={c.fill} fill="url(#{uid}-c{c.j})" />{/each}
          </g>
          <g filter="url(#{uid}-soft)" mask="url(#{uid}-ends)">
            {#each paths.aurora as c (c.j)}<path d={c.edge} stroke={edges[c.j]} stroke-width="1.2" stroke-opacity={0.88 * share} fill="none" stroke-linejoin="round" />{/each}
          </g>
        {:else if look === 'sirius'}
          <defs><filter id="{uid}-soft" x="-5%" y="-20%" width="110%" height="140%"><feGaussianBlur stdDeviation="0.6" /></filter></defs>
          <line x1="0" y1={H / 2} x2={width} y2={H / 2} stroke="#fff" stroke-opacity="0.18" mask="url(#{uid}-ends)" />
          <g filter="url(#{uid}-soft)">
            {#each paths.ribbons as r, i (i)}<path d={r.d} fill={r.color} fill-opacity={r.opacity} />{/each}
          </g>
        {:else if look === 'hairlines'}
          <defs>
            <filter id="{uid}-glow" x="-5%" y="-40%" width="110%" height="180%">
              <feGaussianBlur in="SourceGraphic" stdDeviation="2.4" result="b" />
              <feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge>
            </filter>
          </defs>
          <g mask="url(#{uid}-ends)">
            <g filter="url(#{uid}-glow)" fill="none" stroke-linecap="round" stroke-linejoin="round">
              {#each paths.hairlines as l, i (i)}<path d={l.d} stroke={l.color} stroke-width={l.main ? 1.5 : 0.9} stroke-opacity={(l.main ? 0.95 : 0.4) * share} />{/each}
            </g>
          </g>
        {:else if look === 'meter'}
          {#each meter as b (b.j)}
            {#each { length: rows } as _, r (r)}
              <rect
                x={b.x + (b.w - segment(b.w, r)) / 2}
                y={H - 1 - (r + 1) * STEP + (STEP - SEGMENT)}
                width={segment(b.w, r)}
                height={SEGMENT}
                rx={Math.min(1.5, segment(b.w, r) / 4)}
                fill={r < b.lit ? colors[b.j] : r === b.peak && b.showPeak ? edges[b.j] : '#fff'}
                fill-opacity={(r < b.lit ? 0.85 : r === b.peak && b.showPeak ? 0.95 : 0.05) * towardsTop(r)}
              />
            {/each}
          {/each}
        {/if}
      </g>
    </svg>
  {/if}
</div>

<style>
  /* The box keeps the layout's height; the drawing reaches OVER (40 px) past it above and below, its sides faded in the
     drawing itself. */
  .glass {
    position: relative;
    width: 100%;
    height: 132px;
    pointer-events: none;
  }

  canvas,
  .svg {
    position: absolute;
    top: -40px;
    left: 0;
    display: block;
    mix-blend-mode: plus-lighter;
  }

  /* SVG: the glass's feather on the drawing too. */
  .svg {
    overflow: visible;
    -webkit-mask-image: linear-gradient(90deg, transparent 0%, #000 6%, #000 94%, transparent 100%);
    mask-image: linear-gradient(90deg, transparent 0%, #000 6%, #000 94%, transparent 100%);
  }
</style>
