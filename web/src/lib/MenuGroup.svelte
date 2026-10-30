<script lang="ts">
  // Related entries of a menu joined on one rounded card (Symfonium's): each a row of it a hairline apart, the card's
  // corners on the first and last; a card with nothing in it is not shown. Cards go in lib/MenuCards.
  import type { Snippet } from 'svelte';

  let { label, children }: { label?: string; children: Snippet } = $props();
</script>

<div class="group" role="group" aria-label={label}>{@render children()}</div>

<style>
  .group {
    display: grid;
    gap: 3px;
  }

  .group:empty {
    display: none;
  }

  .group > :global(.entry) {
    min-height: 60px;
    padding: 0 20px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
  }

  .group > :global(.entry:hover),
  .group > :global(.entry:focus-visible) {
    background: color-mix(in srgb, var(--text) 13%, transparent);
  }

  .group > :global(.entry:first-child) {
    border-top-left-radius: 20px;
    border-top-right-radius: 20px;
  }

  .group > :global(.entry:last-child) {
    border-bottom-left-radius: 20px;
    border-bottom-right-radius: 20px;
  }
</style>
