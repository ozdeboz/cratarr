<script lang="ts">
  // A knob on a patch graph's node (decision 0056): its value as an arc over its range, and as a ring outside it the
  // stretch its cables move it over (purple). Turned by dragging up or down (a whole range over 160 px), the wheel or
  // the arrow keys; a step knob by whole steps. Tapped without turning, `ontap` (what moves it). Drawn at true size; the
  // cable handle is the node's, under it. A polyphonic knob (a band's six, `knob.channels`) is one dial at their mean
  // with a tick at each channel's value.
  //
  // Held (a long press; a mouse's right click, lib/longpress) it is fine-adjusted, as the player's waveform lens is
  // scrubbed: sideways sets it, slower the further the finger has gone up (patchgraph.ts fineSpeed, fineStep); the
  // graph shows the lens (`onfine`); let go and it closes. The value goes out through `onturn` as it moves (the store
  // keeps a knob's turns as one step).
  import { longpress } from '../longpress';
  import { fineSpeed, fineStep, knobText, turned, type KnobView } from './patchgraph';

  let {
    knob,
    size = 26,
    onturn,
    ontap,
    onfine,
  }: { knob: KnobView; size?: number; onturn: (value: number) => void; ontap?: () => void; onfine?: (f: { value: number; speed: number; at: { x: number; y: number } } | null) => void } = $props();
  // Where the pointer was last, and the fine adjust under way (its unrounded value, its last x, where it was held).
  let last = { x: 0, y: 0 };
  let fine: { raw: number; x: number; y0: number } | null = null;
  function startFine(at: { x: number; y: number }) {
    if (knob.macro) return;
    fine = { raw: knob.value, x: at.x, y0: at.y };
    onfine?.({ value: knob.value, speed: 1, at: { x: at.x, y: at.y } });
  }
  function moveFine(e: PointerEvent) {
    if (!fine) return;
    const speed = fineSpeed(fine.y0 - e.clientY);
    const next = fineStep(knob, fine.raw, e.clientX - fine.x, speed);
    fine.raw = next.raw;
    fine.x = e.clientX;
    if (next.value !== knob.value) onturn(next.value);
    onfine?.({ value: next.value, speed, at: { x: e.clientX, y: fine.y0 } });
  }
  function endFine() {
    fine = null;
    onfine?.(null);
  }

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
  const shown = $derived(knobText(knob, knob.value));

  function down(e: PointerEvent) {
    last = { x: e.clientX, y: e.clientY };
    // A macro's (0077): it turns with its macro, not by hand.
    if (e.button !== 0 || knob.macro) return;
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    drag = { y: e.clientY, value: knob.value, moved: false };
  }
  function move(e: PointerEvent) {
    last = { x: e.clientX, y: e.clientY };
    if (fine) return moveFine(e);
    if (!drag) return;
    if (Math.abs(drag.y - e.clientY) > 4) drag.moved = true;
    const v = turned(knob, drag.value, drag.y - e.clientY);
    if (v !== knob.value) onturn(v);
  }
  function up(e: PointerEvent) {
    const tap = drag && !drag.moved && e.type === 'pointerup' && !fine;
    drag = null;
    if (fine) endFine();
    if (tap) ontap?.();
  }
  // A mouse's right click (no capture of its own): the window's moves scrub until a click lets go.
  function windowMove(e: PointerEvent) {
    if (fine && !drag) moveFine(e);
  }
  function windowDown() {
    if (fine && !drag) endFine();
  }
  const nudge = (dir: number) => {
    if (knob.macro) return;
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

<svelte:window onpointermove={windowMove} onpointerdown={windowDown} />
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
  oncontextmenu={(e) => e.stopPropagation()}
  use:longpress={(rect) => {
    if (drag?.moved) return;
    startFine(drag ? last : { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 });
  }}
>
  <svg width={size} height={size} viewBox="0 0 {size} {size}">
    <path d={arc(0, 1, r - 2)} class="track" />
    {#if knob.at > 0}<path d={arc(0, knob.at, r - 2)} class="value" />{/if}
    {#if knob.ring}<path d={arc(knob.ring[0], knob.ring[1], r + 2.5)} class="ring" />{/if}
    <!-- Mapped (0077, µMAP's marker): its macro's range round the dial. -->
    {#if knob.macro}<path d={arc(Math.min(...knob.macro.span), Math.max(...knob.macro.span), r + 2.5)} class="mapped"><title>{knob.label} ← {knob.macro.label}</title></path>{/if}
    {#each knob.channels ?? [] as ch (ch.id)}
      {@const a = A0 + SWEEP * ch.at}
      <line x1={c + (r - 4.5) * Math.cos(a)} y1={c + (r - 4.5) * Math.sin(a)} x2={c + (r - 0.5) * Math.cos(a)} y2={c + (r - 0.5) * Math.sin(a)} class="tick" />
    {/each}
    <circle cx={c} cy={c} r={r - 6} class="cap" />
    <!-- Pinned: a pin's head in the ring's gap at the bottom (0057: a pin mark on the knob's ring). -->
    {#if knob.pinned}<circle cx={c} cy={c + r - 1} r="2.6" class="pinmark"><title>Pinned</title></circle>{/if}
  </svg>
  <span>{drag ? shown : knob.label}</span>
</div>

<style>
  .mapped {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linecap: round;
  }

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
  .pinmark {
    fill: var(--accent);
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
  .tick {
    stroke: var(--text);
    stroke-width: 1.2;
    stroke-linecap: round;
    opacity: 0.85;
  }
  .cap {
    fill: var(--border);
  }
  span {
    max-width: 44px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    line-height: 1.2;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .turning span {
    color: var(--text);
  }
</style>
