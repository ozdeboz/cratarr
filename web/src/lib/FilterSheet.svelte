<script lang="ts">
  // The filter, after Symfonium's source filter (opened from the funnel in a floating pill, lib/TopPill): first the
  // list's own filter, remembered per kind of list (decisions 0060 and 0061): how many it lets through, its conditions
  // as cards (lib/ConditionList: a crate's, unsaved; any or all of each item's tracks where the items hold them), and
  // the kind's own switches; at the title's end, Save as crate (the owner, 2026-10-23) and Reset, back to no filter of
  // the list's own (the owner, 2026-10-29: "a reset button in the top right"). Then, last, the one filter that holds everywhere: a card per server this browser can
  // follow, with its kind and its libraries. On the followed server's card the libraries are switches on the Library
  // and on Crates (decision 0034; a crate fills from every library switched on, the owner, 2026-10-23), every used
  // one, remembered as the server is; the last one on stays on, so the page never filters down to nothing; elsewhere a
  // chip only shows the library's use. A tap on a server's card follows it; a tap on the followed one goes back to no choice, which is
  // Cratarr's own source where the server has it (lib/sources). The libraries are only shown, with their use: Cratarr
  // has no per-library visibility, and a use is set only in Settings (decision 0009).
  import Switch from './Switch.svelte';
  import ConditionList from './ConditionList.svelte';
  import type { ListAxis } from './ConditionCard.svelte';
  import { activeConditions, filterOf, filtersOn, hiddenLibraries, inView, resetFilter, setConditions, setSwitch, switchOn } from './listFilter.svelte';
  import { classicalListed } from './recordKinds';
  import { SWITCHES } from './pageView.svelte';
  import { startCrate } from './crates.svelte';
  import { filterList, getCrateAxes, getLibraries, getSettings, getStatus, type LibrariesView, type LibraryUse } from './api';
  import { me } from './me.svelte';
  import Icon from './Icon.svelte';
  import MenuSheet, { type SheetAction } from './MenuSheet.svelte';
  import MarkLoader from './MarkLoader.svelte';
  import { prefs, type PanelKind } from './prefs.svelte';
  import { known, libraryShown, noChoice, remember, showLibrary } from './sources.svelte';
  import { toast } from './toast.svelte';
  import { inSpotify, SPOTIFY_FILTER } from './spotify.svelte';

  // `page`: the page it was opened on, whose filter it shows; null in the panel, which says its `kind` of list (and
  // gives its `tempos` for the Tempo condition's spread, decision 0041). `counted`: how many the filter lets through.
  let {
    page = null,
    kind = null,
    tempos = null,
    counted = null,
    onclose,
  }: {
    page?: string | null;
    kind?: PanelKind | null;
    tempos?: readonly (number | null | undefined)[] | null;
    counted?: { shown: number; total: number; pending?: boolean } | null;
    onclose: () => void;
  } = $props();

  // Discover's Spotify lists filter as their own kind (lib/spotify).
  const list = $derived(page === 'discover' && inSpotify() ? SPOTIFY_FILTER : (page ?? kind ?? ''));
  // Conditions on the lists whose items the server knows the facts of: the Library's albums, Playlists, and a
  // playlist's tracks in the panel (the panel's only list with a filter, lib/PlaylistView); Any or All where the items
  // hold tracks (decision 0061). Crates have none (decision 0060).
  const CONDITIONED = ['library', 'playlists', 'playlist'];
  const conditioned = $derived(CONDITIONED.includes(list));
  const items = $derived(list === 'library' ? 'album' : list === 'playlists' ? 'playlist' : null);
  const switches = $derived((SWITCHES[list] ?? []).filter((w) => (!w.incoming || hasIncoming) && (!w.classical || classicalListed())));
  /** The switches by section, in the order the sections first come; switches without one in a section of no title. */
  const sections = $derived.by(() => {
    const out: { title: string | null; switches: typeof switches }[] = [];
    for (const w of switches) {
      const title = w.section ?? null;
      const s = out.find((x) => x.title === title);
      if (s) s.switches.push(w);
      else out.push({ title, switches: [w] });
    }
    return out;
  });
  const filter = $derived(filterOf(list));
  /** Save as crate (the header's crate with a plus): the conditions that apply, each with its Any or All, a new crate
   *  in the panel's editor (the architecture audit, 2026-10-29: Any or All was left behind). */
  function saveAsCrate() {
    const conditions = activeConditions(list).map(({ axis, not, value, quantifier }) => ({ axis, not, value, ...(quantifier ? { quantifier } : {}) }));
    onclose();
    startCrate('', conditions);
  }
  /** The title's end: Save as crate where there are conditions, then Reset. */
  const ends = $derived<SheetAction[]>([
    ...(conditioned ? [{ icon: 'cratePlus' as const, label: 'Save as crate', disabled: !activeConditions(list).length, onclick: saveAsCrate }] : []),
    { icon: 'reset', label: 'Reset', disabled: !filter.conditions.length && !filtersOn(list), onclick: () => resetFilter(list) },
  ]);
  // The axes and their values with their counts in the list in view, the most common first (decision 0061: over all
  // its items, so everything there is to choose from shows); a server from before it: the whole library's, uncounted.
  let axes = $state<ListAxis[] | null>(null);
  $effect(() => {
    if (!conditioned) return;
    const at = inView[list];
    const without = at && (at.list.kind === 'library' || at.list.kind === 'artists' || at.list.kind === 'crate') ? hiddenLibraries() : undefined;
    const whole = () => getCrateAxes(prefs.source || undefined, hiddenLibraries());
    (at ? filterList(at.list, [], at.source, without).then((f) => f.axes, whole) : whole()).then(
      (a) => (axes = a),
      (e) => {
        axes ??= [];
        toast.fail(e, "The filter's conditions couldn't be loaded");
      },
    );
  });

  // The servers: the status call fills the list when no playlist poll has yet.
  $effect(() => {
    getStatus()
      .then((s) => known.list.length === 0 && remember(s.sources))
      .catch(() => {
        /* the page's own polls report connection problems */
      });
  });

  // Each server's own name and its libraries, as Settings → Libraries reads them. The cards show without them until
  // they come: the list of servers alone is enough to choose one, but unnamed (not its kind first, then its own name
  // over the kind: the owner, 2026-10-24). `settingsKnown`: read, or could not be (then the kind is the name).
  // (An administrator reads them from the server's settings; anyone else from the libraries, decision 0051.)
  let settings = $state<LibrariesView | null>(null);
  let settingsKnown = $state(false);
  $effect(() => {
    let gone = false;
    (me.admin ? getSettings() : getLibraries()).then(
      (s) => {
        if (gone) return;
        settings = s;
        settingsKnown = true;
      },
      (e) => {
        if (gone) return;
        settingsKnown = true;
        toast.fail(e, "The servers' libraries couldn't be loaded");
      },
    );
    return () => (gone = true);
  });

  // A use by the icon of the page that shows it (Collection shows Music, Incoming its own), so a chip needs no second
  // word; the word is still there for screen readers.
  const USE_ICON = { music: 'library', incoming: 'incoming' } as const;
  const USE_WORD: Record<LibraryUse, string> = { music: 'Music', incoming: 'Incoming', unused: 'Not used' };

  const cards = $derived(
    known.list.map((s) => {
      const view = settings?.sources.find((v) => v.id === s.id);
      // A tab's label is its kind ("Jellyfin"); the server's own name comes with the settings. Two of one kind without
      // it are told apart by their id.
      const twin = known.list.filter((x) => x.label === s.label).length > 1;
      const own = view?.status.server_name ?? null;
      const libraries = (view?.server_libraries ?? []).map((l) => ({
        id: l.id,
        name: l.name,
        use: (view!.libraries.music.includes(l.id) ? 'music' : view!.libraries.incoming.includes(l.id) ? 'incoming' : 'unused') as LibraryUse,
      }));
      const name = own ?? (twin ? `${s.label} (${s.id})` : s.label);
      return { id: s.id, name: settingsKnown ? name : '', said: name, kind: own ? s.label : null, playing: s.playing, libraries };
    }),
  );

  // On the Library (decision 0034) and on Crates every used library's chip is a switch; elsewhere a chip only shows the
  // use.
  const switchable = (use: LibraryUse) => (page === 'library' || page === 'crates') && use !== 'unused';
  const hasIncoming = $derived(cards.some((c) => c.libraries.some((l) => l.use === 'incoming')));
  const followedId = $derived(known.list.some((s) => s.id === prefs.source) ? prefs.source : noChoice());

  function follow(id: string) {
    prefs.source = prefs.source === id ? noChoice() : id;
  }

