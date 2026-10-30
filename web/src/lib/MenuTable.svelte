<script lang="ts">
  // A menu's entries as the table has them (decision 0058, lib/menuTable): its kind's cards in order, each entry
  // drawn by the menu's own `entry` (which shows it or not, as what is true of the object says); a card left with
  // nothing in it is not shown (lib/MenuGroup). Discover's card and a selection's extra draw their own cards.
  // `children`: what goes with the entries unseen (a file picker). Out of reach, only what works without Cratarr shows
  // (lib/menuTable's WORKS_AWAY; decision 0085), and one line says the rest needs it.
  import type { Snippet } from 'svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { cardsOf, offeredAway, OWN_CARD, type EntryId, type MenuKind } from './menuTable';
  import { reach } from './reach.svelte';

  let { kind, entry, children }: { kind: MenuKind; entry: Snippet<[EntryId]>; children?: Snippet } = $props();
  const shown = (entries: readonly EntryId[]) => entries.filter((e) => offeredAway(e, reach.away));
</script>

<MenuCards>
  {#each cardsOf(kind) as card (card.group)}
    {#if card.entries.every((e) => OWN_CARD.includes(e))}
      {#each shown(card.entries) as e (e)}{@render entry(e)}{/each}
    {:else}
      <MenuGroup label={card.label}>{#each shown(card.entries) as e (e)}{@render entry(e)}{/each}</MenuGroup>
    {/if}
  {/each}
  {@render children?.()}
  {#if reach.away}<p class="away">The rest needs Cratarr.</p>{/if}
</MenuCards>

<style>
  .away {
    margin: 4px 16px 8px;
    color: var(--muted);
    font-size: 13px;
  }
</style>
