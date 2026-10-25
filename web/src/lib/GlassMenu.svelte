<script lang="ts">
  // A few choices grown out of the glass a tap was on (the owner, 2026-10-25): the Library's view beside back, the
  // Cratarr | Local switch in the bar. The title's thin glass, the shown one ticked; a choice, a tap elsewhere or Esc
  // folds it. Drawn at the page's end where the caller says (`at`, from its button): inside another glass its own blur
  // would see only that glass (a backdrop inside a backdrop), not the page behind. Its options are buttons of class
  // `option` (`aria-selected` or `aria-checked` lit).
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
  style:left="{at.left}px"
  style:min-width="{at.minWidth}px">{@render children()}</span
>

<style>
  /* The title's thin glass (App's .lead), grown from the button. */
  .glass-menu {
    position: fixed;
    z-index: 25;
    display: grid;
    max-width: calc(100vw - 16px);
    padding: 6px;
    border-radius: 18px;
    background: color-mix(in srgb, var(--float-bg) 45%, transparent);
    -webkit-backdrop-filter: blur(10px) saturate(1.3);
    backdrop-filter: blur(10px) saturate(1.3);
    box-shadow: 0 2px 10px rgb(0 0 0 / 0.18);
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
