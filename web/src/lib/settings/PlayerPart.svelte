<script lang="ts">
  // Player (the owner, 2026-10-25: Display's concerns apart): the player's row of extra details (shown or not, and which
  // facts in what order: lib/settings/PlayerRowList) and the full-screen player's bands (lib/BandControls).
  import BandControls from '../BandControls.svelte';
  import { prefs, SWIPES_DOWN, SWIPES_UP, type PlayerSwipes } from '../prefs.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import OrderReset from './OrderReset.svelte';
  import PlayerRowList from './PlayerRowList.svelte';
  import { offerReset } from './reset.svelte';

  offerReset(
    () => "The player's extra details shown, none of them chosen; swiping up the queue, down closing.",
    () => {
      prefs.playerRow = true;
      prefs.playerFacts = [];
      prefs.playerSwipes = { upLeft: 'queue', upRight: 'queue', downLeft: 'close', downRight: 'close' };
    },
  );
  const SWIPE_ROWS: { key: keyof PlayerSwipes; name: string; up: boolean }[] = [
    { key: 'upLeft', name: 'Swipe up, left', up: true },
    { key: 'upRight', name: 'Swipe up, right', up: true },
    { key: 'downLeft', name: 'Swipe down, left', up: false },
    { key: 'downRight', name: 'Swipe down, right', up: false },
  ];
  let choosing = $state<(typeof SWIPE_ROWS)[number] | null>(null);
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

<!-- The player's swipes (the owner, 2026-10-26): up and down, on its left half and its right, each its choice. -->
<Group title="Gestures" more="Swipes up and down anywhere on the player, apart for its left and its right half.">
  {#each SWIPE_ROWS as r (r.key)}
    <Item name={r.name} value={(r.up ? SWIPES_UP : SWIPES_DOWN).find((o) => o.value === prefs.playerSwipes[r.key])?.label ?? null} onclick={() => (choosing = r)} />
  {/each}
</Group>
{#if choosing}
  {@const r = choosing}
  <ChoiceSheet
    title={r.name}
    options={r.up ? SWIPES_UP : SWIPES_DOWN}
    value={prefs.playerSwipes[r.key]}
    onchange={(v) => (prefs.playerSwipes = { ...prefs.playerSwipes, [r.key]: v })}
    onclose={() => (choosing = null)}
  />
{/if}

<!-- The full-screen player's bands (lib/BandGlass): the same settings as the player's own sheet (lib/BandControls), as
     rows here, each slider in its sheet (the owner, 2026-10-25: as Settings' other sliders). -->
<Group title="Visualisation">
  <BandControls rows />
</Group>
