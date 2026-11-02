<script lang="ts">
  // A snippet's mark (the owner, 2026-11-02): a play in a fading circle (Lucide's circle-fading-plus, its play in place
  // of the plus), so a row's button says it plays a part of the track, not the track. Its circle is its progress: as
  // the snippet is heard, the circle fills in from the top, whole when it ends; open, its pause in the middle (no ring
  // of its own besides: "It is precisely this circle that should feature the progress").
  let { open = false, share = 0, size = 32 }: { open?: boolean; share?: number; size?: number } = $props();
  /** The circle's length (r 10): the heard part drawn along it. */
  const ROUND = 2 * Math.PI * 10;
</script>

<svg class="mark" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
  <g class="fading">
    <path d="M12 2a10 10 0 0 1 7.38 16.75" />
    <path d="M2.5 8.875a10 10 0 0 0-.5 3" />
    <path d="M2.83 16a10 10 0 0 0 2.43 3.4" />
    <path d="M4.636 5.235a10 10 0 0 1 .891-.857" />
    <path d="M8.644 21.42a10 10 0 0 0 7.631-.38" />
  </g>
  {#if open && share > 0}<circle class="heard" cx="12" cy="12" r="10" stroke-dasharray="{share * ROUND} {ROUND}" transform="rotate(-90 12 12)" />{/if}
  {#if open}<rect x="8.5" y="8" width="2.5" height="8" rx="0.6" /><rect x="13" y="8" width="2.5" height="8" rx="0.6" />{:else}<path class="play" d="M10 8.5v7l6-3.5z" />{/if}
</svg>

<style>
  .mark {
    flex: none;
    overflow: visible;
  }

  .fading path,
  .heard,
  .play {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  /* Heard: the circle drawn whole in the accent as far as the snippet has sounded, over its fading arcs. */
  .heard {
    stroke: var(--art-accent, var(--accent));
  }

  rect {
    fill: currentColor;
  }
</style>
