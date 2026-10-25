<script lang="ts">
  // The progress ring (the pill's page work, a download on a cover; the owner, 2026-10-25: "our standard progress
  // indicator"): the accent's arc over a faint track, centred on what holds it, which is positioned. No progress: the
  // whole ring.
  let { progress, size, onAccent = false }: { progress: number | null; size: number; onAccent?: boolean } = $props();
</script>

<svg class="ring" class:on-accent={onAccent} viewBox="0 0 36 36" aria-hidden="true" style:width="{size}px" style:height="{size}px"
  >{#if progress == null}<circle cx="18" cy="18" r="16" />{:else}<circle class="bg" cx="18" cy="18" r="16" /><circle
      cx="18"
      cy="18"
      r="16"
      pathLength="100"
      stroke-dasharray="{Math.round(Math.min(1, Math.max(0, progress)) * 100)} 100"
    />{/if}</svg
>

<style>
  .ring {
    position: absolute;
    top: 50%;
    left: 50%;
    fill: none;
    stroke: var(--accent);
    stroke-width: 2.5;
    stroke-linecap: round;
    transform: translate(-50%, -50%) rotate(-90deg);
    pointer-events: none;
  }

  .ring .bg {
    stroke: color-mix(in srgb, var(--accent) 25%, transparent);
  }

  /* Drawn on the accent (a cover's status): in the accent's text colour. */
  .ring.on-accent {
    stroke: var(--accent-text);
  }

  .ring.on-accent .bg {
    stroke: color-mix(in srgb, var(--accent-text) 30%, transparent);
  }
</style>
