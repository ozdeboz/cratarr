<script lang="ts" module>
  /** One cause that left nothing (decision 0095: reasons are data): its word, and what a tap does: ✕ clears it, or it
   *  opens where it is dealt with (the exclusions: too much for one ✕, the owner 2026-11-02). */
  export type NothingChip = { label: string; onclear?: () => void; onopen?: () => void };
</script>

<script lang="ts">
  // Nothing, caused here (decision 0095, "One way to say nothing"): one element on pages, panels and sheets, a slim
  // glass hanging from the control that caused it, its tip on that control (the magnifier, the funnel, a crate's heart
  // or disc, the libraries' switch), each cause a chip, Clear all where there are several. Grown from 0.13's empty page
  // search (A2: "No records for [words ✕]"), which is this now. In the top layer (the platform's popover), so a panel
  // never clips it; placed under its control, kept inside the window, following it as it moves.
  import { fade } from 'svelte/transition';
  import Icon from './Icon.svelte';

  let {
    anchor,
    said,
    chips,
    onclearall = null,
  }: {
    /** The control that caused it: the tip points at its middle. */
    anchor: HTMLElement | null | undefined;
    /** What is not there ("No tracks with", "No records for"). */
    said: string;
    chips: NothingChip[];
    /** Every cause cleared at once; offered where there are several. */
    onclearall?: (() => void) | null;
  } = $props();

  const EDGE = 8;
  /** Room between the control and the tip's point. */
  const GAP = 12;
  let el = $state<HTMLElement>();
  let at = $state<{ left: number; top: number; tip: number } | null>(null);

  function place() {
    const a = anchor;
    const box = el;
    if (!a || !box || !a.isConnected) return void (at = null);
    const r = a.getBoundingClientRect();
    const w = box.offsetWidth;
    const mid = r.left + r.width / 2;
    const left = Math.max(EDGE, Math.min(mid - w / 2, innerWidth - EDGE - w));
    at = { left, top: r.bottom + GAP, tip: Math.max(16, Math.min(w - 16, mid - left)) };
  }

  $effect(() => {
    const a = anchor;
    const box = el;
    if (!a || !box) return;
    // The top layer: over a panel's edge and whatever scrolls it (a browser without it: fixed, as high as anything).
    try {
      box.showPopover?.();
    } catch {
      /* shown already */
    }
    // Its control moves without changing size (the search folding slides the magnifier along the glass; a panel's
    // action row scrolls): followed each frame while it shows, placed again only when something moved.
    let raf = 0;
    let was = '';
    const follow = () => {
      const r = a.getBoundingClientRect();
      const now = `${r.left},${r.bottom},${r.width},${box.offsetWidth},${innerWidth}`;
      if (now !== was) {
        was = now;
        place();
      }
      raf = requestAnimationFrame(follow);
    };
    follow();
    return () => cancelAnimationFrame(raf);
  });
</script>

<div
  class="nothing"
  popover="manual"
  role="status"
  bind:this={el}
  class:placed={at !== null}
  style:left={at ? `${at.left}px` : null}
  style:top={at ? `${at.top}px` : null}
  style:--tip={at ? `${at.tip}px` : null}
  transition:fade={{ duration: 150 }}
>
  <span class="said">{said}</span>
  {#each chips as c (c.label)}
    {#if c.onopen}
      <button class="chip open" onclick={c.onopen}>{c.label}<Icon name="chevronRight" size={14} /></button>
    {:else}
      <span class="chip"><span class="q">{c.label}</span>{#if c.onclear}{@const clear = c.onclear}<button class="clear" aria-label={`Clear ${c.label}`} title="Clear" onclick={clear}><Icon name="close" size={14} /></button>{/if}</span>
    {/if}
  {/each}
  {#if onclearall && chips.filter((c) => c.onclear).length > 1}
    <button class="all" onclick={onclearall}>Clear all</button>
  {/if}
</div>

<style>
  /* One slim glass under its control (A2's look): its words, the causes as chips; several wrap. */
  .nothing {
    position: fixed;
    inset: auto;
    z-index: 60;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    max-width: min(440px, calc(100vw - 16px));
    margin: 0;
    padding: 6px 6px 6px 14px;
    overflow: visible;
    border: 0;
    border-radius: 22px;
    background: var(--surface);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--text) 9%, transparent), 0 12px 32px rgb(0 0 0 / 0.4);
    color: var(--text);
    visibility: hidden;
  }

  /* Placed once its control is measured: never a frame in the wrong place. */
  .nothing.placed {
    visibility: visible;
  }

  /* The tip, on its control's middle. */
  .nothing::before {
    content: '';
    position: absolute;
    top: -6px;
    left: calc(var(--tip, 50%) - 6px);
    width: 12px;
    height: 12px;
    transform: rotate(45deg);
    background: var(--surface);
    box-shadow: -1px -1px 0 color-mix(in srgb, var(--text) 9%, transparent);
  }

  .said {
    color: var(--muted);
    font-size: 14px;
    white-space: nowrap;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 100%;
    padding: 4px 4px 4px 12px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 9%, transparent);
    font-size: 14px;
    white-space: nowrap;
  }

  .chip .q {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  button.chip.open {
    gap: 2px;
    padding-right: 8px;
    border: 0;
    color: inherit;
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }

  .clear {
    display: grid;
    flex: none;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
    cursor: pointer;
  }

  .all {
    padding: 4px 10px;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
  }
</style>
