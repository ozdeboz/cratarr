<script lang="ts">
  // A page's or a panel's whole content loading: the mark in motion (lib/MarkLoader.svelte), in the middle of what can
  // be seen of the page or the component it is in (the window between the bar of pages and the mini-player, or the
  // panel or sheet that scrolls it); lower only where what comes before it reaches that far. Its size is where it is:
  // a page's large, a panel's or a sheet's small, as a panel's swap (lib/Hold) and a sheet's own (the owner,
  // 2026-11-02: the large one in a crate's panel, "we're using smaller kinds these days in smaller panels").
  import { untrack } from 'svelte';
  import MarkLoader from './MarkLoader.svelte';
  import { LOADER_SMALL, loaderSizeIn } from './loader';
  import { holdUntil } from './holdPlace.svelte';
  import { waitingFor } from './waiting.svelte';

  /** `children`: what goes under the mark, where the work can say how far it is (the owner, 2026-10-24: Discover's
   *  first look, its progress under the loader at its usual place). With none, what it waits for where that is the library
   *  on this device opening or a busy Cratarr (lib/waiting; the owner, 2026-11-08: "a standard one, with the progress explaining what
   *  it is"). */
  /** `held`: false where it waits for one part of a page that is otherwise ready (a record not had's track list while its
   *  list is read, F6): the page shows at once, and only that part waits (the review of preview 1's fixes, PB1). */
  /** `row`: a section that is always one horizontal row (the owner, 2026-11-19: "the loading message on the right"): the
   *  small mark at the row's start, what it waits for to its right, the row's own height; it never holds the page. */
  let { children, held = true, row = false }: { children?: import('svelte').Snippet; held?: boolean; row?: boolean } = $props();

  /** Its size where it is (lib/loader): known once it is placed (its mark shows from then). */
  let size = $state<number | null>(null);

  // A page or panel loading as a whole is not ready (lib/Hold): what it showed stays while this is here.
  if (untrack(() => held && !row))
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
    if (row) {
      size = LOADER_SMALL;
      return;
    }
    let raf = 0;
    const measure = () => {
      raf = 0;
      const mark = (size = loaderSizeIn(el));
      const { top, bottom, at } = seen();
      // The middle of its own room as seen: from its top (or what can be seen's, if it starts above) to the bottom of what
      // can be seen, as much room above the mark as below (the owner, 2026-11-19: "still not vertically center aligned": the
      // window's middle put a loader that starts below it at its very top).
      const middle = Math.max((Math.max(top, at) + bottom) / 2, at + mark / 2);
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
    // Everything above it too, up to the scrolling box: a header settling (a record's cover known, its hero shorter) moves it
    // without resizing its parent, and the mark measured before stayed near its top (the owner, 2026-11-19: "There's more
    // space below than above").
    for (let a: Element | null = el; a && a !== box && a !== document.body; a = a.parentElement)
      for (let x = a.previousElementSibling; x; x = x.previousElementSibling) sized.observe(x);
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
  class:fitted={!row && fit !== null}
  class:row
  bind:this={el}
  style:height={!row && fit ? `${fit.height}px` : null}
  style:padding-top={!row && fit ? `${fit.top}px` : null}
>
  <!-- The mark is placed alone; what it waits for hangs under it (or beside it in a row) without moving it, so the line
       coming or going shifts nothing (the owner, 2026-11-19). -->
  <div class="mark">
    {#if size !== null}<MarkLoader {size} />{/if}
    {#if children}<div class="under">{@render children()}</div>{:else if waitingFor()}<div class="under"><p class="opening">{waitingFor()}</p></div>{/if}
  </div>
</div>

<style>
  .page-loading {
    display: grid;
    place-items: center;
    min-height: 40vh;
  }

  .mark {
    position: relative;
    display: grid;
    place-items: center;
  }

  .under {
    position: absolute;
    top: 100%;
    left: 50%;
    width: min(320px, calc(100vw - 32px));
    margin-top: 16px;
    transform: translateX(-50%);
    text-align: center;
  }

  /* One row: the mark at its start, the words to its right, as tall as the row. */
  .page-loading.row {
    display: flex;
    align-items: center;
    min-height: 0;
    padding: 4px 16px;
  }

  .row .mark {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .row .under {
    position: static;
    width: auto;
    margin: 0;
    transform: none;
    text-align: start;
  }

  .opening {
    margin: 0;
    color: var(--muted);
  }

  /* A progress bar under it (lib/Meter, at most 240 px) centred too, under the mark, not at the box's left (the owner,
     2026-10-24: Discover's first look). */
  .under :global(.track) {
    margin-inline: auto;
  }

  /* Measured: as high as the room left, the mark placed from the top. */
  .page-loading.fitted {
    align-items: start;
    align-content: start;
    min-height: 0;
  }
</style>
