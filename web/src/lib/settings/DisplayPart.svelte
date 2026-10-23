<script lang="ts">
  // Display: this browser's own look; acts at once. Its Reset: the theme, the progress bar, levelling and the player's
  // row back to their defaults (lib/settings/reset). The player's row (the owner, 2026-10-19): shown or hidden, and the
  // facts beside its info button (lib/settings/PlayerRowList). Profile (the owner, 2026-10-23): the lists' profiles
  // (lib/ListProfile), open or folded for every list, the row a folded one keeps, and each row on or off; and the
  // artists line under a playlist's or a crate's genres.
  import Seg from '../Seg.svelte';
  import { appInfo, inApp, setDevice } from '../app.svelte';
  import { setLevelling } from '../localPlayer.svelte';
  import { PROFILE_ROWS, prefs, type Theme } from '../prefs.svelte';
  import OrderList from './OrderList.svelte';
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
    () => "Theme, progress bar, levelling, the player's row and the profiles back to their defaults.",
    () => {
      prefs.theme = 'auto';
      prefs.showProgress = true;
      prefs.playerRow = true;
      prefs.playerFacts = [];
      prefs.profileOpen = true;
      prefs.profileFoldAt = 0;
      prefs.profileRows = PROFILE_ROWS.map((r) => r.id);
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

<Group title="Profile">
  <!-- The rows in their order, the fold line among them: above it stays when a profile is folded. -->
  <OrderList
    items={PROFILE_ROWS}
    on={prefs.profileRows}
    label="The profile's rows"
    onchange={(v) => (prefs.profileRows = v)}
    fold={{ at: Math.min(prefs.profileFoldAt, prefs.profileRows.length), onchange: (at) => (prefs.profileFoldAt = at) }}
  />
  <Item name="Artists" line="Under the genres of playlists and crates" toggle={{ checked: prefs.listArtists, onchange: (on) => (prefs.listArtists = on) }} />
</Group>
