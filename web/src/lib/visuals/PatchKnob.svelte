<script lang="ts">
  // A knob on a patch graph's node (decision 0056): its value as an arc over its range, and as a ring outside it the
  // stretch its cables move it over (purple). Turned by dragging up or down (a whole range over 160 px), the wheel or
  // the arrow keys; a step knob by whole steps. Tapped without turning, `ontap` (what moves it). Drawn at true size; the
  // cable handle is the node's, under it.
  import { turned, type KnobView } from './patchgraph';

  let { knob, size = 26, onturn, ontap }: { knob: KnobView; size?: number; onturn: (value: number) => void; ontap?: () => void } = $props();

  const A0 = 0.75 * Math.PI;
  const SWEEP = 1.5 * Math.PI;
  const c = $derived(size / 2);
  const r = $derived(size / 2 - 3);
  function arc(from: number, to: number, rad: number): string {
    const p = (t: number) => [c + rad * Math.cos(A0 + SWEEP * t), c + rad * Math.sin(A0 + SWEEP * t)];
    const [x1, y1] = p(from);
    const [x2, y2] = p(Math.max(to, from + 1e-4));
    return `M${x1.toFixed(2)},${y1.toFixed(2)} A${rad},${rad} 0 ${SWEEP * (to - from) > Math.PI ? 1 : 0} 1 ${x2.toFixed(2)},${y2.toFixed(2)}`;
  }

  let drag: { y: number; value: number; moved: boolean } | null = $state(null);
  const shown = $derived(knob.kind === 'step' ? String(Math.round(knob.value)) : Math.abs(knob.value) >= 10 ? knob.value.toFixed(0) : knob.value.toFixed(2));

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    drag = { y: e.clientY, value: knob.value, moved: false };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    if (Math.abs(drag.y - e.clientY) > 4) drag.moved = true;
    const v = turned(knob, drag.value, drag.y - e.clientY);
    if (v !== knob.value) onturn(v);
  }
  function up(e: PointerEvent) {
    const tap = drag && !drag.moved && e.type === 'pointerup';
    drag = null;
    if (tap) ontap?.();
  }
  const nudge = (dir: number) => {
    const v = turned(knob, knob.value, 0) + (knob.kind === 'step' ? dir : (dir * (knob.max - knob.min)) / 100);
    const next = Math.min(knob.max, Math.max(knob.min, v));
    if (next !== knob.value) onturn(next);
  };
  function wheel(e: WheelEvent) {
    e.preventDefault();
    nudge(e.deltaY < 0 ? 1 : -1);
  }
  function key(e: KeyboardEvent) {
    const dir = e.key === 'ArrowUp' || e.key === 'ArrowRight' ? 1 : e.key === 'ArrowDown' || e.key === 'ArrowLeft' ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    e.stopPropagation();
    nudge(dir);
  }
</script>

<div
  class="knob nodrag nopan nowheel"
  class:turning={drag}
  role="slider"
  tabindex="0"
  aria-label={knob.label}
  aria-valuemin={knob.min}
  aria-valuemax={knob.max}
  aria-valuenow={knob.value}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onwheel={wheel}
  onkeydown={key}
>
  <svg width={size} height={size} viewBox="0 0 {size} {size}">
    <path d={arc(0, 1, r - 2)} class="track" />
    {#if knob.at > 0}<path d={arc(0, knob.at, r - 2)} class="value" />{/if}
    {#if knob.ring}<path d={arc(knob.ring[0], knob.ring[1], r + 2.5)} class="ring" />{/if}
    <circle cx={c} cy={c} r={r - 6} class="cap" />
  </svg>
  <span>{drag ? shown : knob.label}</span>
</div>

<style>
  .knob {
    display: grid;
    justify-items: center;
    gap: 1px;
    width: 44px;
    cursor: ns-resize;
    touch-action: none;
    outline: none;
  }
  .knob:focus-visible svg {
    border-radius: 50%;
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 50%, transparent);
  }
  svg {
    display: block;
    overflow: visible;
  }
  path {
    fill: none;
    stroke-linecap: round;
  }
  .track {
    stroke: color-mix(in srgb, var(--text) 16%, transparent);
    stroke-width: 3;
  }
  .value {
    stroke: var(--accent);
    stroke-width: 3;
  }
  .ring {
    stroke: var(--patch-mod);
    stroke-width: 2.5;
  }
  .cap {
    fill: var(--border);
  }
  span {
    max-width: 44px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10.5px;
    line-height: 1.2;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .turning span {
    color: var(--text);
  }
</style>
