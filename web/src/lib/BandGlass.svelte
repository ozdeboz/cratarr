<script lang="ts">
  // The track in three bands, drawn straight on the player's glass (the owner, 2026-10-24): no box, both ends feathered,
  // its light added to what is behind it (plus-lighter). Three looks, chosen in Settings › Display › Player:
  // - Aurora: each band a soft field of light rising from the foot, a fine bright edge on top; the whole track, the part
  //   played lit, the part ahead a ghost; a glowing playhead.
  // - Sirius: Siri's ribbons, each band two of them, as tall as the band is loud where the music is now (the kept bins
  //   around the playhead, so it moves on every device, no live audio needed), oscillating while it plays.
  // - Hairlines: three fine glowing contours over one faint haze, a dot where each meets the playhead.
  // Low is coral, mid mint, high sky: they differ in lightness too, not in hue alone.
  import { onDestroy } from 'svelte';
  import type { BandLook, BandWave } from './bandWaves.svelte';
  import { reducedMotion } from './motion';

  let { bands, progress, paused, look }: { bands: BandWave; progress: number; paused: boolean; look: BandLook } = $props();

  const H = 132;
  const COLORS = { low: '#ff7d6e', mid: '#6ff0b4', high: '#86c6ff' } as const;
  const EDGES = { low: '#ffb3a8', mid: '#b6f7d6', high: '#c3e2ff' } as const;
  const LINES = { low: '#ff8f80', mid: '#7af2bd', high: '#93ccff' } as const;
  const KEYS = ['low', 'mid', 'high'] as const;
  const uid = $props.id();

  let width = $state(0);
  const at = $derived(Math.max(0, Math.min(1, progress)));
  const x = $derived(width * at);

  /** A band as `n + 1` points along the width: each the mean of its stretch of bins, 0..1. */
  function points(bins: Uint8Array, n: number, scale: number): [number, number][] {
    return Array.from({ length: n + 1 }, (_, j) => {
      const c = (j / n) * (bins.length - 1);
      const from = Math.max(0, Math.floor(c - bins.length / n / 2));
      const to = Math.min(bins.length, Math.max(from + 1, Math.ceil(c + bins.length / n / 2)));
      let s = 0;
      for (let i = from; i < to; i++) s += bins[i];
      return [(j / n) * width, H - 2 - (s / (to - from) / 255) * scale];
    });
  }

  /** A smooth curve through the points (Catmull-Rom as cubic Béziers). */
  function smooth(p: [number, number][]): string {
    let d = `M${p[0][0].toFixed(1)} ${p[0][1].toFixed(1)}`;
    for (let i = 0; i < p.length - 1; i++) {
      const p0 = p[Math.max(0, i - 1)], p1 = p[i], p2 = p[i + 1], p3 = p[Math.min(p.length - 1, i + 2)];
      d += ` C${(p1[0] + (p2[0] - p0[0]) / 6).toFixed(1)} ${(p1[1] + (p2[1] - p0[1]) / 6).toFixed(1)} ${(p2[0] - (p3[0] - p1[0]) / 6).toFixed(1)} ${(p2[1] - (p3[1] - p1[1]) / 6).toFixed(1)} ${p2[0].toFixed(1)} ${p2[1].toFixed(1)}`;
    }
    return d;
  }
  const closed = (d: string) => `${d} L${width} ${H} L0 ${H} Z`;

  // Aurora and Hairlines: the whole track's shapes, made again only for a new track or width.
  const shapes = $derived.by(() => {
    if (!width || look === 'sirius') return null;
    const scale = look === 'aurora' ? 112 : 96;
    const p = KEYS.map((k) => points(bands[k], 128, scale));
    const lines = p.map(smooth);
    const haze = closed(smooth(p[0].map((q, j) => [q[0], Math.min(q[1], p[1][j][1], p[2][j][1]) + 6] as [number, number])));
    return { p, lines, fills: lines.map(closed), haze };
  });
  /** Where each band's contour meets the playhead (Hairlines' dots). */
  const dots = $derived(shapes ? shapes.p.map((p) => p[Math.round(at * 128)][1]) : [0, 0, 0]);

  // Sirius: each band's level now, from the kept bins around the playhead, eased; the ribbons' phase runs while it plays.
  const levelAt = (bins: Uint8Array) => {
    const c = Math.round(at * (bins.length - 1));
    let s = 0;
    for (let i = c - 4; i <= c + 4; i++) s += bins[Math.max(0, Math.min(bins.length - 1, i))];
    return s / 9 / 255;
  };
  let eased = $state([0, 0, 0]);
  let phase = $state(0);
  let frame = 0;
  let last = 0;
  function tick(t: number) {
    const dt = last ? Math.min(0.1, (t - last) / 1000) : 0;
    last = t;
    phase += dt;
    const target = KEYS.map((k) => levelAt(bands[k]));
    eased = eased.map((v, i) => v + (target[i] - v) * Math.min(1, dt * 4));
    frame = requestAnimationFrame(tick);
  }
  $effect(() => {
    if (look !== 'sirius') return;
    if (paused || reducedMotion()) {
      // Still: the ribbons as loud as the music is here.
      eased = KEYS.map((k) => levelAt(bands[k]));
      return;
    }
    last = 0;
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  });
  onDestroy(() => cancelAnimationFrame(frame));

  /** One ribbon: the wave and its mirror closed, tapering by Siri's attenuation (K / (K + x^4))^K, K = 2. */
  function ribbon(amp: number, waves: number, ph: number): string {
    const left = 40, span = Math.max(1, width - 80), top: string[] = [], bottom: string[] = [];
    for (let j = 0; j <= 160; j++) {
      const t = (j / 160) * 4 - 2;
      const y = amp * Math.pow(2 / (2 + t ** 4), 2) * Math.sin(waves * t - ph);
      const px = (left + (j / 160) * span).toFixed(1);
      top.push(`${px} ${(H / 2 - y).toFixed(1)}`);
      bottom.push(`${px} ${(H / 2 + y).toFixed(1)}`);
    }
    return `M${top.join(' L')} L${bottom.reverse().join(' L')} Z`;
  }
  // Low long and slow, high short and quick.
  const RIBBONS = [
    { band: 0, waves: 1.6, speed: 1.1, shift: 0, opacity: 0.42, size: 1 },
    { band: 0, waves: 1.84, speed: 0.8, shift: 1.7, opacity: 0.3, size: 0.62 },
    { band: 1, waves: 3.2, speed: 2.2, shift: 1.1, opacity: 0.42, size: 1 },
    { band: 1, waves: 3.68, speed: 1.7, shift: 2.8, opacity: 0.3, size: 0.62 },
    { band: 2, waves: 6, speed: 3.6, shift: 2.3, opacity: 0.42, size: 1 },
    { band: 2, waves: 6.9, speed: 2.9, shift: 4, opacity: 0.3, size: 0.62 },
  ] as const;
  const ribbons = $derived(
    look === 'sirius' && width
      ? RIBBONS.map((r) => ({ d: ribbon((8 + eased[r.band] * 52) * r.size, r.waves, r.shift + phase * r.speed), color: COLORS[KEYS[r.band]], opacity: r.opacity }))
      : [],
  );
