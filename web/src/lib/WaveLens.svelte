<script lang="ts">
  // The waveform lens (the owner's idea, 2026-10-19: "It's awesome. We build it like this."): while the seek bar is held
  // over a waveform, a round lens instead of the dot, lifted clear of the finger. It shows the waveform under it three
  // times as finely (the same bins, a bar per 3 px, drawn as lib/Waveform draws: 0.7 of the slot wide, at least 6%
  // tall, against the track's loudest), played in the accent left of the point, muted right; a white line inside marks
  // the point. Its centre keeps 10 px inside the player; a stem runs from the point up to it, a white line crosses the
  // waveform at the point, and the time where it will land sits above it in a small tag. Geometry as the mock
  // (lens-real.png, gen5.py): 130 px, its bottom edge 36 px above the waveform's middle. `now` (0..1): where it plays,
  // a line in the lens's text colour when it is within it (the owner, 2026-10-24: something to aim for). What it shows is
  // always centred on the point, the lens kept inside the player or not (the owner, 2026-10-26: the point never leaves
  // its centre; at the ends the waveform stops short of its edge). `zoom`: how much finer than the waveform, by the finger
  // (lib/seekState, the owner 2026-10-28); the same bars across it always, the stretch under it remapped onto them (as
  // the waveform's hold zoom, lib/Waveform), so nothing pops.
  import { barsIn } from './waveforms.svelte';

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
    zoom = 3,
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
    zoom?: number;
  } = $props();

  const D = 130;
  const INNER = 0.78 * D;
  const LIFT = D / 2 + 36;
  /** The bars across the lens: one per 3 px of it. */
  const N = Math.round(D / 3);
  /** From the point to the lens's edge, along the track (0..1). */
  const half = $derived(D / 2 / Math.max(1, width * zoom));
  /** The stretch under the lens on its bars; where each bar is in the track (none drawn past an end). */
  const fine = $derived(barsIn(bins, N, point - half, point + half));
  const placeOf = (i: number) => point - half + ((i + 0.5) / N) * 2 * half;
  const x = $derived(point * width);
  const c = $derived(Math.min(Math.max(x, min), Math.max(min, max)));
  const top = $derived(middle - LIFT - D / 2);
</script>

<span class="pin" style:left="{x - 1}px" style:top="{middle - waveHeight / 2}px" style:height="{waveHeight}px" aria-hidden="true"></span>
<span class="stem" style:left="{x - 1}px" style:top="{top + D - 2}px" style:height="{LIFT - D / 2 + 2}px" aria-hidden="true"></span>
<span class="lens" class:paused style:left="{c - D / 2}px" style:top="{top}px" aria-hidden="true">
  <span class="inner" style:left="0px" style:top="{(D - INNER) / 2}px" style:width="{D}px" style:height="{INNER}px">
    {#each [false, true] as played (played)}
      <svg class:played viewBox="0 0 {N} 100" preserveAspectRatio="none" style:clip-path={played ? 'inset(0 50% 0 0)' : undefined}>
        {#each fine as v, i (i)}
          {@const at = placeOf(i)}
          {#if at >= 0 && at <= 1}
            {@const h = Math.max(6, v * 100)}
            <rect x={i + 0.15} y={(100 - h) / 2} width="0.7" height={h} rx="0.35" />
          {/if}
        {/each}
      </svg>
    {/each}
  </span>
  <span class="mark" style:left="{D / 2 - 1}px"></span>
  {#if now !== null && Math.abs((now * width - x) * zoom) < D / 2 - 4}<span class="mark now" style:left="{D / 2 + (now * width - x) * zoom - 1}px"></span>{/if}
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
