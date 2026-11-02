<script lang="ts">
  // A few choices grown out of the glass a tap was on (the owner, 2026-10-25): the Library's view beside back, the
  // Cratarr | Local switch in the bar. The top glasses' own glass, the shown one ticked; a choice, a tap elsewhere or Esc
  // folds it. Drawn at the page's end where the caller says (`at`, from its button): inside another glass its own blur
  // would see only that glass (a backdrop inside a backdrop), not the page behind. Always whole inside the window (the
  // owner, 2026-11-02: "should be made impossible" to render off screen): moved in from an edge, taller than the room
  // it scrolls. Its options are buttons of class `option` (`aria-selected` or `aria-checked` lit).
  import type { Snippet } from 'svelte';
  import { portal } from './portal';

  let {
    at,
    keep,
    label,
    role = 'listbox',
    onclose,
    children,
  }: {
    /** Where, from the button's place: under it (`top`) or over it (`bottom`), from `left`, at least `minWidth` wide. */
    at: { top?: number; bottom?: number; left: number; minWidth: number };
    /** What a tap may land on without folding it (the button that opened it). */
    keep: () => HTMLElement | undefined;
    label: string;
    role?: 'listbox' | 'menu';
    onclose: () => void;
    children: Snippet;
  } = $props();
  let list = $state<HTMLElement>();

  /** Its left edge, moved in so its whole width is inside the window (8 px from either edge). */
  const EDGE = 8;
  let left = $state(0);
  $effect(() => {
    const want = at.left;
    const fit = () => {
      const w = list?.offsetWidth ?? 0;
      left = Math.max(EDGE, Math.min(want, innerWidth - EDGE - w));
    };
    fit();
    addEventListener('resize', fit);
    return () => removeEventListener('resize', fit);
  });

  $effect(() => {
    const away = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!keep()?.contains(t) && !list?.contains(t)) onclose();
    };
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && onclose();
    addEventListener('pointerdown', away, true);
    addEventListener('keydown', esc);
    return () => {
      removeEventListener('pointerdown', away, true);
      removeEventListener('keydown', esc);
    };
  });
</script>

<span
  class="glass-menu"
  {role}
  aria-label={label}
  use:portal
  bind:this={list}
  style:top={at.top === undefined ? null : `${at.top}px`}
  style:bottom={at.bottom === undefined ? null : `${at.bottom}px`}
  style:left="{left}px"
  style:min-width="{at.minWidth}px"
  style:max-height="calc(100dvh - {EDGE}px - {at.top ?? at.bottom ?? 0}px)">{@render children()}</span
>

<style>
  /* Grown from the button: the top glasses' own fill and blur (the owner, 2026-11-02: one colouring for the glasses and
     their dropdowns), its shadow, but no ring ("I don't want the dropdown to have a border effect"). */
  .glass-menu {
    position: fixed;
    z-index: 25;
    display: grid;
    align-content: start;
    max-width: calc(100vw - 16px);
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 6px;
    border-radius: 18px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .glass-menu :global(.option) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 44px;
    padding: 0 12px;
    border: 0;
    border-radius: 12px;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 16px;
    font-weight: 500;
    text-align: left;
    white-space: nowrap;
    cursor: pointer;
  }

  .glass-menu :global(.option:is([aria-selected='true'], [aria-checked='true'])) {
    color: var(--accent-ink);
    font-weight: 600;
  }

  @media (hover: hover) {
    .glass-menu :global(.option:hover) {
      background: color-mix(in srgb, var(--text) 8%, transparent);
    }
  }
</style>
