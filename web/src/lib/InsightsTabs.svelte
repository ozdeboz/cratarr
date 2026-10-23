<script lang="ts">
  // Insights' tabs (decision 0065): Collection, Listening, Map, as a pill group in the page's overlay row beside the
  // back button (where other pages have their pill, lib/TopPill), in the same glass; the chosen one filled. The chosen
  // tab's own row is attached below it, the pill growing to hold it (the owner, 2026-10-23: "the floating tab bar should
  // extend to get an extra row attached below"): Collection's views, Listening's period, Map's filter. All remembered.
  import { prefs } from './prefs.svelte';

  const TABS = [
    { id: 'collection', label: 'Collection' },
    { id: 'listening', label: 'Listening' },
    { id: 'map', label: 'Map' },
  ] as const;
  const VIEWS = [
    { id: 'overview', label: 'Overview' },
    { id: 'genres', label: 'Genres' },
    { id: 'artists', label: 'Artists' },
    { id: 'playlists', label: 'Playlists' },
  ] as const;
  const PERIODS = [
    { id: 'week', label: 'Week' },
    { id: 'month', label: 'Month' },
    { id: 'year', label: 'Year' },
  ] as const;
  const SIGNALS = [
    { id: 'tracks', label: 'Shared tracks' },
    { id: 'genre', label: 'Genre' },
    { id: 'artists', label: 'Artists' },
  ] as const;
</script>

<div class="pill">
  <div class="row" role="tablist" aria-label="Insights">
    {#each TABS as t (t.id)}
      <button role="tab" aria-selected={prefs.insightsTab === t.id} onclick={() => (prefs.insightsTab = t.id)}>{t.label}</button>
    {/each}
  </div>
  <div class="row second">
    {#if prefs.insightsTab === 'collection'}
      {#each VIEWS as v (v.id)}
        <button aria-pressed={prefs.insightsCollection === v.id} onclick={() => (prefs.insightsCollection = v.id)}>{v.label}</button>
      {/each}
    {:else if prefs.insightsTab === 'listening'}
      {#each PERIODS as p (p.id)}
        <button aria-pressed={prefs.listeningPeriod === p.id} onclick={() => (prefs.listeningPeriod = p.id)}>{p.label}</button>
      {/each}
    {:else}
      <!-- What brings playlists together: any of them, on or off. -->
      {#each SIGNALS as s (s.id)}
        <button aria-pressed={prefs.signals[s.id]} onclick={() => (prefs.signals[s.id] = !prefs.signals[s.id])}>{s.label}</button>
      {/each}
    {/if}
  </div>
</div>

<style>
  .pill {
    display: grid;
    flex: 0 1 460px;
    gap: 2px;
    min-width: 0;
    margin-left: auto;
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

  .second button {
    height: 36px;
    font-size: 14px;
  }

  .second button[aria-pressed='true'] {
    background: color-mix(in srgb, var(--text) 12%, transparent);
    color: var(--text);
  }
</style>
