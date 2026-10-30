<script module lang="ts">
  // Covers that failed to load (an album without art answers 404, as does Android's /__device/cover for an album it
  // has none for): every place that shows one shows the stand-in at once, without asking again. Blurred backdrops
  // check it too, and leave themselves out.
  const failed = $state<Record<string, true>>({});
  export const coverBroken = (src: string | null | undefined) => !src || !!failed[src];
  export const coverBroke = (src: string | null | undefined) => {
    if (src) failed[src] = true;
  };
  /** A mosaic's covers as drawn: four while four of them load, else one (the first that loads, or the stand-in), so
   *  failed covers never make a grid of stand-ins. */
  export function mosaicOf(srcs: string[]): string[] {
    const ok = srcs.filter((s) => !coverBroken(s));
    return !srcs.length ? [] : ok.length >= 4 ? ok.slice(0, 4) : [ok[0] ?? srcs[0]];
  }
</script>

<script lang="ts">
  // A cover, or the stand-in where there is none (no address, or the image fails to load): our record mark, faint (the
  // text colour at 28 %, 45 % of the tile), on a tile a step lighter than the page (--tile). The owner's pick, stand-in
  // A (2026-10-19: "if they don't exist, we need a better way to show up than a broken component"), one look
  // everywhere. It fills its box and takes the box's corners, so the place that draws it sizes and rounds the box. A
  // place on another ground (the visuals' sheets) sets --cover-tile and --cover-mark.
  import Mark from './Mark.svelte';

  /** `onready`: its picture in (or failed, its stand-in shown). */
  let { src, lazy = true, onfail, onready }: { src: string | null | undefined; lazy?: boolean; onfail?: (src: string) => void; onready?: () => void } = $props();

  function fail(e: Event) {
    const at = (e.currentTarget as HTMLImageElement).getAttribute('src');
    if (!at) return;
    coverBroke(at);
    onfail?.(at);
    onready?.();
  }
</script>

{#if src && !failed[src]}
  <img class="cover" {src} alt="" loading={lazy ? 'lazy' : 'eager'} decoding="async" draggable="false" onerror={fail} onload={() => onready?.()} />
{:else}
  <!-- Its stand-in is ready at once: one known to fail, or none to load (a flow shows it then, the owner 2026-10-26). -->
  <span class="cover standin" aria-hidden="true" {@attach () => onready?.()}><Mark fill="currentColor" /></span>
{/if}

<style>
  .cover {
    display: block;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    border-radius: inherit;
    object-fit: cover;
  }

  .standin {
    display: grid;
    place-items: center;
    background: var(--cover-tile, var(--tile));
    color: var(--cover-mark, color-mix(in srgb, var(--text) 28%, transparent));
  }

  .standin :global(svg) {
    width: 45%;
    height: 45%;
  }
</style>
