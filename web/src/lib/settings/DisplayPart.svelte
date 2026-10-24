<script lang="ts">
  // Display: this browser's own look; acts at once. Its Reset: the theme and the player's row back to their defaults
  // (lib/settings/reset). Levelling went to This device (the owner, 2026-10-24: a play affair, not a display one). The
  // player's row (the owner, 2026-10-19): shown or hidden, and the facts beside its info button
  // (lib/settings/PlayerRowList). Profile (the owner, 2026-10-23): the lists' profiles (lib/ListProfile), open or folded
  // for every list, the row a folded one keeps, and each row on or off; under Content (the owner, 2026-10-24), switched
  // off as a whole with Music profiles. A list's Typical artists always show where it has
  // them (the owner, 2026-10-24: no switch).
  import Seg from '../Seg.svelte';
  import BandSlider from '../BandSlider.svelte';
  import { BAND_DIRECTIONS, BAND_ENDS, BAND_LOOKS, BANDS_MORE, COUNT_MORE, WINDOW_MORE } from '../bandWaves.svelte';
  import { inApp } from '../app.svelte';
  import { BAND_COUNT, BAND_CYCLES, BAND_GAP, BAND_HEIGHT, BAND_PACE, BAND_WINDOW, PROFILE_ROWS, prefs, type Theme } from '../prefs.svelte';
  import OrderList from './OrderList.svelte';
  import OrderReset from './OrderReset.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import PlayerRowList from './PlayerRowList.svelte';
  import { offerReset } from './reset.svelte';

  const THEMES: { value: Theme; label: string }[] = [
    { value: 'auto', label: 'System' },
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' },
  ];


  offerReset(
    () => "Theme, the player's extra details and the profiles back to their defaults.",
    () => {
      prefs.theme = 'auto';
      prefs.playerRow = true;
      prefs.playerFacts = [];
      prefs.profileOpen = true;
      prefs.profile = true;
      prefs.profileFoldAt = 0;
      prefs.profileRows = PROFILE_ROWS.map((r) => r.id);
    },
  );
</script>

<!-- Where it is kept is the group's name: this browser (in the app, this device). -->
<Group title={inApp ? 'This device' : 'This browser'}>
  <Item name="Theme"><Seg label="Theme" options={THEMES} bind:value={prefs.theme} /></Item>
</Group>

<Group title="Player details">
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
<Group title="Player visualisation">
  <Item
    name="Bands"
    more={BANDS_MORE}
    ><Seg label="Bands" options={BAND_LOOKS} bind:value={prefs.playerBands} /></Item
  >
  {#if prefs.playerBands !== 'off'}
    <Item name="Bands" more={COUNT_MORE}><BandSlider label="Bands" range={BAND_COUNT} bind:value={prefs.playerBandsCount} ends={['3', '32']} whole /></Item>
    <Item name="Speed"><BandSlider label="Speed" range={BAND_PACE} bind:value={prefs.playerBandsSpeed} ends={['Calm', 'Radical']} /></Item>
    <Item
      name="Window"
      more={WINDOW_MORE}
      ><BandSlider label="Window" range={BAND_WINDOW} bind:value={prefs.playerBandsWindow} ends={['Beat', 'Phrase']} unit="s" /></Item
    >
    <Item name="Cycles"><BandSlider label="Cycles" range={BAND_CYCLES} bind:value={prefs.playerBandsCycles} ends={['Few', 'Many']} /></Item>
    <Item name="Height"><BandSlider label="Height" range={BAND_HEIGHT} bind:value={prefs.playerBandsHeight} ends={['Low', 'Tall']} /></Item>
    <Item name="Gap"><BandSlider label="Gap" range={BAND_GAP} bind:value={prefs.playerBandsGap} ends={['None', 'Wide']} linear /></Item>
    <Item name="Direction"><Seg label="Direction" options={BAND_DIRECTIONS} bind:value={prefs.playerBandsDirection} /></Item>
    <Item name="Ends"><Seg label="Ends" options={BAND_ENDS} bind:value={prefs.playerBandsEnds} /></Item>
  {/if}
</Group>

<Group title="Content">
  {#snippet tail()}
    {#if prefs.profile}<OrderReset
        label="Profile rows"
        isDefault={prefs.profileRows.join() === PROFILE_ROWS.map((r) => r.id).join() && prefs.profileFoldAt === 0}
        reset={() => {
          const was = { rows: prefs.profileRows, at: prefs.profileFoldAt };
          prefs.profileRows = PROFILE_ROWS.map((r) => r.id);
          prefs.profileFoldAt = 0;
          return () => ((prefs.profileRows = was.rows), (prefs.profileFoldAt = was.at));
        }}
      />{/if}
  {/snippet}
  <Item
    name="Music profiles"
    more={"A playlist's, a crate's or an artist's music at a glance, at the top of its page: its tempo, energy and danceability, its era, its styles, and its moods against your whole library.\n\nMost of it comes from AudioMuse, which listens to every track in Cratarr's library: tracks it has not heard yet count only by their tags (Health shows how far it is).\n\nBelow, choose which rows a profile shows and in what order; the rows above the line stay when one is folded."}
    toggle={{ checked: prefs.profile, onchange: (on) => (prefs.profile = on) }}
  />
  <!-- The rows in their order, the fold line among them: above it stays when a profile is folded. -->
  {#if prefs.profile}<OrderList
    items={PROFILE_ROWS}
    on={prefs.profileRows}
    label="The profile's rows"
    onchange={(v) => (prefs.profileRows = v)}
    fold={{ at: Math.min(prefs.profileFoldAt, prefs.profileRows.length), onchange: (at) => (prefs.profileFoldAt = at) }}
  />{/if}
</Group>
