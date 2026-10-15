<script lang="ts">
  // Which source this browser follows. Shows only when more than one source is connected.
  import { getStatus } from './api';
  import { prefs } from './prefs.svelte';
  import { known, noChoice, remember } from './sources.svelte';

  $effect(() => {
    getStatus()
      .then((s) => known.list.length === 0 && remember(s.sources))
      .catch(() => {
        /* the page's own polls report connection problems */
      });
  });

  // Two sources of the same kind are told apart by their id.
  const name = (id: string, label: string) => (known.list.filter((s) => s.label === label).length > 1 ? `${label} (${id})` : label);
  // A saved choice whose source is gone falls back to no choice: Cratarr's own, or else automatic (the server follows
  // whichever started playing most recently).
  const value = $derived(known.list.some((s) => s.id === prefs.source) ? prefs.source : noChoice());
</script>

{#if known.list.length > 1}
  <label class="picker">
    <span>Server</span>
    <select value={value} onchange={(e) => (prefs.source = e.currentTarget.value)}>
      {#if !noChoice()}<option value="">Automatic</option>{/if}
      {#each known.list as s (s.id)}
        <option value={s.id}>{name(s.id, s.label)}{s.playing ? ' ▶' : ''}</option>
      {/each}
    </select>
  </label>
{/if}

<style>
  .picker {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 0.82rem;
    color: var(--muted);
  }

  select {
    flex: 1;
    min-width: 0;
    min-height: 36px;
    padding: 0 10px;
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 8px;
    font: inherit;
  }
</style>
