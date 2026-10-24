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
    progress,
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
    ends = 'taper',
  }: {
    spectrum: Spectrum;
    progress: number;
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
    /** How it ends at both sides: tapering to nothing (Siri's), or full height fading out. */
    ends?: 'taper' | 'fade';
  } = $props();

  const H = 132;
  const FLOOR_DB = 60;
  const uid = $props.id();

  const n = $derived(Math.max(1, Math.round(count)));
  /** A band's place from low (0) to high (1). */
  const place = (j: number) => (n > 1 ? j / (n - 1) : 0.5);
  /** Low to high: coral, mint, sky. */
  const STOPS: [number, number, number][] = [
    [0xff, 0x7d, 0x6e],
    [0x6f, 0xf0, 0xb4],
    [0x86, 0xc6, 0xff],
  ];
  function hue(f: number, white = 0): string {
    const x = f * (STOPS.length - 1);
    const i = Math.min(STOPS.length - 2, Math.floor(x));
    const c = STOPS[i].map((v, k) => v + (STOPS[i + 1][k] - v) * (x - i)).map((v) => Math.round(v + (255 - v) * white));
    return `rgb(${c[0]} ${c[1]} ${c[2]})`;
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
  const smooth = $derived.by(() => {
    const frames = spectrum.frames;
    const amp = grouped.amp;
    const out = new Float32Array(amp.length);
    const at = (f: number, j: number) => amp[Math.max(0, Math.min(frames - 1, f)) * n + j];
    for (let f = 0; f < frames; f++)
      for (let j = 0; j < n; j++) out[f * n + j] = (at(f - 2, j) + 4 * at(f - 1, j) + 6 * at(f, j) + 4 * at(f + 1, j) + at(f + 2, j)) / 16;
    return out;
  });

  let width = $state(0);
  /** Where the playback is now (s): the progress it is told, run on between its updates so the flow is smooth. */
  let now = $state(0);
  /** The waves' own travel, run by `speed` while it plays. */
  let flow = $state(0);
  /** How much of the track's sound is let in: from nothing up when it arrives (a soft takeover, the owner). */
  let input = $state(1);
  const told = $derived(Math.max(0, Math.min(1, progress)) * duration);
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
    // A seek (or a long gap) jumps; small drifts are taken in gently.
    now = Math.abs(told - now) > 0.75 ? told : now + dt + (told - now) * Math.min(1, dt * 2);
    if (look === 'meter') fall(dt);
    frame = requestAnimationFrame(tick);
  }
  $effect(() => {
    if (paused || reducedMotion()) {
      now = told;
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

  /** A band's loudness at `t` seconds into the track, 0..1: a curve through the smoothed frames (Catmull-Rom), so it
   *  has no corners where it turns; none outside the track. */
  function loud(j: number, t: number): number {
    const { frames, fps } = spectrum;
    if (!frames || t < 0 || t > duration) return 0;
    const f = Math.max(0, t * fps - 0.5);
    const i = Math.min(frames - 1, Math.floor(f));
    const x = f - i;
    const v = (k: number) => smooth[Math.max(0, Math.min(frames - 1, k)) * n + j];
    const p0 = v(i - 1), p1 = v(i), p2 = v(i + 1), p3 = v(i + 2);
    const y = p1 + 0.5 * x * (p2 - p0 + x * (2 * p0 - 5 * p1 + 4 * p2 - p3 + x * (3 * (p1 - p2) + p3 - p0)));
    return Math.max(0, Math.min(1, y)) * input;
  }

  /** Points across the width: where each is (`x`), how far along (`at`, 0..1) and from the middle (`u`, -1..1). */
  const moments = $derived.by(() => {
    if (!width || look === 'meter') return [];
    // Eight points a cycle of the quickest wave, so many cycles across do not alias into a moiré; at most one every
    // pixel and a half (every three with many bands to draw).
    const k = Math.max(64, Math.min(Math.ceil(width / (n > 8 ? 3 : 1.5)), Math.ceil(window * cyclesOf(n - 1) * cycles * 1.15 * 8)));
    return Array.from({ length: k + 1 }, (_, i) => ({ x: (i / k) * width, u: (i / k) * 2 - 1, at: i / k }));
  });
  /** Siri's attenuation, (K / (K + x^4))^K with K = 2: full in the middle, tapering to nothing at the ends (the owner:
   *  they should taper off to the ends); none when the ends fade instead. `u` is -1..1 across the width. */
  const taper = (u: number, reach: number) => (ends === 'fade' ? 1 : Math.pow(2 / (2 + (u * reach) ** 4), 2));
  const xy = (x: number, y: number) => `${x.toFixed(1)} ${y.toFixed(1)}`;
  /** How high a band's line is lifted by Gap (px): low at the foot, high at the top. */
  const lift = (j: number) => place(j) * gap * (H * 0.62);
  /** A wave's middle line: the middle, spread by Gap, low below and high above. */
  const middle = (j: number) => H / 2 + (0.5 - place(j)) * gap * (H * 0.62);
  /** Many bands drawn over each other are each drawn lighter. */
  const share = $derived(Math.min(1, Math.sqrt(3 / n)));

  // Aurora: each band a curtain whose top is its loudness along the window, rippled by two waves of its own cycles.
  const aurora = $derived.by(() => {
    if (look !== 'aurora' || !moments.length) return [];
    return Array.from({ length: n }, (_, j) => {
      const top = (shift: number, scale: number) =>
        moments.map((m) => {
          const t = now + way * (m.at - 0.5) * window;
          const c = 2 * Math.PI * cyclesOf(j) * cycles * t;
          const ripple = 0.74 + 0.16 * Math.sin(c + flow * (0.6 + place(j) * 0.8) + shift) + 0.1 * Math.sin(2.1 * c + flow * 1.3 + shift);
          const rise = Math.min(H - 6 - lift(j), (6 + loud(j, t) * 104 * height) * scale * ripple);
          return xy(m.x, H - 2 - lift(j) - rise * taper(m.u, 1.5));
        });
      const front = top(j * 1.3, 1);
      const back = top(j * 1.3 + 2.1, 0.8);
      return { j, edge: `M${front.join(' L')}`, fill: `M${front.join(' L')} L${width} ${H} L0 ${H} Z`, back: `M${back.join(' L')} L${width} ${H} L0 ${H} Z` };
    });
  });

  // Sirius and Hairlines: each band a wave of its own cycles (and a fainter one beside it, up to six bands), as tall as
  // the band is loud at each moment.
  type Wave = { j: number; rate: number; pace: number; shift: number; opacity: number; size: number };
  const waves = $derived<Wave[]>(
    Array.from({ length: n }, (_, j) => {
      const main = { j, rate: 1, pace: paceOf(j), shift: j * 1.3, opacity: 0.42, size: 1 };
      return n <= 6 ? [main, { j, rate: 1.15, pace: paceOf(j) * 0.78, shift: j * 1.3 + 1.7, opacity: 0.3, size: 0.62 }] : [main];
    }).flat(),
  );
  function wave(w: Wave, base: number, scale: number): [number, number][] {
    return moments.map((m) => {
      const t = now + way * (m.at - 0.5) * window;
      const amp = Math.min(H / 2 - 3, (base + loud(w.j, t) * scale * height) * w.size) * taper(m.u, 2);
      return [m.x, amp * Math.sin(2 * Math.PI * cyclesOf(w.j) * cycles * w.rate * t + flow * w.pace + w.shift)];
    });
  }
  const ribbons = $derived(
    look === 'sirius' && moments.length
      ? waves.map((w) => {
          const p = wave(w, 4, 56);
          const c = middle(w.j);
          const d = `M${p.map(([x, y]) => xy(x, c - y)).join(' L')} L${p.map(([x, y]) => xy(x, c + y)).reverse().join(' L')} Z`;
          return { d, color: colors[w.j], opacity: w.opacity * share };
        })
      : [],
  );
  const hairlines = $derived(
    look === 'hairlines' && moments.length
      ? waves.map((w) => ({ d: `M${wave(w, 3, 58).map(([x, y]) => xy(x, middle(w.j) - y)).join(' L')}`, color: edges[w.j], main: w.size === 1 }))
      : [],
  );

  // The meter's layout: `n` bars with a gap between, each of small segments from the foot up.
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
      const u = n > 1 ? ((x + w / 2) / width) * 2 - 1 : 0;
      const reach = taper(u, 1.5);
      const lit = Math.round(bars[j] * reach * rows);
      const peak = Math.min(rows - 1, Math.round(peaks[j].at * reach * rows));
      return { j, x, w, lit, peak, showPeak: peaks[j].at * reach > 0.02 };
    });
  });
