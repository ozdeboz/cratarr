<script lang="ts" module>
  /** One cause that left nothing (decision 0095: reasons are data): its word, and what a tap does: ✕ clears it, or it
   *  opens where it is dealt with (the exclusions: too much for one ✕, the owner 2026-11-02). */
  export type NothingChip = { label: string; onclear?: () => void; onopen?: () => void };
  /** Those hanging from one control, in the order they came (the owner, 2026-11-02: the page's and its panel's both left
   *  nothing for the same words, and theirs lay over each other): each under the one before, the tip the first's. */
  const stacks = new Map<HTMLElement, HTMLElement[]>();
  const STACK_GAP = 8;
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
    more = [],
  }: {
    /** The control that caused it: the tip points at its middle. None in sight: it stands in place, in the empty
     *  list, without a tip. */
    anchor: HTMLElement | null | undefined;
    /** What is not there ("No tracks with", "No records for"). */
    said: string;
    chips: NothingChip[];
    /** Every cause cleared at once; offered where there are several. */
    onclearall?: (() => void) | null;
    /** Other true answers, each its own line under the first (decision 0095: what else would bring things back). */
    more?: { said: string; chips: NothingChip[] }[];
  } = $props();

  const EDGE = 8;
  /** Room between the control and the tip's point. */
  const GAP = 12;
  let el = $state<HTMLElement>();
  let at = $state<{ left: number; top: number; tip: number; first: boolean } | null>(null);

  function place() {
    const a = anchor;
    const box = el;
    if (!a || !box || !a.isConnected) return void (at = null);
    const r = a.getBoundingClientRect();
    // Its control not drawn (hidden, gone): no tip on nothing.
    if (!r.width && !r.height) return void (at = null);
    const w = box.offsetWidth;
    const mid = r.left + r.width / 2;
    const left = Math.max(EDGE, Math.min(mid - w / 2, innerWidth - EDGE - w));
    const before = above(a, box);
    at = { left, top: r.bottom + GAP + before.reduce((n, b) => n + b.offsetHeight + STACK_GAP, 0), tip: Math.max(16, Math.min(w - 16, mid - left)), first: !before.length };
  }
  /** The ones over this one on the same control. */
  const above = (a: HTMLElement, box: HTMLElement) => {
    const list = stacks.get(a) ?? [];
    return list.slice(0, Math.max(0, list.indexOf(box)));
  };

  $effect(() => {
    const a = anchor;
    const box = el;
    if (!a || !box) return void (at = null);
    // The top layer: over a panel's edge and whatever scrolls it (a browser without it: fixed, as high as anything).
    try {
      box.showPopover?.();
    } catch {
      /* shown already */
    }
    // Its control moves without changing size (the search folding slides the magnifier along the glass; a panel's
    // action row scrolls): followed each frame while it shows, placed again only when something moved.
    stacks.set(a, [...(stacks.get(a) ?? []), box]);
    let raf = 0;
    let was = '';
    // Under a sheet that covers its control (the Filter sheet over the page; the owner, 2026-11-02: it lay over the
    // sheet): gone until the sheet is, the top layer being above every sheet.
    const covered = () => [...document.querySelectorAll('[aria-modal="true"]')].some((d) => d !== box && !d.contains(a) && d.getClientRects().length > 0);
    const follow = () => {
      if (covered()) {
        if (was !== 'covered') (was = 'covered'), (at = null);
        raf = requestAnimationFrame(follow);
        return;
      }
      const r = a.getBoundingClientRect();
      const now = `${r.left},${r.bottom},${r.width},${box.offsetWidth},${innerWidth},${above(a, box).map((b) => b.offsetHeight).join()}`;
      if (now !== was) {
        was = now;
        place();
      }
      raf = requestAnimationFrame(follow);
    };
    follow();
    return () => {
      cancelAnimationFrame(raf);
      const rest = (stacks.get(a) ?? []).filter((b) => b !== box);
      if (rest.length) stacks.set(a, rest);
      else stacks.delete(a);
    };
  });
</script>

<div
  class="nothing"
  class:in-place={!anchor}
  popover={anchor ? 'manual' : undefined}
  role="status"
  bind:this={el}
  class:placed={at !== null}
  class:under={at !== null && !at.first}
  style:left={at ? `${at.left}px` : null}
  style:top={at ? `${at.top}px` : null}
  style:--tip={at ? `${at.tip}px` : null}
  transition:fade={{ duration: 150 }}
>
  <span class="line">
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
  </span>
  {#each more as line (line.said + line.chips.map((c) => c.label).join())}
    <span class="line">
      <span class="said">{line.said}</span>
      {#each line.chips as c (c.label)}
        {#if c.onopen}<button class="chip open" onclick={c.onopen}>{c.label}<Icon name="chevronRight" size={14} /></button>
        {:else}<span class="chip"><span class="q">{c.label}</span>{#if c.onclear}{@const clear = c.onclear}<button class="clear" aria-label={`Clear ${c.label}`} title="Clear" onclick={clear}><Icon name="close" size={14} /></button>{/if}</span>{/if}
      {/each}
    </span>
  {/each}
</div>

<style>
  /* One slim glass under its control (A2's look): its words, the causes as chips; several wrap. Each answer a line of its
     own, the glass as wide as the widest (the owner, 2026-11-02: a second line stretched it to the window's edge). */
  .nothing {
    position: fixed;
    inset: auto;
    z-index: 60;
    display: grid;
    justify-items: start;
    gap: 6px;
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

  /* No control to point at: in the list's place, as a line of the page. */
  .nothing.in-place {
    position: static;
    width: fit-content;
    margin: 24px 16px;
    visibility: visible;
  }

  .nothing.in-place::before,
  .nothing.under::before {
    display: none;
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

  /* An answer: a line of its own, the first and each other one under it. */
  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
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
