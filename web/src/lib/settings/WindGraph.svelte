<script lang="ts">
  // Winding's envelope, drawn (the owner, 2026-10-27: a sleek way of showing it), above its sliders: a tap rewind of
  // the track playing now (else from 3:20) at the settings as they are, its speed over its time: the spin-up, the
  // speed limit held, the brake; where it is cut, a tick for each piece; a short one peaking below the limit.
  import { duration, playing, position } from '../remote.svelte';
  import type { Wind } from '../startStop';
  import { clockText } from '../remote.svelte';
  import { rate, tapPlan } from '../windShape';

  let { wind }: { wind: Wind } = $props();

  const W = 320, H = 96, L = 10, R = 10, T = 20, B = 22;
  const from = $derived(playing() && duration() ? Math.max(1, position()) : 200);
  const plan = $derived(tapPlan(wind, from));
  const xs = (t: number) => L + (t / plan.D) * (W - L - R);
  const ys = (h: number) => T + (1 - h) * (H - T - B);
  const curve = $derived.by(() => {
    const n = 120;
    let d = '';
    for (let i = 0; i <= n; i++) {
      const t = (plan.D * i) / n;
      d += `${i ? 'L' : 'M'}${xs(t).toFixed(1)} ${ys(rate(wind, plan.D, t)).toFixed(1)}`;
    }
    return d;
  });
  const ticks = $derived(
    plan.cut && plan.pieces > 1
      ? Array.from({ length: plan.pieces - 1 }, (_, i) => xs(wind.attackMs + ((plan.D - wind.attackMs - wind.releaseMs) * (i + 1)) / plan.pieces))
      : [],
  );
  const secs = (ms: number) => `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s`;
</script>

<svg class="graph" viewBox="0 0 {W} {H}" role="img" aria-label="A rewind from {clockText(from)} takes {secs(plan.D)}">
  <text class="head" x={L} y="12">From {clockText(from)}: <tspan class="strong">{secs(plan.D)}</tspan></text>
  <text class="note" x={W - R} y="12" text-anchor="end">{plan.cut ? `${plan.pieces} ${plan.pieces === 1 ? 'piece' : 'pieces'}` : 'no cuts'}</text>
  <line class="base" x1={L} y1={ys(0)} x2={W - R} y2={ys(0)} />
  <line class="limit" x1={L} y1={ys(1)} x2={W - R} y2={ys(1)} />
  <text class="note" x={W - R} y={ys(1) - 4} text-anchor="end">limit {wind.speed}×</text>
  <path class="area" d="{curve}L{xs(plan.D)} {ys(0)}L{xs(0)} {ys(0)}Z" />
  <path class="line" d={curve} />
  {#each ticks as x, i (i)}<line class="tick" x1={x} y1={ys(0) - 5} x2={x} y2={ys(0) + 1} />{/each}
  <text class="note" x={xs(Math.min(wind.attackMs, plan.D / 2) / 2)} y={H - 6} text-anchor="middle">spin-up</text>
  <text class="note" x={xs(plan.D - Math.min(wind.releaseMs, plan.D / 2) / 2)} y={H - 6} text-anchor="middle">brake</text>
</svg>

<style>
  .graph {
    display: block;
    width: 100%;
    height: auto;
    margin: 4px 0 8px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--text) 4%, transparent);
  }

  .head {
    fill: var(--text);
    font-size: 11px;
  }

  .strong {
    font-weight: 600;
  }

  .note {
    fill: var(--muted);
    font-size: 9.5px;
  }

  .base {
    stroke: var(--border);
  }

  .limit {
    stroke: var(--muted);
    stroke-dasharray: 2 3;
    opacity: 0.6;
  }

  .area {
    fill: var(--accent);
    fill-opacity: 0.18;
  }

  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linejoin: round;
  }

  .tick {
    stroke: var(--muted);
  }
</style>
