<script lang="ts">
  // A part of the settings in the panel (decided 2026-10-16: "the settings page should really be a portal with cards that
  // open settings-specific panels"): the Settings page's row opens it, as a playlist opens. Beside the page's list on a
  // roomy window (`beside`, the owner 2026-10-19) a part is always open: no ✕ and no place to choose there.
  // A part is groups of rows (lib/settings/Group, Item; the settings' North Star, 2026-10-19); a thing in it (a server, a
  // library, a Kodi, a user) has its own page here, behind its row (`settings:<part>/<thing>`): back, not ✕, and the
  // thing's name for a title (`thingTitle`, which the part sets). Each part is its own file (lib/settings/*Part) over
  // one editor (lib/settings/editor), so what is changed in one part shows the Save bar in all of them and survives
  // going from one to another. Authentication's account, Import, this device's settings and Display act at once. The
  // server's parts are its administrators' (decision 0051): for anyone else they show nothing, and the server's
  // settings are not asked for.
  import { deviceName } from './playOnTarget.svelte';
  import { glass, handGlass } from './glass.svelte';
  import GlobalResults from './GlobalResults.svelte';
  import { everywhere } from './globalSearch.svelte';
  import { untrack } from 'svelte';
  import { narrowSettings } from './settings/narrow';
  import { matchesSearch, pageView } from './pageView.svelte';
  import { words } from './search';
  import Crumbs from './Crumbs.svelte';
  import Icon from './Icon.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import Notice from './Notice.svelte';
  import PageLoading from './PageLoading.svelte';
  import PanelPlace from './PanelPlace.svelte';
  import { askDevice, askDownloads, inApp } from './app.svelte';
  import { backFromThing, closePlaylist, panel, settingsThing, upToPage } from './panel.svelte';
  import { SECTIONS, thingTitle, type SettingsSection } from './settingsSections.svelte';
  import { ed } from './settings/editor.svelte';
  import { partReset } from './settings/reset.svelte';
  import SaveBar from './settings/SaveBar.svelte';
  import AccountPart from './settings/AccountPart.svelte';
  import AuthPart from './settings/AuthPart.svelte';
  import UsersPart from './settings/UsersPart.svelte';
  import { me } from './me.svelte';
  import ConnectionsPart from './settings/ConnectionsPart.svelte';
  import DevicePart from './settings/DevicePart.svelte';
  import AppearancePart from './settings/AppearancePart.svelte';
  import LayoutPart from './settings/LayoutPart.svelte';
  import PlayerPart from './settings/PlayerPart.svelte';
  import TastePart from './settings/TastePart.svelte';
  import RadioPart from './settings/RadioPart.svelte';
  import TrackingPart from './settings/TrackingPart.svelte';
  import ContentPart from './settings/ContentPart.svelte';
  import NetworkPart from './settings/NetworkPart.svelte';
  import SyncPart from './settings/SyncPart.svelte';
  import ImportPart from './settings/ImportPart.svelte';
  import ListeningPart from './settings/ListeningPart.svelte';
  import LibrariesPart from './settings/LibrariesPart.svelte';
  import DiagnosticsPart from './settings/DiagnosticsPart.svelte';
  import PlayersSettings from './PlayersSettings.svelte';

  let { section, beside = false }: { section: SettingsSection; beside?: boolean } = $props();
  const thing = $derived(settingsThing(panel.viewed));
  // The part's name, as its row on the Settings page has it; on a thing's page the thing's.
  const partTitle = $derived(section === 'device' ? deviceName() : (SECTIONS.find((x) => x.id === section)?.title ?? 'Settings'));
  const title = $derived(thing ? (thingTitle.text ?? '') : partTitle);

  // Inside the Android app: its settings as they are now (Android's background permission may have changed meanwhile).
  if (inApp) {
    askDevice();
    askDownloads();
  }
  // Loaded when an administrator opens it, depending on that only: the load reads the editor's state, which its answer
  // changes, and tracked that asked again with every answer, without end (a runaway client, 2026-10-23).
  $effect(() => {
    if (me.admin) untrack(() => ed.load());
  });
  const allowed = $derived.by(() => {
    const x = SECTIONS.find((x) => x.id === section);
    return !x?.admin || me.admin || !!x.everyone;
  });

  // Another page (a part, a thing, back) starts at its top.
  let scrollEl = $state<HTMLElement>();
  $effect(() => {
    void panel.viewed;
    if (scrollEl) scrollEl.scrollTop = 0;
  });

  // A part's Reset (lib/settings/reset): asked once more, in a sheet, before its preferences go back to their defaults.
  let resetting = $state(false);
  let resetBusy = $state(false);
  async function reset() {
    resetBusy = true;
    try {
      await partReset.run?.();
      resetting = false;
    } finally {
      resetBusy = false;
    }
  }
  // On a phone this panel's glass is App's (lib/glass, the owner 2026-10-25: one glass, page's and panel's): handed over,
  // its own bar not drawn, its list under the glass.
  const handed = $derived(glass.one && !beside);
  // Its own search there, as every panel's (the owner, 2026-10-25): the part's rows narrowed to it (lib/settings/narrow),
  // all of them when the part's name has the words; a thing's page has none.
  const narrowing = $derived(handed && !thing && words(pageView.query).length > 0 && !matchesSearch(partTitle));
  narrowSettings(() => narrowing);
  const pill = () => (!thing ? { device: false, filter: false, view: false, search: 'settings' } : null);
  handGlass(() => handed, { left: glassLeft, pill, extra: glassExtra });
</script>

