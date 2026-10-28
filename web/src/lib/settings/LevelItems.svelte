<script lang="ts">
  // Level the volume (decision 0035): the switch, and what a track not measured yet gets (the owner, 2026-10-26: a
  // typical track's level, or as it is). In the app its own player's, here this browser's. The same rows in This
  // device's Playing (lib/settings/DevicePart) and in the sheet a long press on the player's Level button opens
  // (lib/LevelSheet). Whether the waveform shows it: this page's own (the owner, 2026-10-27).
  import { levelOn, levelUnknown, playingLevelOn, playingLevelUnknown, setLevelOn, setLevelUnknown, setPlayingLevelOn, setPlayingLevelUnknown } from '../level.svelte';
  import { prefs } from '../prefs.svelte';
  import Seg from '../Seg.svelte';
  import Item from './Item.svelte';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuSheet from '../MenuSheet.svelte';

  /** `playing`: the player that plays (the player's Level sheet: a driven player, a group), else this device's own
   *  (This device's settings). */
  let { playing = false }: { playing?: boolean } = $props();
  const on = $derived(playing ? playingLevelOn() : levelOn());
  const unknown = $derived(playing ? playingLevelUnknown() : levelUnknown());
  const setOn = (v: boolean) => (playing ? setPlayingLevelOn(v) : setLevelOn(v));
  const setUnknown = (v: 'typical' | 'untouched') => (playing ? setPlayingLevelUnknown(v) : setLevelUnknown(v));
  const UNKNOWN: { value: 'typical' | 'untouched'; label: string }[] = [
    { value: 'typical', label: 'Typical' },
    { value: 'untouched', label: 'As is' },
  ];
  /** The player's waveform and the levelling (the owner, 2026-10-27): a sheet of three. */
  const WAVES: { value: 'file' | 'difference' | 'output'; label: string; line: string }[] = [
    { value: 'file', label: 'On file', line: 'The track as made; the levelling not shown' },
    { value: 'difference', label: 'Difference', line: 'As made, the playing colour only as high as you hear it' },
    { value: 'output', label: 'Output', line: 'As you hear it: the whole lowered' },
  ];
  let waveOpen = $state(false);
  function chooseWave(v: 'file' | 'difference' | 'output') {
    prefs.waveLevel = v;
    waveOpen = false;
  }
</script>

<Item
  name="Level the volume"
  line="Every track about as loud as the next"
  more={"Each track plays at about the same loudness, as Cratarr measured it (or as the file's ReplayGain tags say, until it has). It only ever turns a track down.\n\nAn album played through keeps its own dynamics; shuffle and radio are levelled track by track."}
  toggle={{ checked: on, onchange: setOn }}
/>
{#if on}
  <Item
    name="Not measured yet"
    more={"A track Cratarr has not measured yet: turned down as a typical track is, or played as it is until it has been."}
    ><Seg label="Not measured yet" options={UNKNOWN} bind:value={() => unknown, setUnknown} /></Item
  >
  <Item name="Waveform" value={WAVES.find((w) => w.value === prefs.waveLevel)?.label ?? ''} onclick={() => (waveOpen = true)} />
{/if}

{#if waveOpen}
  <MenuSheet label="Waveform" title="Waveform" onclose={() => (waveOpen = false)}>
    {#each WAVES as w (w.value)}<MenuEntry icon="audioLines" label={w.label} sub={w.line} checked={prefs.waveLevel === w.value} plus={false} onclick={() => chooseWave(w.value)} />{/each}
  </MenuSheet>
{/if}
