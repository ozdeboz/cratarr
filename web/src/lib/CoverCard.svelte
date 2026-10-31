<script lang="ts">
  // Home's Sounds like your week card, as one piece (0.13; the owner, 2026-10-31: the crate card is "A · Week"): a strip
  // of covers across its top (a tap opens what it shows), then what it is (`children`: a heading, a line, a row of
  // pills). Home's week and the search's crate card (lib/SearchCrateCard). `wide`: six covers across where it has the
  // room (a page's search), four on a phone, as Home's always are.
  import type { Snippet } from 'svelte';
  import Cover from './Cover.svelte';

  let {
    covers,
    label,
    onopen = null,
    wide = false,
    children,
  }: { covers: readonly string[]; label: string; onopen?: (() => void) | null; wide?: boolean; children: Snippet } = $props();

  /** A strip only where it fills a row of four (else none, as Home's week had it); a wide one one row. */
  const strip = $derived(covers.length < 4 ? [] : wide ? covers.slice(0, covers.length >= 6 ? 6 : 4) : covers);
</script>

<div class="card" role="group" aria-label={label}>
  {#if strip.length}
    {#if onopen}
      <button class="mosaic" class:six={strip.length === 6} onclick={onopen} aria-label="Open {label}">
        {#each strip as src (src)}<span><Cover {src} /></span>{/each}
      </button>
    {:else}
      <div class="mosaic" class:six={strip.length === 6} aria-hidden="true">
        {#each strip as src (src)}<span><Cover {src} /></span>{/each}
      </div>
    {/if}
  {/if}
  <div class="about">{@render children()}</div>
</div>

<style>
  .card {
    overflow: hidden;
    border-radius: 16px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
    container-type: inline-size;
  }

  .mosaic {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    width: 100%;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }

  div.mosaic {
    cursor: default;
  }

  .mosaic span {
    display: block;
    aspect-ratio: 1;
    /* Square whatever the image's shape: without it a portrait cover stretched its box, and its card (the owner,
       2026-10-24). */
    overflow: hidden;
  }

  /* Six across with the room for them; a phone's four. */
  .mosaic.six span:nth-child(n + 5) {
    display: none;
  }

  @container (min-width: 560px) {
    .mosaic.six {
      grid-template-columns: repeat(6, 1fr);
    }

    .mosaic.six span:nth-child(n + 5) {
      display: block;
    }
  }

  .about {
    padding: 14px 16px 16px;
  }

  .about :global(h3) {
    margin: 0;
    font-size: 1.1rem;
  }

  .about :global(p) {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: 0.9rem;
  }

  /* Its pills (the week's genres; a crate's terms). */
  .about :global(.pills) {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  .about :global(.pills button) {
    padding: 2px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }
</style>
