<script lang="ts">
  // Player (the owner, 2026-10-25: Display's concerns apart): the player's row of extra details (shown or not, and which
  // facts in what order: lib/settings/DetailsGroup) and the full-screen player's bands (lib/BandControls).
  import BandControls from '../BandControls.svelte';
  import { COVER_TOUCHES, prefs, SWIPES_DEFAULT, SWIPES_DOWN, SWIPES_SIDE, SWIPES_UP, type PlayerSwipes } from '../prefs.svelte';
  import ChoiceSheet from './ChoiceSheet.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import DetailsGroup from './DetailsGroup.svelte';
  import { offerReset } from './reset.svelte';

  offerReset(
    () => "The player's extra details shown, none of them chosen; swiping up the queue, down closing.",
    () => {
      prefs.playerRow = true;
      prefs.playerFacts = [];
      prefs.playerSwipes = { ...SWIPES_DEFAULT };
    },
  );
  const SWIPE_ROWS: { key: keyof PlayerSwipes; name: string; options: { value: string; label: string }[] }[] = [
    { key: 'upLeft', name: 'Swipe up, left', options: SWIPES_UP },
    { key: 'upRight', name: 'Swipe up, right', options: SWIPES_UP },
    { key: 'downLeft', name: 'Swipe down, left', options: SWIPES_DOWN },
    { key: 'downRight', name: 'Swipe down, right', options: SWIPES_DOWN },
    { key: 'left', name: 'Swipe left', options: SWIPES_SIDE },
    { key: 'right', name: 'Swipe right', options: SWIPES_SIDE },
    { key: 'tap', name: 'Tap', options: COVER_TOUCHES },
    { key: 'doubleTap', name: 'Double tap', options: COVER_TOUCHES },
    { key: 'circle', name: 'Circle', options: COVER_TOUCHES },
  ];
  let choosing = $state<(typeof SWIPE_ROWS)[number] | null>(null);
</script>

<DetailsGroup />

<!-- The player's swipes (the owner, 2026-10-26): up and down, on its left half and its right, each its choice. -->
<Group
  title="Gestures"
  more="Swipes up and down anywhere on the player, apart for its left and its right half; left and right, a tap, a double tap and a circle on the cover."
>
  {#each SWIPE_ROWS as r (r.key)}
    <Item name={r.name} value={r.options.find((o) => o.value === prefs.playerSwipes[r.key])?.label ?? null} onclick={() => (choosing = r)} />
  {/each}
</Group>
{#if choosing}
  {@const r = choosing}
  <ChoiceSheet
    title={r.name}
    options={r.options}
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
