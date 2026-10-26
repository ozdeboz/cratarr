<script lang="ts" module>
  import { prefs } from '../prefs.svelte';

  /** The facts back to none chosen; its undo. */
  export function resetFacts(): () => void {
    const was = prefs.playerFacts;
    prefs.playerFacts = [];
    return () => (prefs.playerFacts = was);
  }
</script>

<script lang="ts">
  // The player's extra details (the owner, 2026-10-19): the row shown or not, and which facts beside its info button in
  // what order (lib/settings/PlayerRowList). In Settings → Player (lib/settings/PlayerPart), and without its switch in
  // the sheet a long press on the info button opens (lib/DetailsSheet, the owner 2026-10-26), whose header has its reset.
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import OrderReset from './OrderReset.svelte';
  import PlayerRowList from './PlayerRowList.svelte';

  let { toggle = true }: { toggle?: boolean } = $props();
</script>

<Group title={toggle ? 'Details' : undefined}>
  {#snippet tail()}
    {#if toggle && prefs.playerRow}<OrderReset label="Extra details" isDefault={!prefs.playerFacts.length} reset={resetFacts} />{/if}
  {/snippet}
  {#if toggle}<Item name="Extra details" toggle={{ checked: prefs.playerRow, onchange: (on) => (prefs.playerRow = on) }} />{/if}
  {#if prefs.playerRow}<PlayerRowList />{/if}
</Group>
