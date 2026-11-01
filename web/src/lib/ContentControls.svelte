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
  import { fade } from 'svelte/transition';
  import { glassPart, GLASS_MS } from './glassMotion';
  import { untrack } from 'svelte';
  import { innerWidth } from 'svelte/reactivity/window';
  import OutputSheet from './OutputSheet.svelte';
  import { filtersOn } from './listFilter.svelte';
  import { pageView, searchFor } from './pageView.svelte';
  import { globalAsked, globalGroups, globalSearch, pageKinds } from './globalSearch.svelte';
  import type { PanelKind } from './prefs.svelte';
  import ViewSheet, { type ListOrder } from './ViewSheet.svelte';
  import { inSpotify, SPOTIFY_FILTER } from './spotify.svelte';
  import { dimmedIn } from './quickCrate';

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
  // One motion for all that unfolds and makes way (the owner, 2026-10-24: the pill never jumps): the pill is a glass
  // part (lib/glassMotion), so the field or the bar comes in as the other's button goes, its glass reaching from the
  // old outline to the new in one step, whatever brings them or takes them away (a page change too: nothing in it lays
  // the page out again, so a page busy drawing does not stall it).
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
  // The field's words read as a crate (0.13; the owner, 2026-10-31): the text as typed, only its joining words lighter,
  // as the sentence's. A layer behind the field draws them (the field's own text clear above it, its caret and editing
  // its own); only for the words read, none while new ones are read.
  const dims = $derived.by(() => {
    const c = globalSearch.crate;
    const value = pageView.query;
    return c && c.q === value.trim() ? dimmedIn(value, c.chips.map((x) => x.term)) : [];
  });
  const layered = $derived.by(() => {
    if (!dims.length) return null;
    const value = pageView.query;
    const out: { text: string; dim: boolean }[] = [];
    let at = 0;
    for (const [a, b] of dims) {
      if (a > at) out.push({ text: value.slice(at, a), dim: false });
      out.push({ text: value.slice(a, b), dim: true });
      at = b;
    }
    if (at < value.length) out.push({ text: value.slice(at), dim: false });
    return out;
  });
  let mirror = $state<HTMLElement>();
  /** The layer scrolled as the field is. */
  const follow = () => mirror && field && (mirror.scrollLeft = field.scrollLeft);
  $effect(() => {
    void layered;
    follow();
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

  // The page's own search finding none (0.13; the owner, 2026-10-31: A2): said under the pill, its tip on the
  // magnifier (lit, folded, after another page; or the field open), the words a chip whose ✕ clears; everywhere's
  // count stays the globe's alone. Not while everywhere's results show.
  const noneShown = $derived(!!page && !!pageView.none && pageView.query.trim() !== '' && !globeOn);
  let row = $state<HTMLElement>();
  // The open search never narrower than the pill folded (the owner, 2026-11-01): the field takes at least the room the
  // buttons it replaces took, besides its own wish (as wide as a phone's row allows, 320 px at most). Said on the field
  // (`data-wants`) for the title beside it to give way where it would squeeze it (lib/NavigationControls).
  let pill = $state<HTMLElement>();
  let folded = $state(0);
  $effect(() => {
    const p = pill;
    if (!p) return;
    const seen = new ResizeObserver(() => {
      if (!searching && !showing && !confirming) folded = p.offsetWidth;
    });
    seen.observe(p);
    return () => seen.disconnect();
  });
  /** The lens and the pill's own room, beside the field. */
  const LENS = 52;
  const fieldWants = $derived(Math.max(Math.min(320, (innerWidth.current ?? 400) - 220), folded - LENS, 140));
  let lens = $state<HTMLElement>();
  /** The magnifier's middle, from the row's right end: where the callout's tip points. */
  let tip = $state(24);
  $effect(() => {
    const [r, l] = [row, lens];
    if (!noneShown || !r || !l) return;
    const place = () => {
      const a = r.getBoundingClientRect();
      const b = l.getBoundingClientRect();
      tip = a.right - (b.left + b.width / 2);
    };
    place();
    // The pill's outline changes (the field opening, at once; its glass then moves): the tip follows.
    const seen = new ResizeObserver(place);
    seen.observe(r);
    return () => seen.disconnect();
  });
  $effect(() => {
    if (!globeShown && untrack(() => globalSearch.scope) === 'everywhere') globalSearch.scope = 'page';
  });
</script>

<span class="pill-row" class:wide={showing} class:fill={searching && pageView.leadAway} bind:this={row}>
  {#if globeShown}
    <span class="globe" transition:fade|global={{ duration: GLASS_MS }}>
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

  <div class="pill" class:wide={showing} role="toolbar" aria-label="The page's tools" bind:this={pill} {@attach glassPart({ radius: 26 })}>
    <span class="shade" aria-hidden="true"></span><span class="glass" aria-hidden="true"></span>
    {#if confirming && page && pageAction}
      {@const a = pageAction}
      <!-- The page's work asked for: the whole pill is ✓ (start it) and ✕ (leave it), until one is tapped. -->
      <button class="confirm" aria-label={a.label} title={a.label} onclick={() => confirm(a)}><Icon name="check" size={22} /></button>
      <button aria-label="Cancel" title="Cancel" onclick={() => (confirming = false)}><Icon name="close" size={22} /></button>
    {:else}
    {#if showing && page && pageAction && !searching}
      {@const a = pageAction}
      <!-- The page's work unfolded: only its bar and what it is at, the pill grown leftwards, its buttons where they
           are, its glass as it is (the owner, 2026-10-24). -->
      <span class="work" role="status" aria-live="polite"
        ><Meter label={a.line ?? a.label} value={a.progress ?? 0} /></span
      >
    {/if}
    <!-- One unfolds at a time, the other's button out of the way: the search hides while the work's progress shows, the
         page's button while the search is open (the owner, 2026-10-24). -->
    {#if words && !(showing && page && pageAction)}
      <button
        bind:this={lens}
        class:lit={!searching && pageView.query.trim() !== ''}
        aria-label={words}
        title={words}
        onmousedown={(e) => searching && e.preventDefault()}
        onclick={() => (searching = !searching)}
       
        ><Icon name="search" size={22} /></button
      >
      {#if searching}
        <!-- The search open: its field right of the magnifier, the pill grown leftwards, the other buttons where they
             are, its glass as it is (the owner, 2026-10-24). -->
        <span class="field" role="search" style:width="{fieldWants}px" data-wants={fieldWants}>
          <span class="typed" class:layered={!!layered}>
            {#if layered}<span class="mirror" aria-hidden="true" bind:this={mirror}>{#each layered as part, k (k)}<span class:dim={part.dim}>{part.text}</span>{/each}</span>{/if}
            <input
              bind:this={field}
              type="search"
              placeholder={words}
              aria-label={words}
              bind:value={pageView.query}
              oninput={() => ((typedHere = true), follow())}
              onscroll={follow}
              onkeyup={follow}
              onselect={follow}
              onkeydown={(e) => e.key === 'Escape' && (searching = false)}
              onblur={() => pageView.query.trim() === '' && (searching = false)}
            />
          </span>
          <button aria-label="Clear the search" title="Clear" onmousedown={(e) => e.preventDefault()} onclick={clear}><Icon name="close" size={20} /></button>
        </span>
      {/if}
    {/if}
    {#if filter && !searching && !(showing && page && pageAction)}<button class="funnel" class:lit={filterLit ?? filtering} aria-label={filters ? `Filter, ${filters} on` : 'Filter'} title="Filter" aria-haspopup="dialog" onclick={() => (onfilter ? onfilter() : (filterOpen = true))}
        ><Icon name="funnel" size={22} />{#if filters}<span class="badge">{filters}</span>{/if}</button
      >{/if}
    {#if view && (page || order?.columns?.shown.length || order?.view || onview) && !searching && !(showing && page && pageAction)}<button aria-label="View" title="View" aria-haspopup="dialog" onclick={() => (onview ? onview() : (viewOpen = true))}><Icon name="sliders" size={22} /></button>{/if}
    <!-- Where it plays gives way to the open field too, as the rest (the owner, 2026-10-26). -->
    {#if device && !searching}<button aria-label="Where it plays" title="Where it plays" aria-haspopup="dialog" onclick={() => (outputOpen = true)}><Icon name="speaker" size={22} /></button>{/if}
    <!-- The page's mode (Playlists' Quick add), lit while on. -->
    {#if page && pageView.mode && !searching && !(showing && pageAction)}
      {@const m = pageView.mode}
      <button class:lit={m.on} aria-pressed={m.on} aria-label={m.label} title={m.label} onclick={m.toggle}><Icon name={m.icon} size={22} /></button>
    {/if}
    <!-- A page's own button; a panel's where it hands one (the Map's Full screen, lib/InsightsPanel). -->
    {#if (page || action) && pageAction && !searching}
      {@const a = pageAction}
      <button class="action" aria-label={a.label} title={a.label} onclick={() => act(a)} use:longpress={() => hold(a)}
        ><Icon name={a.icon} size={22} />{#if a.progress != null}<ProgressRing progress={a.progress} size={38} />{/if}</button
      >
    {/if}
    {/if}
  </div>
  {#if noneShown}
    <div class="none" role="status" style:--tip="{tip}px" transition:fade={{ duration: 150 }}>
      <span class="said">No {pageView.none} for</span>
      <span class="words"><span class="q">{pageView.query.trim()}</span><button class="clear" aria-label="Clear the search" title="Clear" onclick={clear}><Icon name="close" size={14} /></button></span>
    </div>
  {/if}
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
    position: relative;
    display: flex;
    align-items: center;
    min-width: 0;
  }

  /* The page's search finding none (A2): one slim line hanging from the pill, its tip on the magnifier, the words a
     chip whose ✕ clears. */
  .none {
    position: absolute;
    top: calc(100% + 12px);
    right: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: calc(100vw - 24px);
    padding: 6px 6px 6px 14px;
    border-radius: 999px;
    background: var(--surface);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--text) 9%, transparent), 0 12px 32px rgb(0 0 0 / 0.4);
    white-space: nowrap;
  }

  .none::before {
    content: '';
    position: absolute;
    top: -6px;
    right: calc(var(--tip) - 6px);
    width: 12px;
    height: 12px;
    transform: rotate(45deg);
    background: var(--surface);
    box-shadow: -1px -1px 0 color-mix(in srgb, var(--text) 9%, transparent);
  }

  .said {
    color: var(--muted);
    font-size: 14px;
  }

  .words {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 4px 4px 4px 12px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 9%, transparent);
    font-size: 14px;
  }

  .words .q {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .words .clear {
    flex: none;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
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

  /* A dark round-ended pill as Symfonium's, a little see-through and blurring what is under it: its glass and its shade
     are layers under its buttons (lib/glassMotion moves them), the pill itself drawing nothing. */
  .pill {
    position: relative;
    isolation: isolate;
    display: flex;
    /* It may be narrower than its buttons want (the open field gives first): it ran past the window's edge. */
    min-width: 0;
    padding: 2px;
  }

  .pill > .glass,
  .pill > .shade {
    position: absolute;
    inset: 0;
    z-index: -1;
    border-radius: 26px;
    pointer-events: none;
  }

  .pill > .glass {
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
  }

  .pill > .shade {
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

  /* The field and, behind it while its words read as a crate, the layer that draws them: the same box, font and
     letters, so each glyph lies on its own; the field's letters clear then, its caret (the text's colour) still drawn. */
  .typed {
    position: relative;
    display: flex;
    flex: 1;
    min-width: 0;
  }

  .field input,
  .mirror {
    height: 44px;
    padding: 0;
    border: 0;
    font: inherit;
    font-size: 16px;
    letter-spacing: normal;
  }

  .field input {
    flex: 1;
    min-width: 0;
    background: none;
    color: inherit;
    outline: none;
    -webkit-appearance: none;
    appearance: none;
  }

  .mirror {
    position: absolute;
    inset: 0;
    overflow: hidden;
    line-height: 44px;
    white-space: pre;
    pointer-events: none;
  }

  .layered input {
    position: relative;
    -webkit-text-fill-color: transparent;
  }

  /* Its joining words, as the sentence's (lib/CrateSentence .w). */
  .mirror .dim {
    opacity: 0.55;
  }

  /* The page's work unfolded, left of the buttons: its bar and what it is at, in the pill's own text colour; as wide
     as the search opens, so what it is at fits (the owner, 2026-10-25). */
  /* The work's progress unfolded takes all the room the row has, up to 560 px (the owner, 2026-10-25: on a phone it drew
     a little over half, its line cut, a gap left before back). */
  /* The open search where the title gave way: it grows to meet back (the owner, 2026-11-01), its own width the least. */
  .pill-row.fill,
  .pill-row.fill .pill,
  .pill-row.fill .field {
    flex: 1 1 auto;
  }

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
