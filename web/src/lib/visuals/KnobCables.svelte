<script lang="ts" generics="C extends { id: string; value: RouteValue; mode?: string }">
  // What moves a knob (decision 0056): its cables, routes and extras alike, a row each: where it comes from, its curve
  // drawn small, how much at most. A tap opens it (CableSheet), a long press offers to delete or reroute it (CableMenu);
  // "+ Add" picks a source (SourcePicker).
  import { longpress } from '../longpress';
  import { curveOf, endOf } from './curves';
  import type { RouteValue } from './curves';

  let {
    cables,
    label,
    open = null,
    onopen,
    onlongpress,
    onadd,
  }: {
    /** A v1 cable or a v2 one (no mode: cables only add). */
    cables: C[];
    /** A cable's name in its row (its source: "Beat", "Level · Bass", a modulator's). */
    label: (c: C) => string;
    /** The cable open now, marked. */
    open?: string | null;
    onopen: (id: string) => void;
    /** Held (or right-clicked): its menu (CableMenu). */
    onlongpress: (id: string) => void;
    onadd: () => void;
  } = $props();

  const W = 36;
  const H = 18;
  /** A curve as a glyph: across what comes in, up what it gives (the zero line through the middle). */
  function glyph(c: C): string {
    const k = curveOf(c.value);
    const x = (v: number) => (v * W).toFixed(1);
    const y = (v: number) => (H / 2 - v * (H / 2 - 1)).toFixed(1);
    return `M 0 ${y(0)} C ${x(k.h1[0])} ${y(k.h1[1])}, ${x(k.h2[0])} ${y(k.h2[1])}, ${W} ${y(k.h)}`;
  }
  const amount = (c: C) => {
    const v = Math.round(endOf(c.value) * 100);
    return `${v > 0 ? '+' : v < 0 ? '−' : ''}${Math.abs(v)}%`;
  };
</script>

<div class="cables">
  {#each cables as c (c.id)}
    <button class="cable" class:open={open === c.id} use:longpress={() => onlongpress(c.id)} onclick={() => onopen(c.id)}>
      <span class="name">{label(c)}</span>
      {#if c.mode === 'level'}<span class="mode">Level</span>{/if}
      <svg class="glyph" width={W} height={H} viewBox="0 0 {W} {H}" aria-hidden="true">
        <line x1="0" x2={W} y1={H / 2} y2={H / 2} />
        <path d={glyph(c)} class:down={endOf(c.value) < 0} />
      </svg>
      <output>{amount(c)}</output>
    </button>
  {/each}
  <button class="cable add" onclick={onadd}><span class="name">+ Add</span></button>
</div>

<style>
  .cables {
    display: grid;
    gap: 2px;
  }

  .cable {
    display: grid;
    grid-template-columns: 1fr auto auto 3.4em;
    align-items: center;
    gap: 10px;
    min-height: 38px;
    padding: 0 8px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 0.92rem;
    text-align: left;
    cursor: pointer;
    -webkit-touch-callout: none;
    user-select: none;
  }

  .cable:hover,
  .cable:focus-visible,
  .cable.open {
    background: rgb(255 255 255 / 0.08);
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mode {
    font-size: 0.75rem;
    color: rgb(255 255 255 / 0.5);
  }

  .glyph line {
    stroke: rgb(255 255 255 / 0.15);
  }

  .glyph path {
    fill: none;
    stroke: #3fd18f;
    stroke-width: 1.6;
  }

  .glyph path.down {
    stroke: #ff7eb6;
  }

  output {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: rgb(255 255 255 / 0.75);
  }

  .add .name {
    color: var(--accent-ink);
  }
</style>
