<script lang="ts">
  // Display: this browser's own look; acts at once. Its Reset: the theme and the player's row back to their defaults
  // (lib/settings/reset). Levelling went to This device (the owner, 2026-10-24: a play affair, not a display one). The
  // player's row (the owner, 2026-10-19): shown or hidden, and the facts beside its info button
  // (lib/settings/PlayerRowList). Profile (the owner, 2026-10-23): the lists' profiles (lib/ListProfile), open or folded
  // for every list, the row a folded one keeps, and each row on or off. A list's Typical artists always show where it has
  // them (the owner, 2026-10-24: no switch).
  import Seg from '../Seg.svelte';
  import { inApp } from '../app.svelte';
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


  offerReset(
    () => "Theme, the player's row and the profiles back to their defaults.",
    () => {
      prefs.theme = 'auto';
      prefs.playerRow = true;
      prefs.playerFacts = [];
      prefs.profileOpen = true;
      prefs.profileFoldAt = 0;
      prefs.profileRows = PROFILE_ROWS.map((r) => r.id);
    },
  );
</script>

<!-- Where it is kept is the group's name: this browser (in the app, this device). -->
<Group title={inApp ? 'This device' : 'This browser'}>
  <Item name="Theme"><Seg label="Theme" options={THEMES} bind:value={prefs.theme} /></Item>
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
</Group>
