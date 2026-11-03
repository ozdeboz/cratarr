<script lang="ts">
  // An Insights view in the panel (the owner, 2026-10-24: Insights as Settings and Health): opened from the page's list,
  // beside it on a roomy window (no ✕ there), over it on a phone. Its bar: where it is (Collection › Overview) and the
  // pill the pages have (lib/ContentControls: the same glass, the same motion; the owner, 2026-10-25): its funnel opening what is
  // counted, its sliders how it is shown (lib/InsightsSheet), each where the view has any. The views are the tabs' as they
  // were (Collection's sections, Listening's History and Profile, the Map).
  import InsightsCollection from './InsightsCollection.svelte';
  import { rowRoot } from './glassRow.svelte';
  import DiscoveryList from './DiscoveryList.svelte';
  import InsightsListening from './InsightsListening.svelte';
  import InsightsMap from './InsightsMap.svelte';
  import InsightsProfile from './InsightsProfile.svelte';
  import InsightsSheet from './InsightsSheet.svelte';
  import { insightsFilter, insightsView } from './insightsControls.svelte';
  import ContentControls from './ContentControls.svelte';
  import { INSIGHTS_VIEWS, showView, type InsightsView } from './insightsViews';
  import { insights, mapFull } from './insights.svelte';
  import PageLoading from './PageLoading.svelte';
  import { backOne, INSIGHTS, isStep, upToPage, viewPlaylist, viewSibling } from './panel.svelte';
  import Crumbs, { type Level, type Siblings } from './Crumbs.svelte';
  import { groupsOf, landingOf, remembered } from './groupCrumb';
  import { prefs } from './prefs.svelte';
  import { glass, handGlass } from './glass.svelte';
  import GlobalResults from './GlobalResults.svelte';
  import { everywhere } from './globalSearch.svelte';
  import { matchesSearch, pageView, searchElsewhere } from './pageView.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';

  let { view, beside = false }: { view: InsightsView; beside?: boolean } = $props();

  // The page's search through every view (the owner, 2026-11-02, as Settings' parts): under this view, the other
  // views the words name; a tap opens one, the words still searched.
  const otherViews = $derived(pageView.query.trim() ? INSIGHTS_VIEWS.filter((x) => x.id !== view && !x.under && matchesSearch(x.label, x.group)) : []);
  searchElsewhere(() => otherViews.length > 0);
  // The views and their controls read the tab and its section: set before they draw.
  $effect.pre(() => showView(view));
  $effect(() => insights.load());
  const v = $derived(INSIGHTS_VIEWS.find((x) => x.id === view) ?? INSIGHTS_VIEWS[0]);
  const filters = $derived(insightsFilter());
  const views = $derived(insightsView());
  // The funnel is lit while it leaves something out: a library or a source switched off.
  const leftOut = $derived(filters.some((g) => g.controls.some((c) => c.kind === 'toggle' && !c.get())));
  let sheet = $state<'filter' | 'view' | null>(null);
  // On a phone this panel's glass is App's (lib/glass, the owner 2026-10-25: one glass, page's and panel's): handed over,
  // its own bar not drawn, its view under the glass.
  const handed = $derived(glass.one && !beside);
  const pill = () =>
    filters.length || views.length || !beside || view === 'map'
      ? {
          filter: filters.length > 0,
          view: views.length > 0,
          filterLit: leftOut,
          onfilter: () => (sheet = 'filter'),
          onview: () => (sheet = 'view'),
          search: beside ? null : 'insights',
          // The Map alone has a full screen (the owner, 2026-10-27).
          action: view === 'map' ? { icon: 'fullscreen' as const, label: 'Full screen', run: () => (mapFull.on = true) } : null,
        }
      : null;
  handGlass(() => handed, { left: glassLeft, pill });

  // Its group as a level above, a picker of the groups (lib/Crumbs, lib/groupCrumb; the owner, 2026-11-02: one picker of
  // every view was "way too tall"), landing on the view last seen in one, else its first; the view's own name a picker
  // of its group's other views. A view under another is seen as that one.
  const parts = INSIGHTS_VIEWS.filter((x) => !x.under).map((x) => ({ id: x.id as string, group: x.group as string, label: x.label }));
  const groupLevel = $derived<Level>({
    label: v.group,
    pick: {
      menuLabel: 'Insights groups',
      options: groupsOf(parts).map((g) => ({ id: g, label: g })),
      chosen: v.group,
      onchoose: (g) => {
        const to = g === v.group ? null : landingOf(parts, g, prefs.groupLast[`insights/${g}`]);
        if (to) viewSibling(INSIGHTS + to);
      },
    },
  });
  const siblings = $derived.by<Siblings | null>(() => {
    const own = parts.filter((x) => x.group === v.group);
    return beside || v.under || own.length < 2 ? null : { menuLabel: `${v.group} shows`, options: own.map((x) => ({ id: x.id, label: x.label })), chosen: v.id, onchoose: (id) => viewSibling(INSIGHTS + id) };
  });
  $effect(() => {
    prefs.groupLast = remembered(prefs.groupLast, 'insights', v.group, v.under ?? v.id);
  });
