<script lang="ts">
  // A menu's entries as the table has them (decision 0058, lib/menuTable): its kind's cards in order, each entry
  // drawn by the menu's own `entry` (which shows it or not, as what is true of the object says); a card left with
  // nothing in it is not shown (lib/MenuGroup). Discover's card and a selection's extra draw their own cards.
  // `children`: what goes with the entries unseen (a file picker).
  import type { Snippet } from 'svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { cardsOf, OWN_CARD, type EntryId, type MenuKind } from './menuTable';

  let { kind, entry, children }: { kind: MenuKind; entry: Snippet<[EntryId]>; children?: Snippet } = $props();
</script>

<MenuCards>
  {#each cardsOf(kind) as card (card.group)}
    {#if card.entries.every((e) => OWN_CARD.includes(e))}
      {#each card.entries as e (e)}{@render entry(e)}{/each}
    {:else}
      <MenuGroup label={card.label}>{#each card.entries as e (e)}{@render entry(e)}{/each}</MenuGroup>
    {/if}
  {/each}
  {@render children?.()}
</MenuCards>
