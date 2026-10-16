<script lang="ts">
  // The seekbar's waveform (decision 0035): the track's loudness along it as bars, what has played in the accent
  // (`progress`, 0..1), the rest muted. As many bars as fit at 3 px each. The seek bar around it takes the pointer.
  import { bars } from './waveforms.svelte';

  let { bins, progress, paused = false }: { bins: Uint8Array; progress: number; paused?: boolean } = $props();

  let width = $state(0);
  const shown = $derived(bars(bins, Math.max(16, Math.min(bins.length, Math.floor(width / 3)))));
  const done = $derived(Math.min(1, Math.max(0, progress)));
</script>

<div class="wave" class:paused bind:clientWidth={width} aria-hidden="true">
  {#each [false, true] as played (played)}
    <svg class:played viewBox="0 0 {shown.length} 100" preserveAspectRatio="none" style:clip-path={played ? `inset(0 ${(1 - done) * 100}% 0 0)` : undefined}>
      {#each shown as v, i (i)}
        {@const h = Math.max(6, v * 100)}
        <rect x={i + 0.15} y={(100 - h) / 2} width="0.7" height={h} rx="0.35" />
      {/each}
    </svg>
  {/each}
</div>

<style>
  .wave {
    position: relative;
    flex: 1;
    height: 28px;
  }

  svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    fill: color-mix(in srgb, var(--text) 22%, transparent);
  }

  svg.played {
    fill: var(--art-accent, var(--accent));
  }

  .paused svg.played {
    fill: var(--muted);
  }
</style>