</script>

<MenuSheet
  label="Filter"
  icon="funnel"
  title="Filter"
  end={ends}
  {onclose}
>
  <div class="cards">
    <!-- While a changed filter's answer is on its way: the last count, dimmed, the loading sign after it (the owner,
         2026-10-27: it took long with nothing saying so). -->
    {#if counted && filtersOn(list)}<p class="count" class:pending={counted.pending} aria-busy={counted.pending}>
        {counted.shown.toLocaleString()} of {counted.total.toLocaleString()}{#if counted.pending}<MarkLoader size={18} />{/if}
      </p>{/if}
    {#if conditioned}
      <div>
        <ConditionList bind:conditions={() => filter.conditions, (c) => setConditions(list, c)} {axes} {items} bpms={tempos} />
      </div>
    {/if}
    <!-- The switches, in their sections where the list has them (Discover's Sources and Leave out, the owner
         2026-10-24), each section headed as a settings group is. -->
    {#each sections as sec (sec.title ?? '')}
      <div>
        {#if sec.title}<h3 class="head">{sec.title}</h3>{/if}
        <div class="toggles">
          {#each sec.switches as w (w.key)}
            <Switch card label={w.label} checked={switchOn(list, w.key) !== !!w.invert} onchange={(on) => setSwitch(list, w.key, on !== !!w.invert)} />
          {/each}
        </div>
      </div>
    {/each}

    <!-- The servers and libraries hold for the lists of music; Discover's suggestions are from none of them. -->
    {#each list === 'discover' ? [] : cards as c (c.id)}
      {@const on = prefs.source === c.id}
      <div class="server" class:on>
        <button
          class="pick"
          aria-pressed={on}
          aria-label={[c.said, c.kind, c.playing ? 'playing' : null].filter(Boolean).join(', ')}
          onclick={() => follow(c.id)}
        >
          <Icon name="server" size={24} />
          <span class="names">
            <span class="name">{c.name}{#if c.playing}<Icon name="play" size={12} />{/if}</span>
            {#if c.kind}<span class="kind">{c.kind}</span>{/if}
          </span>
          {#if on}<Icon name="check" size={22} />{/if}
        </button>
        {#if c.libraries.length}
          <ul class="libs">
            {#each c.libraries as l (l.id)}
              {#if c.id === followedId && switchable(l.use) && l.use !== 'unused'}
                {@const shown = libraryShown(c.id, l.id)}
                {@const last = shown && c.libraries.filter((x) => switchable(x.use) && libraryShown(c.id, x.id)).length === 1}
                <li>
                  <button class="lib" class:off={!shown} aria-pressed={shown} disabled={last} aria-label={`${l.name}, ${USE_WORD[l.use]}`} onclick={() => showLibrary(c.id, l.id, !shown)}
                    ><Icon name={USE_ICON[l.use]} size={16} /><span>{l.name}</span></button
                  >
                </li>
              {:else}
                <li class="tag" class:unused={l.use === 'unused'} aria-label={`${l.name}, ${USE_WORD[l.use]}`}>
                  {#if l.use !== 'unused'}<Icon name={USE_ICON[l.use]} size={16} />{/if}<span>{l.name}</span>
                </li>
              {/if}
            {/each}
          </ul>
        {/if}
      </div>
    {/each}
  </div>
</MenuSheet>


<style>
  /* Symfonium's sheet: rounded cards on the sheet, a little apart. */
  .cards {
    --card: color-mix(in srgb, var(--text) 8%, transparent);
    --card-hover: color-mix(in srgb, var(--text) 13%, transparent);
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 12px;
    padding: 4px 12px 8px;
  }

  /* The switches share one card: each a row of it, a hairline apart, the card's corners on the first and last. */
  .toggles {
    display: grid;
    gap: 3px;
  }

  /* A section of switches, headed as a settings group (lib/settings/Group). */
  .head {
    margin: 0 4px 6px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }

  .count {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 20px;
    margin: 0 8px;
    color: var(--muted);
    font-size: 14px;
  }

  .count.pending {
    color: color-mix(in srgb, var(--muted) 60%, transparent);
  }

  /* A server's card: the followed one bordered in the accent (the row's "ticked" state) and checked. */
  .server {
    position: relative;
    display: grid;
    gap: 12px;
    padding: 14px 16px;
    border: 2px solid transparent;
    border-radius: 20px;
    background: var(--card);
  }

  .server:hover {
    background: var(--card-hover);
  }

  .server.on {
    border-color: var(--art-accent, var(--accent));
  }

  .server:has(.pick:focus-visible) {
    outline: 2px solid var(--art-accent, var(--accent));
    outline-offset: 2px;
  }

  .pick {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    padding: 0;
    background: none;
    border: 0;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .pick:focus-visible {
    outline: none;
  }

  /* The whole card is the button: the chips are only shown, so a tap on them follows the server too. */
  .pick::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: 18px;
  }

  .pick :global(svg) {
    flex: none;
    color: var(--art-accent, var(--accent));
  }

  .names {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    font-size: 18px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name :global(svg) {
    margin-left: 8px;
    vertical-align: middle;
  }

  .kind {
    color: var(--muted);
    font-size: 0.85rem;
  }

  /* Under the name, as Symfonium's library chips: the use's icon and the library's name; a library not used is only
     outlined. */
  .libs {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    padding: 0 0 0 40px;
    list-style: none;
  }

  .libs li {
    display: flex;
  }

  .tag,
  .lib {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 32px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: color-mix(in srgb, var(--art-accent, var(--accent)) 18%, transparent);
    color: inherit;
    font: inherit;
    font-size: 0.9rem;
  }

  /* A switch: on as the chips are, off only outlined; the last one on cannot be switched off. Above the card's own tap
     area (.pick::after), which covers the whole card. */
  .lib {
    position: relative;
    z-index: 1;
    cursor: pointer;
  }

  .lib.off {
    border-color: var(--border);
    background: none;
    color: var(--muted);
  }

  .lib:disabled {
    cursor: default;
  }

  .libs li.unused {
    border-color: var(--border);
    background: none;
    color: var(--muted);
  }

  .libs :global(svg) {
    flex: none;
  }
</style>
