<script lang="ts">
  // A page's or a panel's whole content loading: the mark in motion (lib/MarkLoader.svelte), in the middle of what can
  // be seen of the page or the component it is in (the window between the bar of pages and the mini-player, or the
  // panel or sheet that scrolls it); lower only where what comes before it reaches that far. Its size is where it is:
  // a page's large, a panel's or a sheet's small, as a panel's swap (lib/Hold) and a sheet's own (the owner,
  // 2026-11-02: the large one in a crate's panel, "we're using smaller kinds these days in smaller panels").
  import { untrack } from 'svelte';
  import MarkLoader from './MarkLoader.svelte';
  import { loaderSizeIn } from './loader';
  import { holdUntil } from './holdPlace.svelte';

  /** `children`: what goes under the mark, where the work can say how far it is (the owner, 2026-10-24: Discover's
   *  first look, its progress under the loader at its usual place). */
  let { children }: { children?: import('svelte').Snippet } = $props();

  /** Its size where it is (lib/loader): known once it is placed (its mark shows from then). */
  let size = $state<number | null>(null);

  // A page or panel loading as a whole is not ready (lib/Hold): what it showed stays while this is here.
  holdUntil(
    () => 0,
    () => false,
  );

  let el: HTMLDivElement;
  /** Its height (to the bottom of what can be seen) and the mark's offset from its top; null until measured. */
  let fit = $state<{ height: number; top: number } | null>(null);

  /** What can be seen of `el`'s container as it opens, scrolled to its top: the scrolling box it is in, else the window
   *  less the bar of pages along the top (--top-h) and the mini-player's room (--bottom-h); and where `el` is in it then. */
  function seen(): { top: number; bottom: number; at: number; box: Element | null } {
    const view = window.visualViewport?.height ?? window.innerHeight;
    const r = el.getBoundingClientRect();
    for (let a = el.parentElement; a && a !== document.body; a = a.parentElement) {
      const y = getComputedStyle(a).overflowY;
      if (y === 'auto' || y === 'scroll') {
        const b = a.getBoundingClientRect();
        return { top: Math.max(b.top, 0), bottom: Math.min(b.bottom, view), at: r.top + a.scrollTop, box: a };
      }
    }
    const css = getComputedStyle(el);
    const px = (name: string) => parseFloat(css.getPropertyValue(name)) || 0;
    return { top: px('--top-h'), bottom: view - px('--bottom-h'), at: r.top + window.scrollY, box: null };
  }

  // Measured as it comes, and again only when what is above it or the room changes (the owner, 2026-11-01: scrolling
  // the loading page stuttered "as if continuously recalculating instead of sitting perfectly still"; it was measured
  // every frame against the window as scrolled, so it grew as it was scrolled and its mark chased the scroll a frame
  // late). As if scrolled to the top, so scrolling changes nothing: it fills the room left and the mark sits still.
  $effect(() => {
    let raf = 0;
    const measure = () => {
      raf = 0;
      const mark = (size = loaderSizeIn(el));
      const { top, bottom, at } = seen();
      const middle = Math.max((top + bottom) / 2, at + mark / 2);
      const next = { height: Math.round(Math.max(bottom - at, mark * 2)), top: Math.round(middle - at - mark / 2) };
      const was = untrack(() => fit);
      if (next.height !== was?.height || next.top !== was?.top) fit = next;
    };
    const ask = () => (raf ||= requestAnimationFrame(measure));
    measure();
    // What is above it settling (its own height changes the parent too, measured to the same and left so).
    const sized = new ResizeObserver(ask);
    if (el.parentElement) sized.observe(el.parentElement);
    const box = seen().box;
    if (box) sized.observe(box);
    addEventListener('resize', ask);
    window.visualViewport?.addEventListener('resize', ask);
    return () => {
      cancelAnimationFrame(raf);
      sized.disconnect();
      removeEventListener('resize', ask);
      window.visualViewport?.removeEventListener('resize', ask);
    };
  });
</script>

<div
  class="page-loading"
  class:fitted={fit !== null}
  bind:this={el}
  style:height={fit ? `${fit.height}px` : null}
  style:padding-top={fit ? `${fit.top}px` : null}
>
  {#if size !== null}<MarkLoader {size} />{/if}
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
