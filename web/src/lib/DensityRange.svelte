<script lang="ts">
  // A range condition's slider (the owner, 2026-10-23: the profile's bars in the Filter sheet and the crate builder):
  // the axis' density over its span as the bar's colour, the chosen part lit and the rest greyed, two handles. A
  // handle at its end is an open end (null). Dragged by the handle or moved with the arrow keys on it; set on release.
  import { densityGradient } from './DensityBar.svelte';

  let {
    bins,
    min,
    max,
    value,
    step = 1,
    ends,
    label,
    onchange,
  }: {
    bins: readonly number[];
    /** The axis' span in this list. */
    min: number;
    max: number;
    value: { min: number | null; max: number | null };
    step?: number;
    ends: [string, string];
    label: string;
    onchange: (v: { min: number | null; max: number | null }) => void;
  } = $props();

  let track = $state<HTMLElement>();
  /** While a handle is dragged: where both are (values). */
  let drag = $state<{ which: 0 | 1; a: number; b: number } | null>(null);

  const span = $derived(max - min || 1);
  const a = $derived(drag ? drag.a : Math.max(min, Math.min(max, value.min ?? min)));
  const b = $derived(drag ? drag.b : Math.max(min, Math.min(max, value.max ?? max)));
  const at = (x: number) => ((x - min) / span) * 100;
  const snap = (x: number) => Math.max(min, Math.min(max, Math.round(x / step) * step));

  function valueAt(clientX: number) {
    const r = track!.getBoundingClientRect();
    return snap(min + ((clientX - r.left) / r.width) * span);
  }

  /** The handles' values as kept: one at its end is open. */
  const kept = (x: number, y: number) => ({ min: x <= min ? null : x, max: y >= max ? null : y });

  function down(e: PointerEvent, which: 0 | 1) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { which, a, b };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const x = valueAt(e.clientX);
    drag = drag.which === 0 ? { ...drag, a: Math.min(x, drag.b) } : { ...drag, b: Math.max(x, drag.a) };
  }
  function up() {
    if (!drag) return;
    const { a: x, b: y } = drag;
    drag = null;
    onchange(kept(x, y));
  }
  function key(e: KeyboardEvent, which: 0 | 1) {
    const d = e.key === 'ArrowLeft' || e.key === 'ArrowDown' ? -step : e.key === 'ArrowRight' || e.key === 'ArrowUp' ? step : 0;
    if (!d) return;
    e.preventDefault();
    onchange(which === 0 ? kept(Math.min(snap(a + d), b), b) : kept(a, Math.max(snap(b + d), a)));
  }
</script>

<div class="range">
  <div class="track" bind:this={track} style:background-image={densityGradient(bins, [(a - min) / span, (b - min) / span])}>
    <div class="chosen" style:left="{at(a)}%" style:width="{at(b) - at(a)}%"></div>
    {#each [a, b] as x, i (i)}
      <button
        class="knob"
        style:left="{at(x)}%"
        role="slider"
        aria-label={i === 0 ? `${label} from` : `${label} to`}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={x}
        onpointerdown={(e) => down(e, i as 0 | 1)}
        onpointermove={move}
        onpointerup={up}
        onpointercancel={up}
        onkeydown={(e) => key(e, i as 0 | 1)}
      ></button>
    {/each}
  </div>
  <div class="ends"><span>{ends[0]}</span><span>{ends[1]}</span></div>
</div>

<style>
  .range {
    padding: 10px 12px 0;
  }

  .track {
    position: relative;
    height: 12px;
    border-radius: 6px;
    background-color: color-mix(in srgb, var(--text) 6%, transparent);
    touch-action: none;
  }

  .chosen {
    position: absolute;
    top: -4px;
    bottom: -4px;
    border: 1.5px solid color-mix(in srgb, var(--text) 60%, transparent);
    border-radius: 8px;
    pointer-events: none;
  }

  /* 44 px to touch, a 24 px knob to see. */
  .knob {
    position: absolute;
    top: 50%;
    width: 44px;
    height: 44px;
    margin: -22px 0 0 -22px;
    padding: 0;
    border: 0;
    background: none;
    cursor: grab;
    touch-action: none;
  }

  .knob::after {
    content: '';
    position: absolute;
    inset: 10px;
    border-radius: 50%;
    background: var(--text);
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.6);
  }

  .ends {
    display: flex;
    justify-content: space-between;
    margin-top: 6px;
    color: var(--muted);
    font-size: 11.5px;
  }
</style>
