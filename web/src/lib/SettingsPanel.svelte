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
  import { rowRoot } from './glassRow.svelte';
  import { glass, handGlass } from './glass.svelte';
  import ResetSheet from './settings/ResetSheet.svelte';
  import GlobalResults from './GlobalResults.svelte';
  import { everywhere } from './globalSearch.svelte';
  import { untrack } from 'svelte';
  import { narrowSettings } from './settings/narrow';
  import { matchesSearch, pageView, searchElsewhere, searchNone } from './pageView.svelte';
  import { words } from './search';
  import Crumbs from './Crumbs.svelte';
  import Icon from './Icon.svelte';
  import Notice from './Notice.svelte';
  import PageLoading from './PageLoading.svelte';
  import PanelPlace from './PanelPlace.svelte';
  import { askDevice, askDownloads, inApp } from './app.svelte';
  import { backFromThing, backOne, panel, SETTINGS, settingsThing, upToPage, viewPlaylist, viewSibling } from './panel.svelte';
  import type { Level, Siblings } from './Crumbs.svelte';
  import { groupsOf, landingOf, remembered } from './groupCrumb';
  import { prefs } from './prefs.svelte';
  import { keptFor, partsFor, SECTIONS, SETTINGS_GROUPS, thingTitle, type SettingsSection } from './settingsSections.svelte';
  import { installed } from './offline.svelte';
  import { ed } from './settings/editor.svelte';
  import { resetRows, runReset } from './settings/reset.svelte';
  import AccountPart from './settings/AccountPart.svelte';
  import AuthPart from './settings/AuthPart.svelte';
  import UsersPart from './settings/UsersPart.svelte';
  import { me } from './me.svelte';
  import { loadSettingsWords, settingsMatches, settingsWords } from './settingsSearch.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
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
  // On a phone the part's name is a picker of the other parts of its group, and its group's a picker of the groups
  // (lib/Crumbs, lib/groupCrumb; the owner, 2026-11-02: Settings › Device › Appearance, one list of every part was "way
  // too tall"): a move sideways, a group's landing on the part last seen in it, else its top one.
  const parts = $derived(partsFor(me.admin, inApp, installed).map((x) => ({ id: x.id as string, group: keptFor(x, me.admin) as string, title: x.id === 'device' ? deviceName() : x.title })));
  const groupNow = $derived(parts.find((x) => x.id === section)?.group ?? null);
  const groupTitle = (kept: string) => SETTINGS_GROUPS.find((g) => g.kept === kept)?.title ?? kept;
  const siblings = $derived.by<Siblings | null>(() => {
    const own = parts.filter((x) => x.group === groupNow);
    return own.length > 1 ? { menuLabel: `${groupTitle(groupNow ?? '')} shows`, options: own.map((x) => ({ id: x.id, label: x.title })), chosen: section, onchoose: (id) => viewSibling(SETTINGS + id) } : null;
  });
  const groupLevel = $derived<Level | null>(
    groupNow
      ? {
          label: groupTitle(groupNow),
          pick: {
            menuLabel: 'Settings groups',
            options: groupsOf(parts).map((g) => ({ id: g, label: groupTitle(g) })),
            chosen: groupNow,
            onchoose: (g) => {
              const to = g === groupNow ? null : landingOf(parts, g, prefs.groupLast[`settings/${g}`]);
              if (to) viewSibling(SETTINGS + to);
            },
          },
        }
      : null,
  );
  // The part seen, remembered for its group's next pick.
  $effect(() => {
    if (groupNow) prefs.groupLast = remembered(prefs.groupLast, 'settings', groupNow, section);
  });
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

  // The server's settings saved as they are changed (the owner, 2026-11-02: "I thought we used to have instantly saving
  // settings"; the Unsaved changes bar is gone): each sheet's OK, switch or choice is the decision, a connection's
  // address and secrets one sheet together; a moment after the last change, while every server has what it needs.
  $effect(() => {
    if (!me.admin || !ed.dirty || !ed.complete || ed.saving || !ed.s?.settings_file) return;
    const t = setTimeout(() => void ed.save(), 300);
    return () => clearTimeout(t);
  });

  // A part's Reset (lib/settings/reset): asked once more, in a sheet, before its preferences go back to their defaults.
  let resetting = $state(false);
  // On a phone this panel's glass is App's (lib/glass, the owner 2026-10-25: one glass, page's and panel's): handed over,
  // its own bar not drawn, its list under the glass.
  const handed = $derived(glass.one && !beside);
  // Its own search there, as every panel's (the owner, 2026-10-25): the part's rows narrowed to it (lib/settings/narrow),
  // all of them when the part's name has the words; a thing's page has none.
  const narrowing = $derived(handed && !thing && words(pageView.query).length > 0 && !matchesSearch(partTitle));
  // None here or in another part (decision 0095): the pill says so from the magnifier, not the panel's own words.
  // Its own rows read as the other parts' are (lib/settingsSearch), once the words are known.
  searchNone(
    () => narrowing && !!settingsWords.by && !settingsMatches(section, partTitle, words(pageView.query)).rows.length && { n: settingsWords.by[section]?.length ?? 0, one: 'setting' },
    'panel',
  );
  narrowSettings(() => narrowing);
  // The page's search through every part (the owner, 2026-11-02: "the search input field should be page-wide across all
  // child panels. Obviously, the current panel should surface at the top, but directly after, all siblings on the page
  // should surface too"): under this part's own rows, each other part it finds with its rows; a tap opens that part,
  // the words still searched. Nothing anywhere: the page's search says so (decision 0095), not this panel.
  $effect(() => {
    if (narrowing) void loadSettingsWords();
  });
  const elsewhere = $derived.by(() => {
    if (!narrowing) return [];
    void settingsWords.by;
    const w = words(pageView.query);
    return SECTIONS.filter((x) => x.id !== section && (!x.app || inApp) && (!x.admin || me.admin || x.everyone))
      .map((x) => ({ id: x.id, title: x.title, found: settingsMatches(x.id, x.title, w) }))
      .filter((x) => x.found.title || x.found.rows.length);
  });
  searchElsewhere(() => elsewhere.length > 0);

  // Its Reset as the pill's own button, in the top right glass (the owner, 2026-11-02: "to save space", not a glass of
  // its own beside it).
  const resetAction = () => (resetRows(section).length ? { icon: 'resetSettings' as const, label: 'Reset', run: () => (resetting = true) } : null);
  const pill = () => (!thing ? { filter: false, view: false, search: 'settings', action: resetAction() } : null);
  handGlass(() => handed, { left: glassLeft, pill, extra: glassExtra });