</script>

<div class="glass" bind:clientWidth={width} aria-hidden="true">
  {#if width}
    <svg {width} height={H} viewBox="0 0 {width} {H}">
      <!-- The thin lines' ends always fade (the owner: never a hard edge), in the drawing itself; the fills keep the
           glass's feather. -->
      <defs>
        <linearGradient id="{uid}-fade" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#fff" stop-opacity="0" />
          <stop offset="0.08" stop-color="#fff" stop-opacity="1" />
          <stop offset="0.92" stop-color="#fff" stop-opacity="1" />
          <stop offset="1" stop-color="#fff" stop-opacity="0" />
        </linearGradient>
        <mask id="{uid}-ends" maskUnits="userSpaceOnUse" x="0" y="0" width={width} height={H}>
          <rect x="0" y="0" width={width} height={H} fill="url(#{uid}-fade)" />
        </mask>
        <!-- Ends: Fade, the whole drawing fading out over a fifth at each side. -->
        <linearGradient id="{uid}-wide" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stop-color="#fff" stop-opacity="0" />
          <stop offset="0.2" stop-color="#fff" stop-opacity="1" />
          <stop offset="0.8" stop-color="#fff" stop-opacity="1" />
          <stop offset="1" stop-color="#fff" stop-opacity="0" />
        </linearGradient>
        <mask id="{uid}-faded" maskUnits="userSpaceOnUse" x="0" y="0" width={width} height={H}>
          <rect x="0" y="0" width={width} height={H} fill="url(#{uid}-wide)" />
        </mask>
      </defs>
      <g mask={ends === 'fade' ? `url(#${uid}-faded)` : undefined}>
        {#if look === 'aurora'}
          <defs>
            {#each colors as c, j (j)}
              <linearGradient id="{uid}-c{j}" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stop-color={c} stop-opacity={0.68 * share} />
                <stop offset="1" stop-color={c} stop-opacity="0" />
              </linearGradient>
            {/each}
            <filter id="{uid}-soft" x="-5%" y="-30%" width="110%" height="160%"><feGaussianBlur stdDeviation="1.4" /></filter>
            <filter id="{uid}-haze" x="-5%" y="-30%" width="110%" height="160%"><feGaussianBlur stdDeviation="4" /></filter>
          </defs>
          <g filter="url(#{uid}-haze)" opacity="0.35">
            {#each aurora as c (c.j)}<path d={c.back} fill="url(#{uid}-c{c.j})" />{/each}
          </g>
          <g filter="url(#{uid}-soft)">
            {#each aurora as c (c.j)}<path d={c.fill} fill="url(#{uid}-c{c.j})" />{/each}
          </g>
          <g filter="url(#{uid}-soft)" mask="url(#{uid}-ends)">
            {#each aurora as c (c.j)}<path d={c.edge} stroke={edges[c.j]} stroke-width="1.2" stroke-opacity={0.88 * share} fill="none" stroke-linejoin="round" />{/each}
          </g>
        {:else if look === 'sirius'}
          <defs><filter id="{uid}-soft" x="-5%" y="-20%" width="110%" height="140%"><feGaussianBlur stdDeviation="0.6" /></filter></defs>
          <line x1="0" y1={H / 2} x2={width} y2={H / 2} stroke="#fff" stroke-opacity="0.18" mask="url(#{uid}-ends)" />
          <g filter="url(#{uid}-soft)">
            {#each ribbons as r, i (i)}<path d={r.d} fill={r.color} fill-opacity={r.opacity} />{/each}
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
              {#each hairlines as l, i (i)}<path d={l.d} stroke={l.color} stroke-width={l.main ? 1.5 : 0.9} stroke-opacity={(l.main ? 0.95 : 0.4) * share} />{/each}
            </g>
          </g>
        {:else if look === 'meter'}
          <!-- Each bar: its segments faint, those lit up to its level, and its peak a brighter segment above. -->
          {#each meter as b (b.j)}
            {#each { length: rows } as _, r (r)}
              <rect
                x={b.x}
                y={H - 1 - (r + 1) * STEP + (STEP - SEGMENT)}
                width={b.w}
                height={SEGMENT}
                rx={Math.min(1.5, b.w / 4)}
                fill={r < b.lit ? colors[b.j] : r === b.peak && b.showPeak ? edges[b.j] : '#fff'}
                fill-opacity={r < b.lit ? 0.85 : r === b.peak && b.showPeak ? 0.95 : 0.05}
              />
            {/each}
          {/each}
        {/if}
      </g>
    </svg>
  {/if}
</div>

<style>
  .glass {
    width: 100%;
    height: 132px;
    -webkit-mask-image: linear-gradient(90deg, transparent 0%, #000 6%, #000 94%, transparent 100%);
    mask-image: linear-gradient(90deg, transparent 0%, #000 6%, #000 94%, transparent 100%);
    pointer-events: none;
  }

  svg {
    display: block;
    mix-blend-mode: plus-lighter;
  }
</style>
