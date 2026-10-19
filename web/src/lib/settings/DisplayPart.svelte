<script lang="ts">
  // Display: this browser's own look; acts at once. Its Reset: the theme, the progress bar, levelling and the player's
  // row back to their defaults (lib/settings/reset). The player's row (the owner, 2026-10-19): shown or hidden, and the
  // facts beside its info button (lib/settings/PlayerRowList).
  import Seg from '../Seg.svelte';
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import { setLevelling } from '../localPlayer.svelte';
  import { prefs, type Theme } from '../prefs.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import PlayerRowList from './PlayerRowList.svelte';
  import { offerReset } from './reset.svelte';

  const THEMES: { value: Theme; label: string }[] = [
    { value: 'auto', label: 'System' },
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' },
  ];

  const setLevel = (on: boolean) => (inApp ? setDevice({ levelling: on }) : setLevelling(on));
  offerReset(
    () => "Theme, progress bar, levelling and the player's row back to their defaults.",
    () => {
      prefs.theme = 'auto';
      prefs.showProgress = true;
      prefs.playerRow = true;
      prefs.playerFacts = [];
      setLevel(true);
    },
  );
</script>

<!-- Where it is kept is the group's name: this browser (in the app, this device). -->
<Group title={inApp ? 'This device' : 'This browser'}>
  <Item name="Theme"><Seg label="Theme" options={THEMES} bind:value={prefs.theme} /></Item>
  <Item name="Progress bar under the playing track" toggle={{ checked: prefs.showProgress, onchange: (on) => (prefs.showProgress = on) }} />
  <!-- Levelling (decision 0035): in the app its own player's, here this browser's. -->
  <Item
    name="Level the volume"
    line="Every track about as loud as the next"
    more={"Each track plays at about the same loudness, as Cratarr measured it (or as the file's ReplayGain tags say, until it has).\n\nAn album played through keeps its own dynamics; shuffle and radio are levelled track by track."}
    toggle={{ checked: inApp ? appInfo.levelling : prefs.levelling, onchange: setLevel }}
  />
</Group>

<Group title="Player row">
  <Item name="Row under the title" line="The info button and the facts below" toggle={{ checked: prefs.playerRow, onchange: (on) => (prefs.playerRow = on) }} />
  {#if prefs.playerRow}<PlayerRowList />{/if}
</Group>
