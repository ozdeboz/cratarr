<script lang="ts">
  // Insights' tabs (decision 0065): Collection, Listening, Map, as a pill group in the page's overlay row beside the
  // back button (where other pages have their pill, lib/TopPill), in the same glass; the chosen one filled. The tab
  // last open is remembered (prefs).
  import { prefs } from './prefs.svelte';

  const TABS = [
    { id: 'collection', label: 'Collection' },
    { id: 'listening', label: 'Listening' },
    { id: 'map', label: 'Map' },
  ] as const;
</script>

<div class="tabs" role="tablist" aria-label="Insights">
  {#each TABS as t (t.id)}
    <button role="tab" aria-selected={prefs.insightsTab === t.id} onclick={() => (prefs.insightsTab = t.id)}>{t.label}</button>
  {/each}
</div>

<style>
  .tabs {
    display: flex;
    flex: 0 1 420px;
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

  .tabs:first-child {
    margin-left: 0;
  }

  button {
    flex: 1;
    min-width: 0;
    height: 44px;
    padding: 0 10px;
    border: 0;
    border-radius: 22px;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 15px;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
  }

  button[aria-selected='true'] {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    color: var(--text);
  }
</style>