</script>

<!-- This panel's glass: its own bar's parts, or App's on a phone (lib/glass). -->
{#snippet glassLeft()}
  <!-- Settings › its group › the part; a thing's page: its part › its name (lib/Crumbs: two at most, the owner 2026-10-25;
       the group a picker, the owner 2026-11-02). -->
  {@const top: Level[] = beside ? [] : [{ label: 'Settings', go: () => upToPage('settings') }, ...(groupLevel ? [groupLevel] : [])]}
  {#if thing}
    <Crumbs back={() => backFromThing(section)} trail={[...top, { label: partTitle, go: () => backFromThing(section) }]} here={title} />
  {:else}
    <Crumbs back={beside ? null : backOne} trail={top} here={partTitle} siblings={beside ? null : siblings} />
  {/if}
{/snippet}
{#snippet glassExtra()}
  <!-- Diagnostics' file for a bug report, in the bar as Finamp's logs have their export. -->
  {#if section === 'diagnostics'}<span class="navpill"><a href="/api/diagnostics/download" download aria-label="Download" title="Download"><Icon name="download" size={22} /></a></span>{/if}
  <!-- Each part with preferences offers a Reset, as each of Finamp's settings screens has one in its bar; handed to App's
       glass, it is the pill's own button. -->
  {#if !thing && !handed && resetRows(section).length}<span class="navpill"><button aria-label="Reset" title="Reset" onclick={() => (resetting = true)}><Icon name="resetSettings" size={22} /></button></span>{/if}
  {#if !beside}<PanelPlace />{/if}
{/snippet}

<div class="view" class:handed>
  {#if !handed}
  <div class="bar" {@attach rowRoot()} class:beside={beside && !thing}>
    {@render glassLeft()}
    <span class="end">
      {@render glassExtra()}
    </span>
  </div>
  {/if}
  <div class="scroll" class:away={handed && everywhere()} bind:this={scrollEl}>
    {#if ed.loadError && me.admin}<Notice text={ed.loadError} />{/if}
    {#if me.admin && ed.s && !ed.s.settings_file && (section === 'connections' || section === 'libraries')}<Notice tone="warn" text="Cratarr can't write its settings file, so changes here are not kept." />{/if}
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

    {#if resetting && resetRows(section).length}
      <!-- What Reset puts back, each one now → its default (lib/settings/ResetSheet). -->
      <ResetSheet title={`Reset ${title}`} rows={resetRows(section)} onconfirm={() => runReset(section)} onclose={() => (resetting = false)} />
    {/if}

      <!-- On a phone, over the page: the global results at the end of this list (decision 0072). -->
    <!-- Its own search leaving no row (shown until one is left, lib/settings/narrow). -->
    {#if elsewhere.length}
      <!-- The other parts the search finds, each a group of its rows (lib/settings/Group, Item). -->
      <div class="elsewhere" aria-label="In other parts">
        {#each elsewhere as e (e.id)}
          <Group title={e.title}>
            {#each e.found.rows.length ? e.found.rows : [e.title] as row (row)}<Item name={row} onclick={() => viewSibling(SETTINGS + e.id)} />{/each}
          </Group>
        {/each}
      </div>
    {/if}
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
  /* What Reset puts back: a row each, its name, then now → its default. */



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

</style>
