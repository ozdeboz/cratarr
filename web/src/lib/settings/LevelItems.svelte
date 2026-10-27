<script lang="ts">
  // Level the volume (decision 0035): the switch, and what a track not measured yet gets (the owner, 2026-10-26: a
  // typical track's level, or as it is). In the app its own player's, here this browser's. The same rows in This
  // device's Playing (lib/settings/DevicePart) and in the sheet a long press on the player's Level button opens
  // (lib/LevelSheet). Whether the waveform shows it: this page's own (the owner, 2026-10-27).
  import { levelOn, levelUnknown, setLevelOn, setLevelUnknown } from '../level.svelte';
  import { prefs } from '../prefs.svelte';
  import Seg from '../Seg.svelte';
  import Item from './Item.svelte';

  const on = $derived(levelOn());
  const unknown = $derived(levelUnknown());
  const UNKNOWN: { value: 'typical' | 'untouched'; label: string }[] = [
    { value: 'typical', label: 'Typical' },
    { value: 'untouched', label: 'As is' },
  ];
</script>

<Item
  name="Level the volume"
  line="Every track about as loud as the next"
  more={"Each track plays at about the same loudness, as Cratarr measured it (or as the file's ReplayGain tags say, until it has). It only ever turns a track down.\n\nAn album played through keeps its own dynamics; shuffle and radio are levelled track by track."}
  toggle={{ checked: on, onchange: setLevelOn }}
/>
{#if on}
  <Item
    name="Not measured yet"
    more={"A track Cratarr has not measured yet: turned down as a typical track is, or played as it is until it has been."}
    ><Seg label="Not measured yet" options={UNKNOWN} bind:value={() => unknown, setLevelUnknown} /></Item
  >
  <Item
    name="Show in the waveform"
    more="The player's waveform as you hear it: a track turned down shows lower."
    toggle={{ checked: prefs.levelInWave, onchange: (v) => (prefs.levelInWave = v) }}
  />
{/if}
