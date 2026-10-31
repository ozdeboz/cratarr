<script lang="ts">
  // Crates (decision 0024): the second step after Incoming. Dynamic cross-sections of the library (the built-in ones,
  // Unlisted and Home's New for you, Sounds like your week and Grave digging, 0.9.3; and one's own), each a big card (lib/CrateCard, decided 2026-10-16) with its picture and what it holds;
  // a tap views it in the panel (decision 0023), where it plays and is sorted like a playlist, and can be saved as one.
  import { panelBeside } from './lib/glass.svelte';
  import { prefs, type CrateOrder } from './lib/prefs.svelte';
  import type { Crate } from './lib/api';
  import ListHead from './lib/ListHead.svelte';
  import { CRATE_ORDERS, howMany, matchesSearch, usePageAction } from './lib/pageView.svelte';
  import { crates, CRATE, EDIT_CRATE, loadCrates, startCrate } from './lib/crates.svelte';
  import CrateCard from './lib/CrateCard.svelte';
  import CrateMenu from './lib/CrateMenu.svelte';
  import { shownPlaylists } from './lib/output.svelte';
  import { viewPlaylist, panel } from './lib/panel.svelte';
  import { switchOn } from './lib/listFilter.svelte';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { deepMatches } from './lib/deepSearch.svelte';
  import { quickCrates, quickOn } from './lib/quickCrates.svelte';
  import { reach } from './lib/reach.svelte';
  import QuickCrate from './lib/QuickCrate.svelte';

  // Asked for when the page shows, and again when the playlists change (sorting a track shrinks Unlisted).
  // And when a library is switched on or off: a crate fills from every library switched on.
  const listsKey = $derived(`${shownPlaylists().map((p) => p.count).join(',')}|${prefs.hiddenLibraries.join(',')}`);
  $effect(() => {
    void listsKey;
    loadCrates();
  });

  // New crate: named in the same sheet as a new playlist, then built in the panel.
  // New crate: the panel's crate edit mode, empty, the caret in its name (as New playlist).
  usePageAction(() => ({ icon: 'plus', label: 'New crate', run: () => startCrate('') }));
  // The list's order (its header): as made, by name, or by what its cards show (decision 0060); either way round. A
  // crate whose card is not known yet (favourites not read) comes first, as made.
  const byName = (a: Crate, b: Crate) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true });
  const stat: Partial<Record<CrateOrder, (c: Crate) => number | null | undefined>> = {
    tracks: (c) => c.stats?.tracks,
    length: (c) => c.stats?.seconds,
    unplayed: (c) => c.stats?.unplayed,
    newWeek: (c) => c.stats?.new_week,
  };
  // The pill's search (the owner, 2026-10-25): a crate whose name holds every word, or one the server finds by its tracks.
  const deepOf = deepMatches('crate', 'crate:');
  const sorted = $derived.by(() => {
    // The filter's Hide built-in crates (0.9.3): only one's own.
    const noBuiltIn = switchOn('crates', 'no-built-in');
    const list = (crates.list ?? []).filter((c) => (matchesSearch(c.name) || !!deepOf(c.id)) && !(noBuiltIn && c.permanent));
    const of = stat[prefs.crateOrder];
    if (prefs.crateOrder === 'name') list.sort(byName);
    else if (of) list.sort((a, b) => (of(a) ?? -1) - (of(b) ?? -1) || byName(a, b));
    return prefs.crateDown ? list.reverse() : list;
  });
  // A crate being grabbed (0.12.1, the owner: "Results replace the list"): the page is that crate alone, its tracks or
  // records; the crates come back once its words are cleared or it is saved.
  const grabbing = $derived(quickOn() && !reach.away && quickCrates.chips.length > 0);
  // A crate's menu (its card's long-press or right-click, decision 0058).
  let menuFor = $state<string | null>(null);
</script>

<main>
  {#if quickOn()}
    <!-- A quick crate (0.11; 0.12 the owner's mockup B): words typed at the top read as a crate's sentence (lib/QuickCrate). -->
    <QuickCrate />
  {/if}
  {#if grabbing}
    <!-- The grabbed crate's results are QuickCrate's own. -->
  {:else if crates.error && !crates.list}
    <Notice text={crates.error} />
  {:else if !crates.list}
    <PageLoading />
  {:else if sorted.length === 0}
    <!-- None left (the search or the filter): no header over an empty list, as the Library (the owner, 2026-10-25). -->
    <p class="empty">No crates match.</p>
  {:else}
    <ListHead fields={CRATE_ORDERS} bind:value={prefs.crateOrder} bind:down={prefs.crateDown} count={howMany(sorted.length, 'crate')} />
    <ul class="grid">
      {#each sorted as c (c.id)}
        <li>
          <CrateCard crate={c} active={panelBeside() && (panel.viewed === CRATE + c.id || panel.viewed === EDIT_CRATE + c.id)} onclick={() => viewPlaylist(CRATE + c.id)} onmenu={() => (menuFor = c.id)} why={deepOf(c.id)} />
        </li>
      {/each}
    </ul>
  {/if}
</main>

{#if menuFor}
  <CrateMenu id={menuFor} onclose={() => (menuFor = null)} />
{/if}


<style>
  /* The page answers to its own width (beside the player or not): a container, as Playlists. */
  main {
    position: relative;
    /* As the Library's: its list's header as near the glass (the owner, 2026-10-26: the headers alike). */
    padding: 6px clamp(12px, 2vw, 32px) 48px;
    container-type: inline-size;
  }

  /* One card across a phone, more as the page widens. */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 340px), 1fr));
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }
</style>
