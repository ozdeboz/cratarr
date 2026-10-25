<script lang="ts">
  // Player (the owner, 2026-10-25: Display's concerns apart): the player's row of extra details (shown or not, and which
  // facts in what order: lib/settings/PlayerRowList) and the full-screen player's bands (lib/BandControls).
  import BandControls from '../BandControls.svelte';
  import { prefs } from '../prefs.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import OrderReset from './OrderReset.svelte';
  import PlayerRowList from './PlayerRowList.svelte';
  import { offerReset } from './reset.svelte';

  offerReset(
    () => "The player's extra details shown, none of them chosen.",
    () => {
      prefs.playerRow = true;
      prefs.playerFacts = [];
    },
  );
</script>

<Group title="Details">
  {#snippet tail()}
    {#if prefs.playerRow}<OrderReset
        label="Extra details"
        isDefault={!prefs.playerFacts.length}
        reset={() => {
          const was = prefs.playerFacts;
          prefs.playerFacts = [];
          return () => (prefs.playerFacts = was);
        }}
      />{/if}
  {/snippet}
  <Item name="Extra details" toggle={{ checked: prefs.playerRow, onchange: (on) => (prefs.playerRow = on) }} />
  {#if prefs.playerRow}<PlayerRowList />{/if}
</Group>

<!-- The full-screen player's bands (lib/BandGlass): the look, and the two settings every look takes. -->
<!-- The bands: the same controls as the player's own sheet (lib/BandControls). -->
<Group title="Visualisation">
  <div class="bands"><BandControls /></div>
</Group>

<style>
  .bands {
    padding: 12px 16px 16px;
  }
</style>
