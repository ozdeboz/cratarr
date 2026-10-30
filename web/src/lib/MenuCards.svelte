<script lang="ts">
  // A menu's entries as Symfonium's: rounded cards a little apart (lib/MenuGroup each), under the item's card.
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
</script>

<div class="cards" role="menu">{@render children()}</div>

<style>
  .cards {
    display: grid;
    gap: 12px;
    padding: 4px 12px 8px;
  }
</style>
