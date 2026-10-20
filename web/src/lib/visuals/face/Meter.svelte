<script lang="ts">
  // A Meter (face.ts; after NexusUI's, MIT: a bar in the accent colour on the panel's fill): a level across, its peak
  // held a second then falling (as Surge XT's and Vital's meters do), a threshold as a line; a reduction (gain taken
  // away, 1 none) fills from the right; several channels, a short bar each, side by side. `t`: the frame's time (s),
  // for the peak's hold.
  import { untrack } from 'svelte';
  import { meterFill, peakStep, type Peak } from './face';

  let {
    value = null,
    values,
    max = 1,
    peak = false,
    reduction = false,
    threshold = null,
    label,
    t = 0,
    width = 130,
  }: {
    value?: number | null;
    /** Several channels (a Merge's in): a bar each. */
    values?: (number | null)[];
    max?: number;
    peak?: boolean;
    reduction?: boolean;
    threshold?: number | null;
    label?: string;
    t?: number;
    width?: number;
  } = $props();

  const fill = $derived(meterFill(value, max, reduction));
  let held = $state.raw<Peak | null>(null);
  $effect(() => {
    const v = value;
    const now = t;
    if (peak && v !== null) held = untrack(() => peakStep(held, meterFill(v, max), now));
  });
  const LABEL_W = $derived(label ? 12 : 0);
  const W = $derived(width - LABEL_W);
</script>

{#if values}
  <svg class="meter" {width} height="22" viewBox="0 0 {width} 22" aria-hidden="true">
    {#each values as v, c (c)}
      {@const bw = (width - (values.length - 1) * 3) / values.length}
      <rect x={c * (bw + 3)} y="0" width={bw} height="22" rx="2" class="well" />
      {#if v !== null}<rect x={c * (bw + 3)} y={22 - meterFill(v, max) * 22} width={bw} height={meterFill(v, max) * 22} rx="2" class="bar" />{/if}
    {/each}
  </svg>
{:else}
  <svg class="meter" {width} height="10" viewBox="0 0 {width} 10" aria-hidden="true">
    {#if label}<text x="0" y="8.5" class="label">{label}</text>{/if}
    <rect x={LABEL_W} y="2" width={W} height="6" rx="3" class="well" />
    {#if reduction}
      <rect x={LABEL_W + W * (1 - fill)} y="2" width={W * fill} height="6" rx="3" class="bar cut" />
    {:else}
      <rect x={LABEL_W} y="2" width={W * fill} height="6" rx="3" class="bar" />
    {/if}
    {#if peak && held && held.value > 0.001}<rect x={LABEL_W + Math.min(W - 2, W * held.value)} y="1" width="2" height="8" class="peak" />{/if}
    {#if threshold !== null}<line x1={LABEL_W + W * meterFill(threshold, max)} x2={LABEL_W + W * meterFill(threshold, max)} y1="0" y2="10" class="threshold" />{/if}
  </svg>
{/if}

<style>
  .meter {
    display: block;
    overflow: visible;
  }
  .well {
    fill: color-mix(in srgb, var(--meter, var(--patch-mod, var(--accent))) 16%, transparent);
  }
  .bar {
    fill: var(--meter, var(--patch-mod, var(--accent)));
  }
  .bar.cut {
    fill: color-mix(in srgb, var(--danger, #e5484d) 75%, var(--meter, var(--accent)));
  }
  .peak {
    fill: var(--text);
  }
  .threshold {
    stroke: var(--text);
    stroke-width: 1;
    stroke-dasharray: 2 2;
  }
  .label {
    fill: var(--muted);
    font-size: 9px;
  }
</style>
