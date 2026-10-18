<script lang="ts">
  // The help a held label asks for (held.svelte.ts), under the label, or over it where there is no room below.
  import { helpShown, hideHelp } from './held.svelte';

  const GAP = 6;
  const MARGIN = 8;
  const WIDTH = 280;

  let box = $state<HTMLDivElement>();
  let height = $state(0);

  const place = $derived.by(() => {
    const vw = typeof innerWidth === 'number' ? innerWidth : 1000;
    const vh = typeof innerHeight === 'number' ? innerHeight : 800;
    const width = Math.min(WIDTH, vw - 2 * MARGIN);
    const left = Math.min(Math.max(MARGIN, helpShown.left), vw - width - MARGIN);
    const below = helpShown.bottom + GAP + height + MARGIN <= vh || helpShown.top - GAP - height < MARGIN;
    return { left, width, top: below ? helpShown.bottom + GAP : helpShown.top - GAP - height };
  });

  $effect(() => {
    if (!helpShown.open) return;
    // Away on a tap anywhere but on it (before anything under the tap stops it on its way), on Escape, and on any
    // scroll (its label has moved). The tap that puts it away does nothing else: on the picture it would have closed
    // the panel too, on a button pressed it.
    const swallow = (e: Event) => {
      e.stopPropagation();
      e.preventDefault();
    };
    const tap = (e: PointerEvent) => {
      if (box && e.target instanceof Node && box.contains(e.target)) return;
      swallow(e);
      hideHelp();
      for (const type of ['pointerup', 'click'] as const) addEventListener(type, swallow, { capture: true, once: true });
      setTimeout(() => {
        removeEventListener('pointerup', swallow, true);
        removeEventListener('click', swallow, true);
      }, 800);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        hideHelp();
      }
    };
    addEventListener('pointerdown', tap, true);
    addEventListener('keydown', key, true);
    addEventListener('scroll', hideHelp, true);
    addEventListener('resize', hideHelp);
    return () => {
      removeEventListener('pointerdown', tap, true);
      removeEventListener('keydown', key, true);
      removeEventListener('scroll', hideHelp, true);
      removeEventListener('resize', hideHelp);
    };
  });
</script>

{#if helpShown.open}
  <div
    class="help"
    role="tooltip"
    bind:this={box}
    bind:clientHeight={height}
    style:left="{place.left}px"
    style:top="{place.top}px"
    style:width="{place.width}px"
  >
    {helpShown.text}
  </div>
{/if}

<style>
  .help {
    position: fixed;
    /* Over the visuals (2000), wherever it is placed. */
    z-index: 2001;
    box-sizing: border-box;
    padding: 10px 12px;
    border-radius: 10px;
    /* A shade lighter than the panel, so it reads as on top of it. */
    background: rgb(44 44 52 / 0.98);
    border: 1px solid rgb(255 255 255 / 0.12);
    box-shadow: 0 6px 24px rgb(0 0 0 / 0.5);
    color: #fff;
    font-size: 14px;
    line-height: 1.4;
    pointer-events: auto;
  }
</style>