</script>

<div class="glass" bind:clientWidth={width} aria-hidden="true">
  {#if width}
    <svg {width} height={H} viewBox="0 0 {width} {H}">
      {#if look === 'aurora' && shapes}
        <defs>
          {#each KEYS as k (k)}
            <linearGradient id="{uid}-{k}" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stop-color={COLORS[k]} stop-opacity={k === 'mid' ? 0.6 : 0.72} />
              <stop offset="1" stop-color={COLORS[k]} stop-opacity="0" />
            </linearGradient>
          {/each}
          <filter id="{uid}-soft" x="-5%" y="-30%" width="110%" height="160%"><feGaussianBlur stdDeviation="1.4" /></filter>
          <filter id="{uid}-glow" x="-50%" y="-10%" width="200%" height="120%"><feGaussianBlur stdDeviation="3" /></filter>
          <clipPath id="{uid}-played"><rect x="0" y="0" width={x} height={H} /></clipPath>
          <clipPath id="{uid}-ahead"><rect {x} y="0" width={Math.max(0, width - x)} height={H} /></clipPath>
        </defs>
        <g clip-path="url(#{uid}-played)" filter="url(#{uid}-soft)">
          {#each KEYS as k, i (k)}<path d={shapes.fills[i]} fill="url(#{uid}-{k})" />{/each}
          {#each KEYS as k, i (k)}<path d={shapes.lines[i]} stroke={EDGES[k]} stroke-width="1.2" stroke-opacity="0.88" fill="none" />{/each}
        </g>
        <g clip-path="url(#{uid}-ahead)" filter="url(#{uid}-soft)" opacity="0.28">
          {#each KEYS as k, i (k)}<path d={shapes.fills[i]} fill="url(#{uid}-{k})" />{/each}
        </g>
        <rect x={x - 3} y="6" width="6" height={H - 6} fill="#fff" opacity="0.35" filter="url(#{uid}-glow)" />
        <rect x={x - 0.75} y="10" width="1.5" height={H - 10} rx="0.75" fill="#fff" opacity="0.9" />
      {:else if look === 'hairlines' && shapes}
        <defs>
          <filter id="{uid}-glow" x="-5%" y="-40%" width="110%" height="180%">
            <feGaussianBlur in="SourceGraphic" stdDeviation="2.4" result="b" />
            <feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge>
          </filter>
          <linearGradient id="{uid}-haze" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stop-color="#fff" stop-opacity="0.1" />
            <stop offset="1" stop-color="#fff" stop-opacity="0" />
          </linearGradient>
          <clipPath id="{uid}-played"><rect x="0" y="0" width={x} height={H} /></clipPath>
          <clipPath id="{uid}-ahead"><rect {x} y="0" width={Math.max(0, width - x)} height={H} /></clipPath>
        </defs>
        <path d={shapes.haze} fill="url(#{uid}-haze)" />
        <g clip-path="url(#{uid}-played)" filter="url(#{uid}-glow)">
          {#each KEYS as k, i (k)}<path d={shapes.lines[i]} stroke={LINES[k]} stroke-width="1.5" fill="none" stroke-linecap="round" />{/each}
        </g>
        <g clip-path="url(#{uid}-ahead)" opacity="0.3">
          {#each KEYS as k, i (k)}<path d={shapes.lines[i]} stroke="#fff" stroke-width="1.2" fill="none" />{/each}
        </g>
        <rect x={x - 0.5} y="8" width="1" height={H - 8} fill="#fff" opacity="0.55" />
        {#each KEYS as k, i (k)}<circle cx={x} cy={dots[i]} r="3.5" fill={EDGES[k]} filter="url(#{uid}-glow)" />{/each}
      {:else if look === 'sirius'}
        <defs><filter id="{uid}-soft" x="-5%" y="-20%" width="110%" height="140%"><feGaussianBlur stdDeviation="0.6" /></filter></defs>
        <line x1="40" y1={H / 2} x2={width - 40} y2={H / 2} stroke="#fff" stroke-opacity="0.18" />
        <g filter="url(#{uid}-soft)">
          {#each ribbons as r, i (i)}<path d={r.d} fill={r.color} fill-opacity={r.opacity} />{/each}
        </g>
      {/if}
    </svg>
  {/if}
</div>

<style>
  .glass {
    width: 100%;
    height: 132px;
    mask-image: linear-gradient(90deg, transparent 0%, #000 6%, #000 94%, transparent 100%);
    pointer-events: none;
  }

  svg {
    display: block;
    mix-blend-mode: plus-lighter;
  }
</style>
