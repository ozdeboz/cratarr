<script lang="ts">
  // Crates (decision 0024): the second step after Incoming. Dynamic cross-sections of the library (the permanent
  // Unlisted, and one's own), each a big card (lib/CrateCard, decided 2026-10-16) with its picture and what it holds;
  // a tap views it in the panel (decision 0023), where it plays and is sorted like a playlist, and can be saved as one.
  import { prefs } from './lib/prefs.svelte';
  import ListHead from './lib/ListHead.svelte';
  import { CRATE_ORDERS, howMany, usePageAction } from './lib/pageView.svelte';
  import { crates, CRATE, EDIT_CRATE, loadCrates, startCrate } from './lib/crates.svelte';
  import CrateCard from './lib/CrateCard.svelte';
  import { shownPlaylists } from './lib/output.svelte';
  import { viewPlaylist, panel } from './lib/panel.svelte';
  import Notice from './lib/Notice.svelte';
  import PageLoading from './lib/PageLoading.svelte';

  // Asked for when the page shows, and again when the playlists change (sorting a track shrinks Unlisted).
  const listsKey = $derived(shownPlaylists().map((p) => p.count).join(','));
  $effect(() => {
    void listsKey;
    loadCrates();
  });

  // New crate: named in the same sheet as a new playlist, then built in the panel.
  // New crate: the panel's crate edit mode, empty, the caret in its name (as New playlist).
  usePageAction(() => ({ icon: 'plus', label: 'New crate', run: () => startCrate('') }));
  // The list's order (its header, or View): as made, or by name; either way round.
  const sorted = $derived.by(() => {
    const list = [...(crates.list ?? [])];
    if (prefs.crateOrder === 'name') list.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base', numeric: true }));
    return prefs.crateDown ? list.reverse() : list;
  });
</script>

<main>
  {#if crates.error && !crates.list}
    <Notice text={crates.error} />
  {:else if !crates.list}
    <PageLoading />
  {:else}
    <ListHead fields={CRATE_ORDERS} bind:value={prefs.crateOrder} bind:down={prefs.crateDown} count={howMany(sorted.length, 'crate')} />
    <ul class="grid">
      {#each sorted as c (c.id)}
        <li>
          <CrateCard crate={c} active={panel.viewed === CRATE + c.id || panel.viewed === EDIT_CRATE + c.id} onclick={() => viewPlaylist(CRATE + c.id)} />
        </li>
      {/each}
    </ul>
  {/if}
</main>


<style>
  /* The page answers to its own width (beside the player or not): a container, as Playlists. */
  main {
    position: relative;
    padding: 14px clamp(12px, 2vw, 32px) 48px;
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
</style>
