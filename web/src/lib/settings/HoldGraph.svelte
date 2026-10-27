<script lang="ts">
  // A hold, drawn (Settings › Tape; the owner, 2026-10-27), as winding's graph is (lib/settings/WindGraph): its speed over
  // the first seconds held: spun up to where it starts, then ever faster, doubling, until its top speed.
  import type { HoldWind } from '../startStop';

  let { hold }: { hold: HoldWind } = $props();

  const W = 320, H = 96, L = 10, R = 10, T = 20, B = 22;
  /** How long until it is at its top (ms): the spin-up, then the doublings. */
  const reach = $derived(hold.attackMs + hold.doublingS * 1000 * Math.max(0, Math.log2(hold.top / hold.speed)));
  /** The time drawn: past the top a little. */
  const span = $derived(Math.max(2000, reach * 1.25));
  const speedAt = (t: number) =>
    t < hold.attackMs ? (hold.speed * (1 - Math.cos((Math.PI * t) / hold.attackMs))) / 2 : Math.min(hold.top, hold.speed * 2 ** ((t - hold.attackMs) / (hold.doublingS * 1000)));
  const xs = (t: number) => L + (t / span) * (W - L - R);
  const ys = (v: number) => T + (1 - v / hold.top) * (H - T - B);
  const curve = $derived.by(() => {
    let d = '';
    for (let i = 0; i <= 120; i++) {
      const t = (span * i) / 120;
      d += `${i ? 'L' : 'M'}${xs(t).toFixed(1)} ${ys(speedAt(t)).toFixed(1)}`;
    }
    return d;
  });
  const secs = (ms: number) => `${(ms / 1000).toFixed(1)} s`;
</script>

<svg class="graph" viewBox="0 0 {W} {H}" role="img" aria-label="Held: {hold.speed}× at once, {hold.top}× after {secs(reach)}">
  <text class="head" x={L} y="12">{hold.speed}× → <tspan class="strong">{hold.top}×</tspan> in {secs(reach)}</text>
  <line class="base" x1={L} y1={ys(0)} x2={W - R} y2={ys(0)} />
  <line class="limit" x1={L} y1={ys(hold.top)} x2={W - R} y2={ys(hold.top)} />
  <path class="area" d="{curve}L{xs(span)} {ys(0)}L{xs(0)} {ys(0)}Z" />
  <path class="line" d={curve} />
  <text class="note" x={xs(Math.min(hold.attackMs, span / 2) / 2)} y={H - 6} text-anchor="middle">spin-up</text>
  <text class="note" x={W - R} y={H - 6} text-anchor="end">held</text>
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
</style>
