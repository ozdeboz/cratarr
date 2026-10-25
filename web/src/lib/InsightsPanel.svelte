<script lang="ts">
  // An Insights view in the panel (the owner, 2026-10-24: Insights as Settings and Health): opened from the page's list,
  // beside it on a roomy window (no ✕ there), over it on a phone. Its bar: where it is (Collection › Overview) and the
  // pill the pages have (lib/TopPill: the same glass, the same motion; the owner, 2026-10-25): its funnel opening what is
  // counted, its sliders how it is shown (lib/InsightsSheet), each where the view has any. The views are the tabs' as they
  // were (Collection's sections, Listening's History and Profile, the Map).
  import Icon from './Icon.svelte';
  import InsightsCollection from './InsightsCollection.svelte';
  import InsightsListening from './InsightsListening.svelte';
  import InsightsMap from './InsightsMap.svelte';
  import InsightsProfile from './InsightsProfile.svelte';
  import InsightsSheet from './InsightsSheet.svelte';
  import { insightsFilter, insightsView } from './insightsControls.svelte';
  import TopPill from './TopPill.svelte';
  import { INSIGHTS_VIEWS, showView, type InsightsView } from './insightsViews';
  import { insights } from './insights.svelte';
  import PageLoading from './PageLoading.svelte';
  import { closePlaylist, upToPage } from './panel.svelte';
  import Crumbs from './Crumbs.svelte';

  let { view, beside = false }: { view: InsightsView; beside?: boolean } = $props();

  // The views and their controls read the tab and its section: set before they draw.
  $effect.pre(() => showView(view));
  $effect(() => insights.load());
  const v = $derived(INSIGHTS_VIEWS.find((x) => x.id === view) ?? INSIGHTS_VIEWS[0]);
  const filters = $derived(insightsFilter());
  const views = $derived(insightsView());
  // The funnel is lit while it leaves something out: a library or a source switched off.
  const leftOut = $derived(filters.some((g) => g.controls.some((c) => c.kind === 'toggle' && !c.get())));
  let sheet = $state<'filter' | 'view' | null>(null);
</script>

<div class="view">
  <div class="bar" class:beside>
    {#if !beside}<span class="navpill"><button aria-label="Close" title="Close" onclick={() => closePlaylist()}><Icon name="close" size={22} /></button></span>{/if}
    <Crumbs up={{ label: v.group, go: beside ? undefined : () => upToPage('#/insights') }} here={v.label} />
    {#if filters.length || views.length || !beside}
      <TopPill
        device={false}
        filter={filters.length > 0}
        view={views.length > 0}
        filterLit={leftOut}
        onfilter={() => (sheet = 'filter')}
        onview={() => (sheet = 'view')}
        search={beside ? null : 'insights'}
      />
    {/if}
  </div>
  <div class="scroll">
    {#if view === 'history'}
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

  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--text);
    cursor: pointer;
  }
</style>
