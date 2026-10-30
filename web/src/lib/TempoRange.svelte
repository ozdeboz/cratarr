<script lang="ts">
  // The tempo selector (decision 0041), as the price filters people know: the spread (lib/Spread) over a slider with two
  // handles in 5-BPM steps, the bands inside lit, and how many tracks the range keeps. The whole scale is no range
  // (`value` null): every track, with or without a tempo. With a range, a track without a tempo is not kept.
  import Spread, { BAND, HI, LO, bandsOf, within } from './Spread.svelte';
  import SpreadScale from './SpreadScale.svelte';

  let { bpms, value = $bindable(null) }: { bpms: readonly (number | null | undefined)[]; value?: [number, number] | null } = $props();

  const from = $derived(value?.[0] ?? LO);
  const to = $derived(value?.[1] ?? HI);
  const kept = $derived(value === null ? bpms.length : bpms.filter((b) => within(b, value)).length);

  function set(f: number, t: number) {
    value = f <= LO && t >= HI ? null : [f, t];
  }
</script>

<div class="tempo">
  <div class="graph"><Spread series={[bandsOf(bpms)]} range={value} axis={false} /></div>
  <div class="handles">
    <input type="range" min={LO} max={HI} step={BAND} value={from} aria-label="Tempo from" oninput={(e) => set(Math.min(+e.currentTarget.value, to - BAND), to)} />
    <input type="range" min={LO} max={HI} step={BAND} value={to} aria-label="Tempo to" oninput={(e) => set(from, Math.max(+e.currentTarget.value, from + BAND))} />
  </div>
  <div class="graph"><SpreadScale /></div>
  <p class="sum">{value ? `${from}–${to} BPM` : 'Any tempo'} · {kept} {kept === 1 ? 'track' : 'tracks'}</p>
</div>

<style>
  /* As wide as the spread gets (lib/Spread) and the handles' overhang: the handles stay over its bands. */
  .tempo {
    --thumb: 22px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: calc(480px + var(--thumb));
  }

  /* The bands' edges under the handles' centres: the spread is inset by half a handle on each side. */
  .graph {
    padding: 0 calc(var(--thumb) / 2);
  }

  /* Two sliders on one track: only their handles take the pointer. */
  .handles {
    position: relative;
    height: var(--thumb);
  }

  .handles::before {
    content: '';
    position: absolute;
    inset: calc(50% - 1px) calc(var(--thumb) / 2);
    border-radius: 1px;
    background: var(--border);
  }

  input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    background: none;
    pointer-events: none;
    appearance: none;
    -webkit-appearance: none;
  }

  input::-webkit-slider-thumb {
    width: var(--thumb);
    height: var(--thumb);
    border: 0;
    border-radius: 50%;
    background: var(--accent);
    pointer-events: auto;
    cursor: grab;
    -webkit-appearance: none;
  }

  input::-moz-range-thumb {
    width: var(--thumb);
    height: var(--thumb);
    border: 0;
    border-radius: 50%;
    background: var(--accent);
    pointer-events: auto;
    cursor: grab;
  }

  input:focus-visible::-webkit-slider-thumb {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }

  input:focus-visible::-moz-range-thumb {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }

  .sum {
    margin: 0;
    font-size: 0.85rem;
    color: var(--muted);
  }
</style>
