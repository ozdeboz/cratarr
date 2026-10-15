<script lang="ts">
  // Crates (decision 0024): the second step after Incoming. Dynamic cross-sections of the library (the permanent
  // Unlisted, and one's own), each a medium row (lib/Row.svelte) with its live count, as the playlists; a tap views it in the panel
  // (decision 0023), where it plays and is sorted like a playlist, and can be saved as one. Where a favourite playlist's
  // chip has its small heart, a crate's shows its favourites setting (the panel's toggle sets it).
  import { prefs } from './lib/prefs.svelte';
  import ListHead from './lib/ListHead.svelte';
  import { CRATE_ORDERS, howMany, usePageAction } from './lib/pageView.svelte';
  import { crates, CRATE, EDIT_CRATE, FAVOURITES, favouritesOf, loadCrates, startCrate } from './lib/crates.svelte';
  import Icon from './lib/Icon.svelte';
  import Row from './lib/Row.svelte';
  import NameSheet from './lib/NameSheet.svelte';
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
  let naming = $state(false);
  usePageAction(() => ({ icon: 'plus', label: 'New crate', run: () => (naming = true) }));
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
        {@const fav = FAVOURITES[favouritesOf(c.id)]}
        {@const n = c.counts[favouritesOf(c.id)]}
        <li>
          <Row
            size="medium"
            title={c.name}
            count={n === null ? '–' : n.toLocaleString()}
            active={panel.viewed === CRATE + c.id || panel.viewed === EDIT_CRATE + c.id}
            label={`View ${c.name}`}
            onclick={() => viewPlaylist(CRATE + c.id)}
          >
            {#snippet meta()}
              <span class="flags"><span class="heart" class:filled={fav.filled} title={fav.label} aria-label={fav.label}><Icon name={fav.icon} size={10} /></span></span>
            {/snippet}
          </Row>
        </li>
      {/each}
    </ul>
  {/if}
</main>

{#if naming}
  <NameSheet
    title="New crate"
    what="crate"
    action="Create crate"
    onsubmit={(name) => {
      naming = false;
      startCrate(name);
    }}
    onclose={() => (naming = false)}
  />
{/if}

<style>
  /* The page answers to its own width (beside the player or not): a container, as Playlists. */
  main {
    position: relative;
    padding: 14px clamp(12px, 2vw, 32px) 48px;
    container-type: inline-size;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* In a favourite playlist's small heart's place after its count (Playlists), in the chip's accent: filled for Only
     favourites. */
  .flags {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 10px;
    margin-left: -4px;
  }

  .heart {
    display: flex;
    color: var(--accent);
  }

  .heart.filled :global(svg) {
    fill: currentColor;
  }
</style>
