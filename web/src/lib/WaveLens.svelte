<script lang="ts">
  // The waveform lens (the owner's idea, 2026-10-19: "It's awesome. We build it like this."): while the seek bar is held
  // over a waveform, a round lens instead of the dot, lifted clear of the finger. It shows the waveform under it three
  // times as finely (the same bins, a bar per 3 px, drawn as lib/Waveform draws: 0.7 of the slot wide, at least 6%
  // tall, against the track's loudest), played in the accent left of the point, muted right; a white line inside marks
  // the point. Its centre keeps 10 px inside the player; a stem runs from the point up to it, a white line crosses the
  // waveform at the point, and the time where it will land sits above it in a small tag. Geometry as claude-5f's mock
  // (lens-real.png, gen5.py): 130 px, its bottom edge 36 px above the waveform's middle. `now` (0..1): where it plays,
  // a line in the lens's text colour when it is within it (the owner, 2026-10-24: something to aim for).
  import { bars } from './waveforms.svelte';

  let {
    bins,
    point,
    width,
    middle,
    waveHeight,
    min,
    max,
    time,
    paused = false,
    now = null,
  }: {
    bins: Uint8Array;
    /** 0..1 along the waveform. */
    point: number;
    /** The waveform's width and its middle's height (px, in the seek bar). */
    width: number;
    middle: number;
    waveHeight: number;
    /** Where the lens's centre may go (px along the seek bar): the player's edges, 10 px in. */
    min: number;
    max: number;
    time: string;
    paused?: boolean;
    now?: number | null;
  } = $props();

  const D = 130;
  const ZOOM = 3;
  const INNER = 0.78 * D;
  const LIFT = D / 2 + 36;
  // A bar per 3 px of the waveform drawn three times as wide: a bar per px of the waveform.
  const fine = $derived(bars(bins, Math.max(16, Math.min(bins.length * 4, Math.round(width)))));
  const x = $derived(point * width);
  const c = $derived(Math.min(Math.max(x, min), Math.max(min, max)));
  const top = $derived(middle - LIFT - D / 2);
</script>

<span class="pin" style:left="{x - 1}px" style:top="{middle - waveHeight / 2}px" style:height="{waveHeight}px" aria-hidden="true"></span>
<span class="stem" style:left="{x - 1}px" style:top="{top + D - 2}px" style:height="{LIFT - D / 2 + 2}px" aria-hidden="true"></span>
<span class="lens" class:paused style:left="{c - D / 2}px" style:top="{top}px" aria-hidden="true">
  <span class="inner" style:left="{D / 2 - c * ZOOM}px" style:top="{(D - INNER) / 2}px" style:width="{width * ZOOM}px" style:height="{INNER}px">
    {#each [false, true] as played (played)}
      <svg class:played viewBox="0 0 {fine.length} 100" preserveAspectRatio="none" style:clip-path={played ? `inset(0 ${(1 - point) * 100}% 0 0)` : undefined}>
        {#each fine as v, i (i)}
          {@const h = Math.max(6, v * 100)}
          <rect x={i + 0.15} y={(100 - h) / 2} width="0.7" height={h} rx="0.35" />
        {/each}
      </svg>
    {/each}
  </span>
  <span class="mark" style:left="{D / 2 + (x - c) * ZOOM - 1}px"></span>
  {#if now !== null && Math.abs((now * width - c) * ZOOM) < D / 2 - 4}<span class="mark now" style:left="{D / 2 + (now * width - c) * ZOOM - 1}px"></span>{/if}
</span>
<span class="tag" style:left="{c}px" style:top="{top - 26}px">{time}</span>

<style>
  .pin,
  .stem,
  .lens,
  .tag {
    position: absolute;
    pointer-events: none;
  }

  /* The point on the waveform, and the stem up to the lens. */
  .pin {
    z-index: 2;
    width: 2px;
    border-radius: 1px;
    background: rgb(255 255 255 / 0.85);
  }

  .stem {
    z-index: 2;
    width: 2px;
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 70%, white);
  }

  .lens {
    z-index: 3;
    width: 130px;
    height: 130px;
    box-sizing: border-box;
    overflow: hidden;
    border: 2px solid color-mix(in srgb, var(--art-accent, var(--accent)) 70%, white);
    border-radius: 50%;
    background: var(--surface);
    box-shadow: 0 8px 22px rgb(0 0 0 / 0.55);
  }

  .inner {
    position: absolute;
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

  .mark {
    position: absolute;
    top: 8px;
    bottom: 8px;
    width: 2px;
    border-radius: 1px;
    background: rgb(255 255 255 / 0.9);
  }

  /* Where it plays: dashed, so it reads apart from where it will land. */
  .mark.now {
    background: repeating-linear-gradient(to bottom, rgb(255 255 255 / 0.75) 0 4px, transparent 4px 7px);
  }

  /* Where it will land, over the lens. */
  .tag {
    z-index: 4;
    transform: translateX(-50%);
    padding: 2px 8px;
    border: 1px solid color-mix(in srgb, var(--text) 14%, transparent);
    border-radius: 8px;
    background: rgb(20 23 28 / 0.92);
    color: #fff;
    font-size: 0.75rem;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
