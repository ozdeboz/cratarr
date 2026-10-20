<script lang="ts">
  // A Scope (face.ts; after NexusUI's oscilloscope, MIT: one line across, its stroke `height / 100 + 2`, a centre line
  // while nothing comes): a value's last two seconds, the newest at the right; a threshold (a gate's Above) as a dashed
  // line across. Each new reading comes with the frame's time `t` (s).
  import { untrack } from 'svelte';
  import { scopePath, scopePush, scopeRange, scopeStroke, scopeY, type Sample } from './face';

  let { value, t, threshold = null, width = 130, height = 30 }: { value: number | null; t: number; threshold?: number | null; width?: number; height?: number } = $props();

  let seen = $state.raw<Sample[]>([]);
  $effect(() => {
    const now = t;
    const v = value;
    // Its own samples are not what it follows (only the new reading and its time).
    seen = untrack(() => scopePush(seen, v, now));
  });
  const range = $derived(scopeRange(seen, threshold));
  const path = $derived(scopePath(seen, width, height, range, t));
</script>

<svg class="scope" {width} {height} viewBox="0 0 {width} {height}" aria-hidden="true">
  <rect x="0" y="0" {width} {height} rx="4" class="bg" />
  {#if path}
    <path d={path} class="trace" stroke-width={scopeStroke(height)} />
  {:else}
    <path d="M0,{height / 2} H{width}" class="idle" />
  {/if}
  {#if threshold !== null}<line x1="0" x2={width} y1={scopeY(threshold, height, range)} y2={scopeY(threshold, height, range)} class="threshold" />{/if}
</svg>

<style>
  .scope {
    display: block;
  }
  .bg {
    fill: color-mix(in srgb, var(--meter, var(--patch-mod, var(--accent))) 8%, transparent);
  }
  path {
    fill: none;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .trace {
    stroke: var(--meter, var(--patch-mod, var(--accent)));
  }
  .idle {
    stroke: color-mix(in srgb, var(--meter, var(--patch-mod, var(--accent))) 35%, transparent);
    stroke-width: 1;
  }
  .threshold {
    stroke: var(--text);
    stroke-width: 1;
    stroke-dasharray: 3 3;
    opacity: 0.7;
  }
</style>
