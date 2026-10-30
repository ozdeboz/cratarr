<script lang="ts">
  // A list's section (docs/ui-conventions.md → Rows): a heading with its count that folds the list away, and under it,
  // while open, one line saying what the list is and then the rows. The same on a page (Health) and in the panel (the
  // Repair tab, a selection's Tracks).
  import type { Snippet } from 'svelte';
  import type { Attachment } from 'svelte/attachments';
  import Icon from './Icon.svelte';

  let {
    title,
    count,
    hint,
    dot,
    open = $bindable(true),
    children,
    attach,
  }: {
    title: string;
    /** "12 tracks": muted after the title. */
    count?: string;
    /** What the list is, under the heading while it is open. */
    hint?: string;
    /** A colour dot before the title: the list's colour in a stacked tempo spread above (decision 0041). */
    dot?: string;
    open?: boolean;
    children: Snippet;
    attach?: Attachment<HTMLElement>;
  } = $props();
</script>

<section class="sect" {@attach attach}>
  <button class="head" class:folded={!open} aria-expanded={open} onclick={() => (open = !open)}
    >{#if dot}<i class="dot" style:background={dot} aria-hidden="true"></i>{/if}<span class="t">{title}</span>{#if count}<span class="c">{count}</span>{/if}<Icon name="chevronUp" size={20} /></button
  >
  {#if open}
    {#if hint}<p class="hint">{hint}</p>{/if}
    {@render children()}
  {/if}
</section>

<style>
  .sect + :global(.sect) {
    margin-top: 6px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 44px;
    padding: 0 12px;
    border: 0;
    border-radius: 10px;
    background: color-mix(in srgb, var(--text) 5%, transparent);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }

  .t {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }

  .c {
    flex: none;
    color: var(--muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .head :global(svg) {
    flex: none;
    transition: transform 150ms;
  }

  .head.folded :global(svg) {
    transform: rotate(180deg);
  }

  /* In line with the rows' titles. */
  .hint {
    margin: 8px 12px;
    color: var(--muted);
    font-size: 0.9rem;
  }

  @media (prefers-reduced-motion: reduce) {
    .head :global(svg) {
      transition: none;
    }
  }

  /* Animations off on this device (Display): the same as the system's reduced motion. */
  :global(:root[data-motion='off']) .head :global(svg) {
    transition: none;
  }
</style>
