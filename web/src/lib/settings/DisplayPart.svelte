<script lang="ts">
  // Display: this browser's own look; acts at once. Its Reset: the theme and the player's row back to their defaults
  // (lib/settings/reset). Levelling went to This device (the owner, 2026-10-24: a play affair, not a display one). The
  // player's row (the owner, 2026-10-19): shown or hidden, and the facts beside its info button
  // (lib/settings/PlayerRowList). Profile (the owner, 2026-10-23): the lists' profiles (lib/ListProfile), open or folded
  // for every list, the row a folded one keeps, and each row on or off; under Content (the owner, 2026-10-24), switched
  // off as a whole with Music profiles. A list's Typical artists always show where it has
  // them (the owner, 2026-10-24: no switch).
  import BandControls from '../BandControls.svelte';
  import { inApp } from '../app.svelte';
  import { PROFILE_ROWS, prefs } from '../prefs.svelte';
  import AppearancePicker from './AppearancePicker.svelte';
  import OrderList from './OrderList.svelte';
  import OrderReset from './OrderReset.svelte';
  import Group from './Group.svelte';
  import Seg from '../Seg.svelte';
  import Item from './Item.svelte';
  import PlayerRowList from './PlayerRowList.svelte';
  import { offerReset } from './reset.svelte';



  const EDGES: { value: 'top' | 'bottom'; label: string }[] = [
    { value: 'top', label: 'Top' },
    { value: 'bottom', label: 'Bottom' },
  ];

  offerReset(
    () => "Theme, animations, the layout, the player's extra details and the profiles back to their defaults.",
    () => {
      prefs.theme = 'auto';
      prefs.animations = true;
      prefs.navAt = 'top';
      prefs.navHides = false;
      prefs.stickyActions = true;
      prefs.miniAt = 'bottom';
      prefs.navControlsAt = 'top';
      prefs.contentControlsAt = 'top';
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
  <!-- Light or dark as two pictures, Automatic under them (the owner, 2026-10-25: Apple's way, mockup T3). Automatic
       switched off keeps the look the system gave. -->
  <AppearancePicker />
  <Item
    name="Automatic"
    line="Follows the system"
    toggle={{
      checked: prefs.theme === 'auto',
      onchange: (on) => (prefs.theme = on ? 'auto' : matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'),
    }}
  />
  <Item name="Animations" toggle={{ checked: prefs.animations, onchange: (on) => (prefs.animations = on) }} />
</Group>

<!-- Where the bar of pages runs along an edge (below 1024 px; the owner, 2026-10-25): which edge, whether it slides
     away while scrolling (panels open under it either way); the mini-player's edge; where each of the two kinds of
     controls floats. Along an edge, from it inwards: the bar, the mini-player, the controls. -->
<Group title="Layout" more="On a phone or a narrow window, where the bar of pages runs along an edge of the screen.">
  {#snippet tail()}
    <OrderReset
      label="Layout"
      isDefault={prefs.navAt === 'top' && !prefs.navHides && prefs.stickyActions && prefs.miniAt === 'bottom' && prefs.navControlsAt === 'top' && prefs.contentControlsAt === 'top'}
      reset={() => {
        const was = { navAt: prefs.navAt, navHides: prefs.navHides, stickyActions: prefs.stickyActions, miniAt: prefs.miniAt, navControlsAt: prefs.navControlsAt, contentControlsAt: prefs.contentControlsAt };
        Object.assign(prefs, { navAt: 'top', navHides: false, stickyActions: true, miniAt: 'bottom', navControlsAt: 'top', contentControlsAt: 'top' });
        return () => Object.assign(prefs, was);
      }}
    />
  {/snippet}
  <Item name="Navigation bar"><Seg label="Navigation bar" options={EDGES} bind:value={prefs.navAt} /></Item>
  <!-- Along the bottom it never slides away (the owner, 2026-10-25). -->
  <Item
    name="Hide when scrolling"
    line={prefs.navAt === 'bottom' ? 'Not along the bottom' : null}
    disabled={prefs.navAt === 'bottom'}
    toggle={{ checked: prefs.navHides && prefs.navAt !== 'bottom', onchange: (on) => (prefs.navHides = on) }}
  />
  <Item
    name="Sticky action bar"
    line="Play buttons stay in sight"
    more="On, the action bar stays at the top as you scroll a playlist, an album or an artist, and the navigation bar slides away while you scroll down there. Off, the action bar scrolls away with the rest."
    toggle={{ checked: prefs.stickyActions, onchange: (on) => (prefs.stickyActions = on) }}
  />
  <Item name="Mini player"><Seg label="Mini player" options={EDGES} bind:value={prefs.miniAt} /></Item>
  <Item name="Navigation controls" line="Back, Home and the title"><Seg label="Navigation controls" options={EDGES} bind:value={prefs.navControlsAt} /></Item>
  <Item name="Content controls" line="The search, the filter and View"><Seg label="Content controls" options={EDGES} bind:value={prefs.contentControlsAt} /></Item>
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
<!-- The bands: the same controls as the player's own sheet (lib/BandControls). -->
<Group title="Player visualisation">
  <div class="bands"><BandControls /></div>
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

<style>
  .bands {
    padding: 12px 16px 16px;
  }
</style>
