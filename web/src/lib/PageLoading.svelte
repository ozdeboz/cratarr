<script lang="ts">
  // A page's or a panel's whole content loading: the mark in motion (lib/MarkLoader.svelte), in the middle of what can
  // be seen of the page or the component it is in (the window between the bar of pages and the mini-player, or the
  // panel or sheet that scrolls it); lower only where what comes before it reaches that far.
  import MarkLoader from './MarkLoader.svelte';
  import { holdUntil } from './holdPlace.svelte';

  /** `children`: what goes under the mark, where the work can say how far it is (the owner, 2026-10-24: Discover's
   *  first look, its progress under the loader at its usual place). */
  let { size = 96, children }: { size?: number; children?: import('svelte').Snippet } = $props();

  // A page or panel loading as a whole is not ready (lib/Hold): what it showed stays while this is here.
  holdUntil(
    () => 0,
    () => false,
  );

  let el: HTMLDivElement;
  /** Its height (to the bottom of what can be seen) and the mark's offset from its top; null until measured. */
  let fit = $state<{ height: number; top: number } | null>(null);

  /** What can be seen of `el`'s container: the scrolling box it is in, else the window, less the bar of pages along
   *  the top (--top-h) and the mini-player's room (--bottom-h). */
  function seen(): { top: number; bottom: number } {
    const view = window.visualViewport?.height ?? window.innerHeight;
    for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) {
      const y = getComputedStyle(a).overflowY;
      if (y === 'auto' || y === 'scroll') {
        const r = a.getBoundingClientRect();
        return { top: Math.max(r.top, 0), bottom: Math.min(r.bottom, view) };
      }
    }
    const css = getComputedStyle(el);
    const px = (name: string) => parseFloat(css.getPropertyValue(name)) || 0;
    return { top: px('--top-h'), bottom: view - px('--bottom-h') };
  }

  // Measured every frame while it shows (it only shows while loading): what is above it settles meanwhile.
  $effect(() => {
    let raf = 0;
    const measure = () => {
      const { top, bottom } = seen();
      const at = el.getBoundingClientRect().top;
      const middle = Math.max((top + bottom) / 2, at + size / 2);
      const next = { height: Math.round(Math.max(bottom - at, size * 2)), top: Math.round(middle - at - size / 2) };
      if (next.height !== fit?.height || next.top !== fit?.top) fit = next;
      raf = requestAnimationFrame(measure);
    };
    measure();
    return () => cancelAnimationFrame(raf);
  });
</script>

<div
  class="page-loading"
  class:fitted={fit !== null}
  bind:this={el}
  style:height={fit ? `${fit.height}px` : null}
  style:padding-top={fit ? `${fit.top}px` : null}
>
  <MarkLoader {size} />
  {#if children}<div class="under">{@render children()}</div>{/if}
</div>

<style>
  .page-loading {
    display: grid;
    place-items: center;
    min-height: 40vh;
  }

  .under {
    width: min(320px, 100%);
    margin-top: 16px;
    text-align: center;
  }

  /* A progress bar under it (lib/Meter, at most 240 px) centred too, under the mark, not at the box's left (the owner,
     2026-10-24: Discover's first look). */
  .under :global(.track) {
    margin-inline: auto;
  }

  /* Measured: as high as the room left, the mark placed from the top. */
  .page-loading.fitted {
    align-items: start;
    min-height: 0;
  }
</style>
