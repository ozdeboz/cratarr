<script lang="ts">
  // How an Insights view is looked at (the owner, 2026-10-24: "the panel should become a sheet"): its controls
  // (lib/insightsControls: the period, the libraries, its sort, its sources), each kind drawn one way, as the settings'
  // rows. Opened by the sliders in the view's bar (lib/InsightsPanel).
  import MenuSheet from './MenuSheet.svelte';
  import Seg from './Seg.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import { insightsControls } from './insightsControls.svelte';

  let { title, onclose }: { title: string; onclose: () => void } = $props();
  const groups = $derived(insightsControls());
</script>

<MenuSheet label="View" icon="sliders" {title} {onclose}>
  <div class="controls">
    {#each groups as g, i (i)}
      <Group title={g.title}>
        {#each g.controls as c, j (j)}
          {#if c.kind === 'choice'}
            <Item name={c.name}><Seg label={c.name} options={c.options} bind:value={c.get, c.set} /></Item>
          {:else}
            <Item name={c.name} line={c.line ?? null} disabled={c.disabled} toggle={{ checked: c.get(), onchange: c.set }} />
          {/if}
        {/each}
      </Group>
    {/each}
  </div>
</MenuSheet>

<style>
  .controls {
    display: grid;
    gap: 16px;
    padding: 0 16px 16px;
  }
</style>
