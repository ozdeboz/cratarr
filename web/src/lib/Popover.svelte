<script lang="ts">
  // A small menu anchored to an item (docs/ui-conventions.md): below it when there is room, else above; kept on
  // screen sideways; the arrow points at the item. Esc, a tap outside or scrolling the page closes it.
  import type { Snippet } from 'svelte';

  let { anchor, label, onclose, children }: { anchor: DOMRect; label: string; onclose: () => void; children: Snippet } =
    $props();

  let box = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0, arrow: 0, below: true });

  const MARGIN = 8;
  const GAP = 10;

  $effect(() => {
    if (!box) return;
    const w = box.offsetWidth;
    const h = box.offsetHeight;
    const below = anchor.bottom + GAP + h <= innerHeight - MARGIN || anchor.top - GAP - h < MARGIN;
    const centre = anchor.left + anchor.width / 2;
    const left = Math.min(Math.max(centre - w / 2, MARGIN), innerWidth - w - MARGIN);
    pos = {
      left,
      top: below ? anchor.bottom + GAP : anchor.top - GAP - h,
      arrow: Math.min(Math.max(centre - left, 16), w - 16),
      below,
    };
    box.querySelector<HTMLElement>('button, input, select')?.focus();
  });

  $effect(() => {
    const key = (e: KeyboardEvent) => e.key === 'Escape' && onclose();
    const outside = (e: PointerEvent) => box && !box.contains(e.target as Node) && onclose();
    const scrolled = (e: Event) => box && !box.contains(e.target as Node) && onclose();
    addEventListener('keydown', key);
    addEventListener('pointerdown', outside, true);
    addEventListener('scroll', scrolled, true);
    addEventListener('resize', onclose);
    return () => {
      removeEventListener('keydown', key);
      removeEventListener('pointerdown', outside, true);
      removeEventListener('scroll', scrolled, true);
      removeEventListener('resize', onclose);
    };
  });
</script>

<div
  class="pop"
  class:below={pos.below}
  bind:this={box}
  role="dialog"
  aria-label={label}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  style:--arrow="{pos.arrow}px"
>
  {@render children()}
</div>

<style>
  .pop {
    position: fixed;
    z-index: 40;
    min-width: 220px;
    max-width: min(320px, calc(100vw - 16px));
    padding: 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.4);
  }

  /* The arrow: a rotated square on the edge facing the item. */
  .pop::before {
    content: '';
    position: absolute;
    left: calc(var(--arrow) - 7px);
    width: 12px;
    height: 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    transform: rotate(45deg);
  }

  .pop.below::before {
    top: -7px;
    border-right: 0;
    border-bottom: 0;
  }

  .pop:not(.below)::before {
    bottom: -7px;
    border-left: 0;
    border-top: 0;
  }
</style>
