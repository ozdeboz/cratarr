<script lang="ts">
  // The seekbar's waveform (decision 0035): the track's loudness along it as bars, what has played in the accent
  // (`progress`, 0..1), the rest muted. As many bars as fit at 3 px each. The seek bar around it takes the pointer.
  // `now` (0..1): where it plays while the handle is held elsewhere, a thin line (the owner, 2026-10-24).
  // While it loads (`bins` null; the owner, 2026-10-26: no pop from line to waveform): the plain bar's line, which gets
  // the bars' gaps from the left as far as it has loaded (`loading`, 0..1: a progress bar of its own), each piece where
  // its bar will stand and as wide; once in, the pieces grow up and down into the bars.
  // Each layer is one path (the owner, 2026-10-26: hundreds of shapes, each animated, ran poorly), redrawn per frame
  // only while the bars grow.
  import { untrack } from 'svelte';
  import { reducedMotion } from './motion';
  import { bars } from './waveforms.svelte';

  let {
    bins,
    progress,
    paused = false,
    now = null,
    loading = null,
  }: { bins: Uint8Array | null; progress: number; paused?: boolean; now?: number | null; loading?: number | null } = $props();

  const GROW_MS = 320;
  let width = $state(0);
  let height = $state(28);
  const n = $derived(Math.max(16, Math.min(512, Math.floor(width / 3))));
  const shown = $derived(bins ? bars(bins, n) : null);
  const done = $derived(Math.min(1, Math.max(0, progress)));
  /** The plain bar's line (4 px), in the drawing's units. */
  const line = $derived((100 * 4) / Math.max(4, height));
  /** How many pieces have their gaps. */
  const cut = $derived(shown ? n : Math.round(Math.min(1, Math.max(0, loading ?? 0)) * n));
  // How far the pieces have grown into bars, 0..1: at once where it was in from the start; from the line where it
  // loaded in sight.
  let grown = $state(untrack(() => (bins ? 1 : 0)));
  $effect(() => {
    if (!bins) return void (grown = 0);
    if (untrack(() => grown) >= 1) return;
    if (reducedMotion()) return void (grown = 1);
    const start = performance.now();
    let r = 0;
    const step = (t: number) => {
      grown = Math.min(1, (t - start) / GROW_MS);
      if (grown < 1) r = requestAnimationFrame(step);
    };
    r = requestAnimationFrame(step);
    return () => cancelAnimationFrame(r);
  });
  const f = (x: number) => Math.round(x * 100) / 100;
  /** The bars (or the pieces of the line) as one path, the line not yet cut after them. */
  const d = $derived.by(() => {
    const e = 1 - (1 - grown) ** 2;
    let s = '';
    for (let i = 0; i < cut; i++) {
      const h = shown ? line + (Math.max(6, shown[i] * 100) - line) * e : line;
      s += `M${f(i + 0.15)} ${f((100 - h) / 2)}h0.7v${f(h)}h-0.7z`;
    }
    if (cut < n) s += `M${cut} ${f((100 - line) / 2)}h${n - cut}v${f(line)}h${cut - n}z`;
    return s;
  });
</script>

<div class="wave" class:paused bind:clientWidth={width} bind:clientHeight={height} aria-hidden="true">
  {#each [false, true] as played (played)}
    <svg class:played viewBox="0 0 {n} 100" preserveAspectRatio="none" style:clip-path={played ? `inset(0 ${(1 - done) * 100}% 0 0)` : undefined}>
      <path {d} />
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
