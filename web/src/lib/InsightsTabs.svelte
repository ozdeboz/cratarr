<script lang="ts">
  // Insights' tabs (decision 0065): Collection, Listening, Map, as a pill group in the page's overlay row beside the
  // back button (where other pages have their pill, lib/TopPill), in the same glass; the chosen one filled. A tab's own
  // sections are a row attached below, the pill growing to hold it (the owner, 2026-10-23): Collection's, Listening's. How a tab is
  // viewed (a period, a filter) is not here but in Insights' panel (lib/InsightsPanel). All remembered.
  import { prefs } from './prefs.svelte';

  const TABS = [
    { id: 'collection', label: 'Collection' },
    { id: 'listening', label: 'Listening' },
    { id: 'map', label: 'Map' },
  ] as const;
  const LISTENING = [
    { id: 'history', label: 'History' },
    { id: 'profile', label: 'Profile' },
  ] as const;
  const VIEWS = [
    { id: 'overview', label: 'Overview' },
    { id: 'genres', label: 'Genres' },
    { id: 'artists', label: 'Artists' },
    { id: 'playlists', label: 'Playlists' },
  ] as const;
</script>

<div class="pill">
  <div class="row" role="tablist" aria-label="Insights">
    {#each TABS as t (t.id)}
      <button role="tab" aria-selected={prefs.insightsTab === t.id} onclick={() => (prefs.insightsTab = t.id)}>{t.label}</button>
    {/each}
  </div>
  {#if prefs.insightsTab === 'collection'}
    <div class="row second">
      {#each VIEWS as v (v.id)}
        <button aria-pressed={prefs.insightsCollection === v.id} onclick={() => (prefs.insightsCollection = v.id)}>{v.label}</button>
      {/each}
    </div>
  {:else if prefs.insightsTab === 'listening'}
    <div class="row second">
      {#each LISTENING as v (v.id)}
        <button aria-pressed={prefs.listeningView === v.id} onclick={() => (prefs.listeningView = v.id)}>{v.label}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .pill {
    display: grid;
    gap: 2px;
    min-width: 0;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .row {
    display: flex;
    gap: 2px;
    min-width: 0;
  }

  button {
    flex: 1;
    min-width: 0;
    height: 44px;
    padding: 0 8px;
    border: 0;
    border-radius: 22px;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 15px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }

  button[aria-selected='true'] {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    color: var(--text);
  }

  /* The tab's own row: lower, its choices outlined until chosen; a line above it, inside the pill. */
  .second {
    padding-top: 2px;
    border-top: 1px solid color-mix(in srgb, var(--text) 10%, transparent);
  }

  /* Four to a row on a phone: less room around each word, so none is cut. */
  .second button {
    height: 36px;
    padding: 0 2px;
    font-size: 14px;
    letter-spacing: -0.01em;
  }

  .second button[aria-pressed='true'] {
    background: color-mix(in srgb, var(--text) 12%, transparent);
    color: var(--text);
  }
</style>
