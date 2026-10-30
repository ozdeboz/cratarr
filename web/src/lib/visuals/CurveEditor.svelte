<script lang="ts">
  // A route's curve, edited (decision 0046, amended for 64.1): across its signal (0..1), up what it gives (−1..1, the
  // zero line through the middle), green above and pink below; its start fixed at (0, 0), two handles shaping it, its
  // end held at the right edge (only up or down). Behind it, dim, where the signal has been over the last half minute,
  // mirrored about the zero line as a sound's envelope is (the owner: "this gives us the best way to tune the curve to
  // the signal"); on it, where the signal is now. Below: whether it walks with the behaviour, its pin, its column's pin.
  // Drawn at the panel's own pixels, so the handles stay round.
  import Icon from '../Icon.svelte';
  import { held } from './held.svelte';
  import { isStraight, straight, withEnd, type Curve } from './curves';

  let {
    curve,
    mixed,
    live,
    density,
    walks,
    pinned,
    onchange,
    onwalks,
    onpin,
  }: {
    curve: Curve;
    /** The bands' curves differ (all six shown: their mean, dashed). */
    mixed: boolean;
    /** Where the signal is now, and what the curve gives there. */
    live: { x: number; y: number } | null;
    /** How often the signal has been at each of the bins across, the most 1 (none: not known yet). */
    density: Float32Array | null;
    /** Whether it walks with the behaviour (v1); with no `onwalks` (v2: walks are Randoms) there is no Walk to set. */
    walks: boolean;
    pinned: boolean;
    /** The curve changed (`done`: the drag let go). */
    onchange: (c: Curve, done: boolean) => void;
    onwalks?: (on: boolean) => void;
    onpin: (on: boolean) => void;
  } = $props();

  const H = 120;
  const PAD = 10;
  let W = $state(300);
  const id = Math.random().toString(36).slice(2, 8);
  const X = (x: number) => PAD + x * (W - 2 * PAD);
  const Y = (y: number) => H / 2 - y * (H / 2 - PAD);
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

  const path = $derived(`M ${X(0)} ${Y(0)} C ${X(curve.h1[0])} ${Y(curve.h1[1])}, ${X(curve.h2[0])} ${Y(curve.h2[1])}, ${X(1)} ${Y(curve.h)}`);
  // The signal's envelope: each bin's share of the most, its half-height at most nine tenths of the graph's half.
  const envelope = $derived.by(() => {
    if (!density || !density.length) return '';
    const n = density.length;
    const half = (i: number) => density[i] * 0.9 * (H / 2 - PAD);
    const at = (i: number) => X((i + 0.5) / n);
    let d = `M ${X(0)} ${Y(0)}`;
    for (let i = 0; i < n; i++) d += ` L ${at(i)} ${Y(0) - half(i)}`;
    d += ` L ${X(1)} ${Y(0)}`;
    for (let i = n - 1; i >= 0; i--) d += ` L ${at(i)} ${Y(0) + half(i)}`;
    return d + ' Z';
  });

  // Dragging a handle: `h1`, `h2` anywhere within the graph, `end` only up or down, the curve's shape scaled with it
  // from where the drag began (through 0 and back it is kept).
  let dragging = $state<'h1' | 'h2' | 'end' | null>(null);
  let from: Curve | null = null;
  let svg: SVGSVGElement | undefined = $state();
  function moved(e: PointerEvent, done = false) {
    if (!dragging || !svg) return;
    const box = svg.getBoundingClientRect();
    const x = clamp((e.clientX - box.left - PAD) / (W - 2 * PAD), 0, 1);
    const y = clamp((H / 2 - (e.clientY - box.top)) / (H / 2 - PAD), -1, 1);
    const at: [number, number] = [Math.round(x * 100) / 100, Math.round(y * 100) / 100];
    // A bent curve that ended at 0 has no scale to keep: its end alone moves.
    const was = from ?? curve;
    const ended = isStraight(was) || Math.abs(was.h) > 1e-6 ? withEnd(was, at[1]) : { ...was, h: at[1] };
    onchange(dragging === 'end' ? ended : dragging === 'h1' ? { ...curve, h1: at } : { ...curve, h2: at }, done);
  }
  function grab(which: 'h1' | 'h2' | 'end', e: PointerEvent) {
    e.stopPropagation();
    dragging = which;
    from = curve;
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  }
  function letGo(e: PointerEvent) {
    e.stopPropagation();
    if (!dragging) return;
    moved(e, true);
    dragging = null;
  }
</script>

