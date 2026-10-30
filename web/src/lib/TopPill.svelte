<script lang="ts">
  // Symfonium's floating pill (2026-10-15), at the top right of the panel (lib/PlaylistView) and of a page with something
  // to play (App.svelte): the page's search (it opens into a field over the row), the filter (lib/FilterSheet), View
  // (lib/ViewSheet: a page's order, layout and size, or the panel list's order), and where it plays (lib/OutputSheet)
  // where nothing else leads to it (no mini-player, which has it beside the heart; no player beside the page; the panel's
  // pill and the page's alike), and last the page's own button (a + to
  // make something, its ticked ones' ⋮; pageView.usePageAction). Google's Cast guidelines put the device button at the
  // top right of the screen that plays.
  import FilterSheet from './FilterSheet.svelte';
  import ProgressRing from './ProgressRing.svelte';
  import Icon from './Icon.svelte';
  import { longpress } from './longpress';
  import Meter from './Meter.svelte';
  import { slide } from 'svelte/transition';
  import { untrack } from 'svelte';
  import OutputSheet from './OutputSheet.svelte';
  import { filtersOn } from './listFilter.svelte';
  import { pageView, searchFor } from './pageView.svelte';
  import { globalAsked, globalGroups, globalSearch, pageKinds } from './globalSearch.svelte';
  import type { PanelKind } from './prefs.svelte';
  import ViewSheet, { type ListOrder } from './ViewSheet.svelte';
  import { inSpotify, SPOTIFY_FILTER } from './spotify.svelte';

  let {
    page = null,
    view = false,
    device = true,
    order = null,
    tempos = null,
    filter = true,
    kind = null,
    counted = null,
    onfilter = null,
    search = null,
    onview = null,
    filterLit = null,
    action = undefined,
  }: {
    /** The page it is on (its search, its own filters and View), or null in the panel. */
    page?: string | null;
    /** The page's own button, where App says whose (lib/Hold, 0.9.8: the shown page's while the next one is held);
     *  absent, the page that set it last (pageView.action). */
    action?: import('./pageView.svelte').PageAction | null;
    view?: boolean;
    device?: boolean;
    /** The panel list's order, for its View. */
    order?: ListOrder | null;
    /** The panel list's tempos, for its filter's tempo selector (decision 0041). */
    tempos?: readonly (number | null | undefined)[] | null;
    /** The filter's button; not on local mode's pages (decision 0052), which have only their search. */
    filter?: boolean;
    /** In the panel: the kind of list it shows, whose filter is remembered (decision 0060). */
    kind?: PanelKind | null;
    /** In the panel: how many of its list the filter lets through, of how many. */
    counted?: { shown: number; total: number; pending?: boolean } | null;
    /** A panel's own filter and View (Insights' sheets): the buttons open these instead of the lists' sheets; `filterLit`:
     *  whether its filter leaves something out. */
    onfilter?: (() => void) | null;
    /** In a panel over its page (a phone): the page whose search it carries, so the search is there with the page
     *  hidden; beside the page (the page's pill shows it) none, so it is never there twice (the owner, 2026-10-25). */
    search?: string | null;
    onview?: (() => void) | null;
    filterLit?: boolean | null;
  } = $props();
  /** The page's own button: App's say, else the page that set it last. */
  const pageAction = $derived(action !== undefined ? action : pageView.action);
  let filterOpen = $state(false);
  let viewOpen = $state(false);
  let outputOpen = $state(false);

  // The search: the magnifier opens a field over the row (the whole row on a phone, the pill grown leftwards on a
  // wide page); Esc or leaving it empty folds it, ✕ empties it. A search that applies keeps the magnifier lit.
  const words = $derived(page ? searchFor(page) : search ? searchFor(search) : null);
  let searching = $state(false);
  let field = $state<HTMLInputElement>();
  $effect(() => {
    if (searching && !quiet) field?.focus();
    quiet = false;
  });
  $effect(() => {
    // The field, or the work's progress unfolded: either takes the row's room (App's title gives way where it must).
    pageView.searchOpen = searching || showing;
    return () => (pageView.searchOpen = false);
  });
  // Emptied from elsewhere (a result tapped, lib/GlobalResults) while not typed in: folded too.
  // Given its words again from elsewhere (back to a search, lib/globalSearch): open, without the keyboard.
  let typed = untrack(() => pageView.query);
  let quiet = false;
  $effect(() => {
    const q = pageView.query;
    if (q === '' && typed !== '' && document.activeElement !== field) searching = false;
    if (q !== '' && typed === '' && !untrack(() => searching)) {
      quiet = true;
      searching = true;
    }
    typed = q;
  });
  // Folded when what it searches changes (another page, the panel's pill for the page's), not each time the settings
  // are given again: App's one pill gets a new object whenever the panel's settings are read again, and the search
  // opened by a tap folded at once (the owner, 2026-10-25).
  let searched = untrack(() => words);
  $effect(() => {
    const w = words;
    if (w === searched) return;
    searched = w;
    searching = false;
  });
  // A filter leaves something out of what shows: its conditions and switches, counted on the funnel (the list stays
  // clean, decision 0061), or a library switched off (remembered, so it is said here by the funnel's light).
  const list = $derived(page === 'discover' && inSpotify() ? SPOTIFY_FILTER : (page ?? kind ?? ''));
  const filters = $derived(list ? filtersOn(list) : 0);
  const filtering = $derived(filters > 0 || (!!page && pageView.narrowed));
  function clear() {
    pageView.query = '';
    searching = false;
  }

  // The page's work, as the search (the owner, 2026-10-24): a tap on its button unfolds the pill into its progress, the
  // one bar and what it is at, which folds again after a moment, and at once when the work ends; a tap while it runs
  // unfolds it again rather than starting it twice. Only the pill shows it (no line at the page's top); folded, the
  // button's ring does.
  const SHOWN_MS = 4000;
  /** One motion for all that unfolds and makes way: the field or the bar slides in as the other's button slides out,
   *  together, so the pill's width changes in one smooth step; every button has it, played whatever brings it or takes
   *  it away (`|global`: a page change, the two steps' swap), so the pill never jumps (the owner, 2026-10-24). */
  const UNFOLD = { axis: 'x', duration: 200 } as const;
  /** After a page change the slides wait this long: the new page draws first (Playlists blocks the browser for up to
   *  about 90 ms as it is opened or left, measured 2026-10-24), so the slide is seen in full instead of jumping. */
  const PAGE_WAIT = 100;
  /** The page the pill last settled on (not shown, only compared): another page means a change still drawing. */
  let settled = untrack(() => page);
  $effect(() => {
    const p = page;
    if (p === settled) return;
    const t = window.setTimeout(() => (settled = p), PAGE_WAIT);
    return () => clearTimeout(t);
  });
  function paced(node: Element) {
    return slide(node, { ...UNFOLD, delay: page !== settled ? PAGE_WAIT : 0 });
  }
  let showing = $state(false);
  let ran = false;
  let fold = 0;
  function unfold() {
    showing = true;
    clearTimeout(fold);
    fold = window.setTimeout(() => (showing = false), SHOWN_MS);
  }
  function act(a: NonNullable<typeof pageView.action>) {
    // A button that starts work it shows (`progress` given, null while idle); others only act.
    if (a.progress === undefined) return a.run();
    // Its progress showing: the tap folds it (the owner, 2026-10-24).
    if (showing) {
      clearTimeout(fold);
      showing = false;
      return;
    }
    // Not running: two steps (the owner, 2026-10-24): the pill asks first, ✓ starts it, ✕ leaves it.
    if (a.progress === null) {
      confirming = true;
      return;
    }
    unfold();
  }
  /** A long-press while the work runs: its progress unfolds and stays, no fold after SHOWN_MS (the owner, 2026-10-25);
   *  a tap folds it, and it folds when the work ends, as ever. */
  function hold(a: NonNullable<typeof pageView.action>) {
    if (a.progress == null) return;
    clearTimeout(fold);
    showing = true;
  }
  /** The page's work asked for and not yet confirmed: the pill is ✓ and ✕ alone. */
  let confirming = $state(false);
  function confirm(a: NonNullable<typeof pageView.action>) {
    confirming = false;
    a.run();
    unfold();
  }
  $effect(() => {
    void page;
    confirming = false;
  });
  $effect(() => {
    const p = pageAction?.progress;
    if (p != null) ran = true;
    else if (ran) {
      ran = false;
      showing = false;
    }
  });
  $effect(() => {
    void page;
    showing = false;
  });
  $effect(() => () => clearTimeout(fold));

  // The globe (the owner, 2026-10-25): while a search is on, left of the magnifier on thinner glass (as the title's),
  // how many everywhere finds on its badge; a tap shows everywhere's results in the page's or the panel's place, lit,
  // another its own again. Home searches everywhere as it is, Find more music is MusicBrainz's: no globe there.
  // Not on a panel that is a search's result, carrying the page's words (the owner, 2026-10-25); a search typed in
  // the panel's own field is its own, and has it.
  const pillOf = $derived(`${page ?? ''}|${search ?? ''}`);
  let typedHere = $state(false);
  let pillWas = untrack(() => pillOf);
  $effect(() => {
    if (pillOf === pillWas) return;
    pillWas = pillOf;
    typedHere = false;
  });
  $effect(() => {
    if (pageView.query.trim() === '') typedHere = false;
  });
  const globe = $derived(!!words && pageView.query.trim() !== '' && (page ? page !== 'home' && page !== 'find' : typedHere));
  const found = $derived(globe ? globalGroups(page ? pageKinds(page) : []).reduce((n, g) => n + g.total, 0) : 0);
  const globeOn = $derived(globalSearch.scope === 'everywhere');
  // While the new words are asked, the count the last ones had (the owner, 2026-10-25: the badge went while typing).
  let lastFound = $state<number | null>(null);
  $effect(() => {
    if (!globe) lastFound = null;
    else if (globalAsked()) lastFound = found;
  });
  const badge = $derived(globalAsked() ? found : lastFound);
  // Only with something found (the owner, 2026-10-25): none, no globe; once gone, back only with an answer that finds
  // something, so no empty glass comes and goes while typing. An answer comes 300 ms after the last key at the
  // earliest (lib/globalSearch), past the owner's 100 ms. Gone while lit: the page's own results again.
  const globeShown = $derived(globe && (badge ?? 0) > 0);
  $effect(() => {
    if (!globeShown && untrack(() => globalSearch.scope) === 'everywhere') globalSearch.scope = 'page';
  });