<!-- This panel's glass: its own bar's parts, or App's on a phone (lib/glass). -->
{#snippet glassLeft()}
  {#if thing}
    <span class="navpill"><button aria-label="Back" title="Back" onclick={() => backFromThing(section)}><Icon name="back" size={22} /></button></span>
  {:else if !beside}
    <span class="navpill"><button aria-label="Back" title="Back" onclick={() => closePlaylist()}><Icon name="back" size={22} /></button></span>
  {/if}
  <!-- Settings › the part; a thing's page: its part › its name (lib/Crumbs: two at most, the owner 2026-10-25). -->
  {@const top = beside ? [] : [{ label: 'Settings', go: () => upToPage('#/settings') }]}
  {#if thing}
    <Crumbs trail={[...top, { label: partTitle, go: () => backFromThing(section) }]} here={title} />
  {:else}
    <Crumbs trail={top} here={partTitle} />
  {/if}
{/snippet}
{#snippet glassExtra()}
  <!-- Diagnostics' file for a bug report, in the bar as Finamp's logs have their export. -->
  {#if section === 'diagnostics'}<span class="navpill"><a href="/api/diagnostics/download" download aria-label="Download" title="Download"><Icon name="download" size={22} /></a></span>{/if}
  <!-- Each part with preferences offers a Reset, as each of Finamp's settings screens has one in its bar. -->
  {#if partReset.run && !thing}<span class="navpill"><button aria-label="Reset" title="Reset" onclick={() => (resetting = true)}><Icon name="resetSettings" size={22} /></button></span>{/if}
  {#if !beside}<PanelPlace />{/if}
{/snippet}

<div class="view" class:handed>
  {#if !handed}
  <div class="bar" class:beside={beside && !thing}>
    {@render glassLeft()}
    <span class="end">
      {@render glassExtra()}
    </span>
  </div>
  {/if}
  <div class="scroll" class:away={handed && everywhere()} bind:this={scrollEl}>
    {#if ed.loadError && me.admin}<Notice text={ed.loadError} />{/if}
    {#if !allowed}
      <!-- Another user's view of an administrator's part: nothing. -->
    {:else if section === 'device'}
      <DevicePart />
    {:else if section === 'network'}
      <NetworkPart />
    {:else if section === 'sync'}
      <SyncPart />
    {:else if section === 'appearance'}
      <AppearancePart />
    {:else if section === 'layout'}
      <LayoutPart />
    {:else if section === 'player'}
      <PlayerPart />
    {:else if section === 'account'}
      <AccountPart {thing} />
    {:else if section === 'content'}
      <ContentPart />
    {:else if section === 'taste'}
      <TastePart {thing} />
    {:else if section === 'radio'}
      <RadioPart />
    {:else if section === 'tracking'}
      <TrackingPart {thing} />
    {:else if section === 'import'}
      <ImportPart {thing} own />
    {:else if section === 'users'}
      <UsersPart {thing} />
    {:else if section === 'players'}
      <PlayersSettings {thing} />
    {:else if section === 'diagnostics'}
      <DiagnosticsPart />
    {:else if !ed.s}
      {#if !ed.loadError}<PageLoading />{/if}
    {:else if section === 'connections'}
      <ConnectionsPart {thing} />
    {:else if section === 'libraries'}
      <LibrariesPart {thing} />
    {:else if section === 'auth'}
      <AuthPart />
    {:else if section === 'serverimport'}
      <ImportPart {thing} />
    {/if}

    {#if resetting && partReset.run}
      <MenuSheet label="Reset" icon="resetSettings" title={`Reset ${title}`} onclose={() => (resetting = false)}>
        <p class="reset">{partReset.what}</p>
        <MenuEntry icon="resetSettings" label="Reset" danger busy={resetBusy} disabled={resetBusy} onclick={reset} />
        <MenuEntry icon="close" label="Cancel" disabled={resetBusy} onclick={() => (resetting = false)} />
      </MenuSheet>
    {/if}

    {#if ed.dirty}
      <SaveBar saving={ed.saving} canSave={!!ed.s?.settings_file} onsave={ed.save} ondiscard={() => ed.s && ed.fill(ed.s)} />
    {/if}
      <!-- On a phone, over the page: the global results at the end of this list (decision 0072). -->
    <!-- Its own search leaving no row (shown until one is left, lib/settings/narrow). -->
    {#if narrowing}<p class="none">No settings match.</p>{/if}
    {#if handed && everywhere()}<GlobalResults />{/if}
  </div>
</div>

<style>
  /* The panel: its bar on top, the part under it scrolling (room left for the mini player). */
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

  /* Beside the list, without ✕: the name in line with the part under it. */
  .bar.beside {
    padding-left: clamp(12px, 2vw, 24px);
  }

  /* The breadcrumb's part and its ›, as Health's panel has them. */
  .end {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* Padding, not margin: the sheet measures its content's height. */
  .reset {
    margin: 0;
    padding: 4px 24px 12px;
    color: var(--muted);
  }

  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button,
  .navpill a {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 22px;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px clamp(12px, 2vw, 24px) calc(96px + var(--mini-h, 0px));
  }

  /* Its glass handed to App's row on a phone (lib/glass): the list starts under it, its first heading where the page's
     is (the glass's room, then its own 16 px; it began 12 px higher, the owner 2026-10-25). */
  .view.handed .scroll {
    padding-top: calc(var(--bar-room, 0px) + var(--mini-top, 0px) + var(--glass-top) + 16px);
    padding-bottom: calc(var(--mini-h, 0px) + var(--glass-bottom) + 16px);
  }

  .none {
    margin: 8px 4px;
    color: var(--muted);
    font-size: 0.9rem;
  }

  .scroll:has(:global(.item:not(.left))) .none {
    display: none;
  }
</style>