<div class="editor" bind:clientWidth={W}>
  <svg bind:this={svg} width={W} height={H} viewBox="0 0 {W} {H}" role="img" aria-label="Curve">
    <defs>
      <clipPath id="above-{id}"><rect x="0" y="0" width={W} height={H / 2} /></clipPath>
      <clipPath id="below-{id}"><rect x="0" y={H / 2} width={W} height={H / 2} /></clipPath>
    </defs>
    <rect class="frame" x={PAD} y={PAD} width={W - 2 * PAD} height={H - 2 * PAD} rx="4" />
    {#if envelope}<path class="envelope" d={envelope} />{/if}
    <line class="zero" x1={X(0)} y1={Y(0)} x2={X(1)} y2={Y(0)} />
    <line class="arm" x1={X(0)} y1={Y(0)} x2={X(curve.h1[0])} y2={Y(curve.h1[1])} />
    <line class="arm" x1={X(1)} y1={Y(curve.h)} x2={X(curve.h2[0])} y2={Y(curve.h2[1])} />
    <path class="curve up" class:mixed d={path} clip-path="url(#above-{id})" />
    <path class="curve down" class:mixed d={path} clip-path="url(#below-{id})" />
    <circle class="start" cx={X(0)} cy={Y(0)} r="3" />
    {#if live}<circle class="live" cx={X(live.x)} cy={Y(live.y)} r="4.5" />{/if}
    {#each [['h1', curve.h1[0], curve.h1[1]], ['h2', curve.h2[0], curve.h2[1]], ['end', 1, curve.h]] as const as [which, x, y] (which)}
      <circle
        class="handle"
        class:end={which === 'end'}
        class:on={dragging === which}
        cx={X(x)}
        cy={Y(y)}
        r="7"
        role="slider"
        aria-label={which === 'end' ? 'End' : 'Handle'}
        aria-valuenow={Math.round(y * 100)}
        tabindex="-1"
        onpointerdown={(e) => grab(which, e)}
        onpointermove={(e) => moved(e)}
        onpointerup={letGo}
        onpointercancel={letGo}
      />
    {/each}
  </svg>
  <div class="controls">
    <output>{curve.h > 0 ? '+' : ''}{Math.round(curve.h * 100)}%</output>
    <button class="chip" use:held={'route:zero'} disabled={curve.h === 0 && isStraight(curve) && !mixed} onclick={() => onchange(straight(0), true)}>0</button>
    {#if onwalks}<button class="chip" class:on={walks} aria-pressed={walks} use:held={'route:walks'} onclick={() => onwalks(!walks)}>Walk</button>{/if}
    <button class="pin" class:on={pinned} aria-pressed={pinned} aria-label={pinned ? 'Unpin route' : 'Pin route'} use:held={'route:pin'} onclick={() => onpin(!pinned)}
      ><Icon name="pin" size={16} /></button
    >
  </div>
</div>

<style>
  .editor {
    display: grid;
    gap: 6px;
    min-width: 0;
    touch-action: none;
  }

  svg {
    display: block;
    overflow: visible;
  }

  .frame {
    fill: rgb(255 255 255 / 0.03);
    stroke: rgb(255 255 255 / 0.12);
  }

  .envelope {
    fill: rgb(255 255 255 / 0.08);
  }

  .zero {
    stroke: rgb(255 255 255 / 0.25);
  }

  .arm {
    stroke: rgb(255 255 255 / 0.3);
    stroke-dasharray: 3 3;
  }

  .curve {
    fill: none;
    stroke-width: 2.5;
    stroke-linecap: round;
  }

  .curve.mixed {
    stroke-dasharray: 6 4;
  }

  .curve.up {
    stroke: var(--accent);
  }

  .curve.down {
    stroke: #e8708f;
  }

  .start {
    fill: rgb(255 255 255 / 0.6);
  }

  .live {
    fill: #fff;
    pointer-events: none;
  }

  .handle {
    fill: rgb(20 20 24);
    stroke: #fff;
    stroke-width: 2;
    cursor: grab;
  }

  .handle.end {
    cursor: ns-resize;
  }

  .handle.on {
    fill: #fff;
    cursor: grabbing;
  }

  .controls {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .controls output {
    min-width: 3.5em;
    font-variant-numeric: tabular-nums;
  }

  .controls .chip {
    min-height: 30px;
    padding: 0 12px;
    border: 1px solid rgb(255 255 255 / 0.3);
    border-radius: 999px;
    background: none;
    color: #fff;
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .controls .chip.on {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 30%, transparent);
  }

  .controls .pin {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    min-height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: 8px;
    background: rgb(255 255 255 / 0.06);
    color: rgb(255 255 255 / 0.55);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }

  .controls .pin.on {
    background: rgb(255 255 255 / 0.16);
    color: #fff;
  }
</style>