</script>

<span class="pill-row" class:wide={showing}>
  {#if globeShown}
    <span class="globe" transition:paced|global>
      <button
        class="funnel"
        class:on={globeOn}
        aria-pressed={globeOn}
        aria-label={globeOn ? "Back to this page's results" : `Everywhere, ${badge} found`}
        title="Everywhere"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => (globalSearch.scope = globeOn ? 'page' : 'everywhere')}
        ><Icon name="globe" size={22} />{#if badge !== null}<span class="badge">{badge > 99 ? '99+' : badge}</span>{/if}</button
      >
    </span>
  {/if}

  <div class="pill" class:wide={showing} role="toolbar" aria-label="The page's tools">
    {#if confirming && page && pageAction}
      {@const a = pageAction}
      <!-- The page's work asked for: the whole pill is ✓ (start it) and ✕ (leave it), until one is tapped. -->
      <button class="confirm" aria-label={a.label} title={a.label} onclick={() => confirm(a)} transition:paced|global><Icon name="check" size={22} /></button>
      <button aria-label="Cancel" title="Cancel" onclick={() => (confirming = false)} transition:paced|global><Icon name="close" size={22} /></button>
    {:else}
    {#if showing && page && pageAction && !searching}
      {@const a = pageAction}
      <!-- The page's work unfolded: only its bar and what it is at, the pill grown leftwards, its buttons where they
           are, its glass as it is (the owner, 2026-10-24). -->
      <span class="work" role="status" aria-live="polite" transition:paced|global
        ><Meter label={a.line ?? a.label} value={a.progress ?? 0} /></span
      >
    {/if}
    <!-- One unfolds at a time, the other's button out of the way: the search hides while the work's progress shows, the
         page's button while the search is open (the owner, 2026-10-24). -->
    {#if words && !(showing && page && pageAction)}
      <button
        class:lit={!searching && pageView.query.trim() !== ''}
        aria-label={words}
        title={words}
        onmousedown={(e) => searching && e.preventDefault()}
        onclick={() => (searching = !searching)}
        transition:paced|global
        ><Icon name="search" size={22} /></button
      >
      {#if searching}
        <!-- The search open: its field right of the magnifier, the pill grown leftwards, the other buttons where they
             are, its glass as it is (the owner, 2026-10-24). -->
        <span class="field" role="search" transition:paced|global>
          <input
            bind:this={field}
            type="search"
            placeholder={words}
            aria-label={words}
            bind:value={pageView.query}
            oninput={() => (typedHere = true)}
            onkeydown={(e) => e.key === 'Escape' && (searching = false)}
            onblur={() => pageView.query.trim() === '' && (searching = false)}
          />
          <button aria-label="Clear the search" title="Clear" onmousedown={(e) => e.preventDefault()} onclick={clear}><Icon name="close" size={20} /></button>
        </span>
      {/if}
    {/if}
    {#if filter && !searching && !(showing && page && pageAction)}<button class="funnel" class:lit={filterLit ?? filtering} aria-label={filters ? `Filter, ${filters} on` : 'Filter'} title="Filter" aria-haspopup="dialog" onclick={() => (onfilter ? onfilter() : (filterOpen = true))} transition:paced|global
        ><Icon name="funnel" size={22} />{#if filters}<span class="badge">{filters}</span>{/if}</button
      >{/if}
    {#if view && (page || order?.columns?.shown.length || order?.view || onview) && !searching && !(showing && page && pageAction)}<button aria-label="View" title="View" aria-haspopup="dialog" onclick={() => (onview ? onview() : (viewOpen = true))} transition:paced|global><Icon name="sliders" size={22} /></button>{/if}
    <!-- Where it plays gives way to the open field too, as the rest (the owner, 2026-10-26). -->
    {#if device && !searching}<button aria-label="Where it plays" title="Where it plays" aria-haspopup="dialog" onclick={() => (outputOpen = true)} transition:paced|global><Icon name="speaker" size={22} /></button>{/if}
    <!-- The page's mode (Playlists' Quick add), lit while on. -->
    {#if page && pageView.mode && !searching && !(showing && pageAction)}
      {@const m = pageView.mode}
      <button class:lit={m.on} aria-pressed={m.on} aria-label={m.label} title={m.label} onclick={m.toggle} transition:paced|global><Icon name={m.icon} size={22} /></button>
    {/if}
    <!-- A page's own button; a panel's where it hands one (the Map's Full screen, lib/InsightsPanel). -->
    {#if (page || action) && pageAction && !searching}
      {@const a = pageAction}
      <button class="action" aria-label={a.label} title={a.label} onclick={() => act(a)} use:longpress={() => hold(a)} transition:paced|global
        ><Icon name={a.icon} size={22} />{#if a.progress != null}<ProgressRing progress={a.progress} size={38} />{/if}</button
      >
    {/if}
    {/if}
  </div>
</span>
{#if filterOpen}
  <FilterSheet {page} {kind} {tempos} counted={page ? pageView.counted : counted} onclose={() => (filterOpen = false)} />
{/if}
{#if viewOpen && (page || order?.columns?.shown.length || order?.view)}
  <ViewSheet {page} {order} onclose={() => (viewOpen = false)} />
{/if}
{#if outputOpen}
  <OutputSheet onclose={() => (outputOpen = false)} />
{/if}

<style>
  /* The pill and the globe left of it, the pill's round end over the globe's glass. */
  .pill-row {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  /* Thinner glass than the pill's (as the title's, App's .lead): more light through. */
  .globe {
    display: flex;
    align-items: center;
    margin-right: -24px;
    /* As tall as the pill with its band (app.css --island-ring). */
    padding: 4px 26px 4px 4px;
    border-radius: 26px 0 0 26px;
    background: color-mix(in srgb, var(--float-bg) 45%, transparent);
    -webkit-backdrop-filter: blur(10px) saturate(1.3);
    backdrop-filter: blur(10px) saturate(1.3);
    box-shadow: 0 2px 10px rgb(0 0 0 / 0.18);
  }

  /* Everywhere's shown: lit. */
  .globe button.on {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    color: var(--accent-ink);
  }

  /* A dark round-ended pill as Symfonium's, a little see-through and blurring what is under it. */
  .pill {
    position: relative;
    display: flex;
    padding: 2px;
    /* Nothing draws past its outline: a button sliding out (narrower than its icon and ring) is cut at the edge. */
    overflow: hidden;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  button {
    display: grid;
    place-items: center;
    width: 48px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 22px;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  /* Only where a pointer hovers: on a touchscreen a tapped button would stay shaded. */
  @media (hover: hover) {
    button:hover {
      background: color-mix(in srgb, var(--text) 8%, transparent);
    }
  }

  /* The page's own button with work going on: a ring of its progress around the icon. */
  .action {
    position: relative;
  }

  /* The page's work asked for: its ✓ in the accent, the ✕ as the pill's other buttons. */
  .confirm {
    color: var(--accent-ink);
  }

  /* A search or a filter that applies. */
  button.lit {
    color: var(--accent-ink);
  }

  /* How many filters are on: a small count on the funnel's corner, the list itself left clean (decision 0061). */
  .funnel {
    position: relative;
  }

  .badge {
    position: absolute;
    top: 2px;
    right: 0;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font-size: 11px;
    font-weight: 700;
    line-height: 16px;
    text-align: center;
  }

  /* The search open: its field and ✕ inside the pill, right of the magnifier, as wide as a phone's row allows. */
  .field {
    display: flex;
    align-items: center;
    width: min(320px, calc(100vw - 220px));
    min-width: 140px;
    overflow: hidden;
  }

  .field input {
    flex: 1;
    min-width: 0;
    height: 44px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-size: 16px;
    outline: none;
  }

  /* The page's work unfolded, left of the buttons: its bar and what it is at, in the pill's own text colour; as wide
     as the search opens, so what it is at fits (the owner, 2026-10-25). */
  /* The work's progress unfolded takes all the room the row has, up to 560 px (the owner, 2026-10-25: on a phone it drew
     a little over half, its line cut, a gap left before back). */
  .pill-row.wide,
  .pill.wide {
    flex: 1 1 auto;
    justify-content: flex-end;
  }

  .work {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    max-width: 560px;
    min-width: 140px;
    padding: 0 10px 0 16px;
    overflow: hidden;
    white-space: nowrap;
  }

  .work :global(.meter) {
    width: 100%;
    margin-top: 0;
    overflow: hidden;
    color: inherit;
    text-overflow: ellipsis;
  }

  .work :global(.track) {
    max-width: none;
  }

  /* One ✕: ours (it also folds the field), not the browser's. */
  .field input::-webkit-search-cancel-button {
    display: none;
  }
</style>
