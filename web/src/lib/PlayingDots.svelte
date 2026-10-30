<script lang="ts">
  // What plays, its one sign (the owner, 2026-10-27: "only one playing indicator"): dots moving up and down in a wave,
  // a wave of brightness running through them with it; still while paused. In the text's colour (what holds it sets
  // it). A list's playing track has it in place of its number, a track's cover and an album's Play over theirs.
  let { still = false, count = 4, small = false }: { still?: boolean; count?: number; small?: boolean } = $props();
</script>

<span class="dots" class:still class:small aria-hidden="true">{#each { length: count } as _, i (i)}<i></i>{/each}</span>

<style>
  .dots {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .dots i {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: currentColor;
    animation: dot 1.1s ease-in-out infinite;
  }

  .small {
    gap: 2px;
  }

  .small i {
    width: 3px;
    height: 3px;
  }

  .dots i:nth-child(2) {
    animation-delay: 0.15s;
  }

  .dots i:nth-child(3) {
    animation-delay: 0.3s;
  }

  .dots i:nth-child(4) {
    animation-delay: 0.45s;
  }

  .still i {
    animation: none;
  }

  @keyframes dot {
    0%,
    100% {
      opacity: 0.35;
      transform: translateY(0);
    }
    50% {
      opacity: 1;
      transform: translateY(-3px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dots i {
      animation: none;
    }
  }

  /* Animations off on this device (Display): the same as the system's reduced motion. */
  :global(:root[data-motion='off']) .dots i {
    animation: none;
  }
</style>
