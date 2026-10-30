<script lang="ts">
  // Layout (the owner, 2026-10-25: Display's concerns apart): where things sit below 1024 px, where the bar of pages
  // runs along an edge: the bar's edge and when it slides away, the sticky action bar, the mini-player's edge, and where
  // each of the two kinds of controls floats (App reads them all).
  import { glass } from '../glass.svelte';
  import { prefs } from '../prefs.svelte';
  import Seg from '../Seg.svelte';
  import Group from './Group.svelte';
  import Item from './Item.svelte';
  import { offerReset } from './reset.svelte';

  const EDGES: { value: 'top' | 'bottom'; label: string }[] = [
    { value: 'top', label: 'Top' },
    { value: 'bottom', label: 'Bottom' },
  ];

  // From 1024 px the pages are in the sidebar on the left (App: one glass only below it): where the bar, the mini-player
  // and the controls sit along an edge does not apply, so those rows are dimmed, each its (i) saying why (the owner,
  // 2026-10-25). The sticky action bar applies at any width.
  const side = $derived(!glass.one);
  const WHY = 'With a window this wide the pages are in the sidebar on the left, so this applies only to a narrower window (under 1024 px), where the bar of pages runs along the top or the bottom.';
  const why = $derived(side ? WHY : null);

  offerReset(
    () => 'The bar of pages and the controls along the top, the mini-player along the bottom, the action bar sticking.',
    () => {
      Object.assign(prefs, { navAt: 'top', navHides: false, navHidesOnItem: true, stickyActions: true, miniAt: 'bottom', navControlsAt: 'top', contentControlsAt: 'top' });
    },
  );
</script>

<!-- Where the bar of pages runs along an edge (below 1024 px; the owner, 2026-10-25): which edge, whether it slides
     away while scrolling (panels open under it either way); the mini-player's edge; where each of the two kinds of
     controls floats. Along an edge, from it inwards: the bar, the mini-player, the controls. -->
<Group>
  <Item name="Navigation bar" more={why} disabled={side}><Seg label="Navigation bar" options={EDGES} bind:value={prefs.navAt} /></Item>
  <Item
    name="Hide when opening a library item"
    line={prefs.navAt === 'bottom' ? 'Not along the bottom' : null}
    more={why}
    disabled={side || prefs.navAt === 'bottom'}
    toggle={{ checked: prefs.navHidesOnItem && prefs.navAt !== 'bottom', onchange: (on) => (prefs.navHidesOnItem = on) }}
  />
  <!-- Along the bottom it never slides away (the owner, 2026-10-25). -->
  <Item
    name="Hide when scrolling"
    line={prefs.navAt === 'bottom' ? 'Not along the bottom' : null}
    more={why}
    disabled={side || prefs.navAt === 'bottom'}
    toggle={{ checked: prefs.navHides && prefs.navAt !== 'bottom', onchange: (on) => (prefs.navHides = on) }}
  />
  <Item
    name="Sticky action bar"
    line="Play buttons stay in sight"
    more="On, the action bar stays at the top as you scroll a playlist, an album or an artist, and the navigation bar slides away while you scroll down there. Off, the action bar scrolls away with the rest."
    toggle={{ checked: prefs.stickyActions, onchange: (on) => (prefs.stickyActions = on) }}
  />
  <Item name="Mini player" more={why} disabled={side}><Seg label="Mini player" options={EDGES} bind:value={prefs.miniAt} /></Item>
  <Item name="Navigation controls" line="Back, Home and the title" more={why} disabled={side}><Seg label="Navigation controls" options={EDGES} bind:value={prefs.navControlsAt} /></Item>
  <Item name="Content controls" line="The search, the filter and View" more={why} disabled={side}><Seg label="Content controls" options={EDGES} bind:value={prefs.contentControlsAt} /></Item>
</Group>
