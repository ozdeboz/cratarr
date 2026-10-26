<script lang="ts">
  // The seekbar's waveform (decision 0035): the track's loudness along it as bars, what has played in the accent
  // (`progress`, 0..1), the rest muted. As many bars as fit at 3 px each. The seek bar around it takes the pointer.
  // `now` (0..1): where it plays while the handle is held elsewhere, a thin line (the owner, 2026-10-24).
  // While it loads (`bins` null; the owner, 2026-10-26: no pop from line to waveform): the plain bar's line, which gets
  // the bars' gaps from the left as far as it has loaded (`loading`, 0..1: a progress bar of its own), each piece where
  // its bar will stand and as wide; once in, the pieces grow up and down into the bars.
  import { untrack } from 'svelte';
  import { bars } from './waveforms.svelte';

  let {
    bins,
    progress,
    paused = false,
    now = null,
    loading = null,
  }: { bins: Uint8Array | null; progress: number; paused?: boolean; now?: number | null; loading?: number | null } = $props();

  let width = $state(0);
  let height = $state(28);
  const n = $derived(Math.max(16, Math.min(bins?.length ?? 512, Math.floor(width / 3))));
  const shown = $derived(bins ? bars(bins, n) : null);
  const done = $derived(Math.min(1, Math.max(0, progress)));
  /** The plain bar's line (4 px), in the drawing's units. */
  const line = $derived((100 * 4) / Math.max(4, height));
  /** How many pieces have their gaps. */
  const cut = $derived(shown ? n : Math.round(Math.min(1, Math.max(0, loading ?? 0)) * n));
  // Grown into bars: at once where it was in from the start; a frame after it came in where it loaded in sight.
  let grown = $state(untrack(() => !!bins));
  $effect(() => {
    if (!bins || grown) return;
    let r2 = 0;
    const r1 = requestAnimationFrame(() => (r2 = requestAnimationFrame(() => (grown = true))));
    return () => {
      cancelAnimationFrame(r1);
      cancelAnimationFrame(r2);
    };
  });
</script>

<div class="wave" class:paused bind:clientWidth={width} bind:clientHeight={height} aria-hidden="true">
  {#each [false, true] as played (played)}
    <svg class:played viewBox="0 0 {n} 100" preserveAspectRatio="none" style:clip-path={played ? `inset(0 ${(1 - done) * 100}% 0 0)` : undefined}>
      {#each { length: cut } as _, i (i)}
        {@const h = shown ? Math.max(6, shown[i] * 100) : line}
        <rect class="bar" x={i + 0.15} y={(100 - h) / 2} width="0.7" height={h} rx="0.35" style:transform={grown || !shown ? undefined : `scaleY(${line / h})`} />
      {/each}
      {#if cut < n}<rect x={cut} y={(100 - line) / 2} width={n - cut} height={line} rx="0.35" />{/if}
    </svg>
  {/each}
  {#if now !== null}<span class="now" style:left="{Math.min(1, Math.max(0, now)) * 100}%"></span>{/if}
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

  /* Each piece grows from the line into its bar, from its middle. */
  .bar {
    transform-box: fill-box;
    transform-origin: center;
    transition: transform 320ms ease-out;
  }

  .now {
    position: absolute;
    top: -3px;
    bottom: -3px;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--text);
  }
</style>
