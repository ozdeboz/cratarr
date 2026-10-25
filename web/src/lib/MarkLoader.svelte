<script lang="ts">
  // Cratarr's loading sign: the mark in motion (lib/markMotion.ts, the owner's tuning). It opens with the Reveal, plays
  // loop variations while `loading`, and once that is false ends with the Vanish after the playing variation, then
  // calls `ondone`. Reduced motion: the still mark. `size` is its diameter.
  import { reducedMotion } from './motion';
  import { untrack } from 'svelte';
  import { CENTRE, GAP, LINE_EDGE_IN, LINE_EDGE_OUT, NOTHING, VARIANTS, WHOLE, paths, shuffleBag, type Paths, type VariantName } from './markMotion';

  let { size = 48, loading = true, ondone }: { size?: number; loading?: boolean; ondone?: () => void } = $props();

  // The system's reduced motion, or Animations off on this device (lib/motion).
  const reduced = typeof matchMedia !== 'undefined' && reducedMotion();
  let frame = $state<Paths>(reduced ? WHOLE : NOTHING);
  // The lines show only between two edges (one per mark on the page, so the mask's id is its own).
  const edge = `mark-edge-${Math.random().toString(36).slice(2)}`;

  // Read by the running loop without restarting it (set from `loading` right away).
  let still = false;
  $effect(() => {
    still = !loading;
    if (reduced && still) ondone?.();
  });

  $effect(() => {
    if (reduced) return;
    const next = shuffleBag();
    let phase: 'start' | 'loop' | 'done' = 'start';
    let cur: VariantName | null = null;
    let t0 = 0;
    let restUntil = 0;
    let raf = 0;
    const tick = (now: number) => {
      if (cur) {
        const v = VARIANTS[cur];
        const t = now - t0;
        if (t < v.T) frame = paths(v.at(t));
        else {
          frame = paths(v.at(v.T));
          cur = null;
          restUntil = now + GAP;
          if (phase === 'done') return untrack(() => ondone?.());
        }
      } else if (now >= restUntil) {
        if (phase === 'start') [cur, phase] = ['reveal', 'loop'];
        else if (still) [cur, phase] = ['vanish', 'done'];
        else cur = next();
        t0 = now;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });
</script>

<svg class="loader" width={size} height={size} viewBox="30 30 48 48" role="img" aria-label="Loading">
  <defs>
    <mask id={edge}>
      <circle cx={CENTRE.x} cy={CENTRE.y} r={LINE_EDGE_OUT} fill="#fff" />
      <circle cx={CENTRE.x} cy={CENTRE.y} r={LINE_EDGE_IN} fill="#000" />
    </mask>
  </defs>
  {#each frame.rings as d, i (i)}<path {d} fill-rule="evenodd" />{/each}
  <g mask="url(#{edge})">
    {#each frame.lines as [d, o], i (i)}<path {d} opacity={o} />{/each}
  </g>
</svg>

<style>
  .loader {
    display: block;
    fill: var(--accent);
  }
</style>
