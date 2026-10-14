<script lang="ts">
  // Crates (decision 0024): the second step after Incoming. Dynamic cross-sections of the library (Favourites, Unlisted,
  // Unlisted favourites), each a chip with its live count, styled as the playlist chips; a tap opens it in the panel
  // (decision 0023), where it plays and is sorted like a playlist, and can be saved as one.
  import { crates, CRATE, loadCrates } from './lib/crates.svelte';
  import { shownPlaylists } from './lib/output.svelte';
  import { openPlaylist, panel } from './lib/panel.svelte';
  import Notice from './lib/Notice.svelte';

  // Asked for when the page shows, and again when the playlists change (sorting a track shrinks Unlisted).
  const listsKey = $derived(shownPlaylists().map((p) => p.count).join(','));
  $effect(() => {
    void listsKey;
    loadCrates();
  });
</script>

<main>
  {#if crates.error && !crates.list}
    <Notice text={crates.error} />
  {:else if !crates.list}
    <p class="empty">Loading…</p>
  {:else}
    <ul class="grid">
      {#each crates.list as c (c.id)}
        <li>
          <button class="chip" class:open={panel.open === CRATE + c.id} aria-label={`Open ${c.name}`} onclick={() => openPlaylist(CRATE + c.id)}>
            <span class="name">{c.name}</span>
            <span class="n">{c.count === null ? '–' : c.count.toLocaleString()}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</main>

<style>
  main {
    padding: 16px clamp(12px, 2vw, 32px) 48px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* As the playlist chips: tinted in the accent; the crate open in the panel has a border. */
  .chip {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 48px;
    padding: 6px 14px 6px 12px;
    border: 0;
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .chip.open {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .n {
    flex: none;
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
    opacity: 0.65;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }
</style>