</script>

<!-- This panel's glass: its own bar's parts, or App's on a phone (lib/glass). -->
{#snippet glassLeft()}
  {@const parent = v.under ? INSIGHTS_VIEWS.find((x) => x.id === v.under) : null}
  <!-- The levels Back walks through (lib/Crumbs): Insights, the view a deeper one was opened from; between them its
       group, a heading of the page's list and not a place: a picker of the groups (the owner, 2026-11-02). On a phone
       the view's name picks another view of its group. -->
  <Crumbs
    back={beside ? null : backOne}
    trail={[
      ...(beside ? [] : [{ label: 'Insights', go: () => upToPage('insights') }, groupLevel]),
      ...(v.under && isStep() ? [{ label: parent?.label ?? v.group, go: backOne }] : []),
    ]}
    here={v.label}
    {siblings}
  />
{/snippet}

<div class="view" class:handed>
  {#if !handed}
    <div class="bar" {@attach rowRoot()} class:beside>
      {@render glassLeft()}
      {#if pill()}<ContentControls {...pill()!} />{/if}
    </div>
  {/if}
  <div class="scroll" class:away={handed && everywhere()}>
    {#if view === 'discover-missing' || view === 'discover-excluded'}
      <DiscoveryList kind={view === 'discover-missing' ? 'missing' : 'excluded'} />
    {:else if view === 'history'}
      <InsightsListening />
    {:else if view === 'profile'}
      <InsightsProfile />
    {:else if view === 'map'}
      <InsightsMap />
    {:else if insights.error}
      <p class="error" role="alert">{insights.error}</p>
    {:else if !insights.data}
      <PageLoading />
    {:else}
      <InsightsCollection data={insights.data} />
    {/if}
    {#if otherViews.length}
      <!-- The other views the search names (lib/settings/Group, Item). -->
      <Group title="Other views">
        {#each otherViews as o (o.id)}<Item name={o.label} line={o.group} onclick={() => viewSibling(INSIGHTS + o.id)} />{/each}
      </Group>
    {/if}
      <!-- On a phone, over the page: the global results at the end of this list (decision 0072). -->
    {#if handed && everywhere()}<GlobalResults />{/if}
  </div>
</div>

{#if sheet}<InsightsSheet title={`${v.group} › ${v.label}`} part={sheet} onclose={() => (sheet = null)} />{/if}

<style>
  /* As a part of the settings in the panel (lib/SettingsPanel): its bar on top, what it holds under it. */
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 65px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .bar.beside {
    padding-left: clamp(12px, 2vw, 24px);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px clamp(12px, 2vw, 24px) calc(var(--mini-h, 0px) + 16px);
  }

  .error {
    color: var(--danger-ink);
  }

  /* Its glass handed to App's row on a phone (lib/glass): the view starts under it, its first heading where the page's
     is (the glass's room, then its own 16 px; it began 12 px higher, the owner 2026-10-25). */
  .view.handed .scroll {
    padding-top: calc(var(--bar-room, 0px) + var(--mini-top, 0px) + var(--glass-top) + 16px);
    padding-bottom: calc(var(--mini-h, 0px) + var(--glass-bottom) + 16px);
  }
</style>
