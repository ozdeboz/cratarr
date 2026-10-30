<script lang="ts">
  // The panel's Export tab (decisions 0007, 0025): the selected playlists (each can be left out; more are ticked with the
  // Playlists page's Select: beside the page, on its chips; full-screen, Add playlists goes there), then the same step as
  // every menu's Download… (lib/ExportStep, decision 0058): what it will hold, the choices and the download.
  import ExportStep from './ExportStep.svelte';
  import Icon from './Icon.svelte';
  import { inSidebar } from './live.svelte';
  import { shownPlaylists } from './output.svelte';
  import PlaylistItem from './PlaylistItem.svelte';
  import { addMore, leaveOut } from './select.svelte';

  let { ids }: { ids: string[] } = $props();

  const all = $derived(shownPlaylists());
  const picked = $derived(ids.map((id) => all.find((p) => p.id === id)).filter((p) => !!p));
</script>

<div class="tab">
  <ul class="lists" aria-label="Playlists to export">
    {#each picked as p (p.id)}
      <li>
        <PlaylistItem {p} size="small">
          {#snippet action()}<button class="icon" aria-label={`Leave out ${p.name}`} onclick={() => leaveOut(p.id)}><Icon name="close" size={18} /></button>{/snippet}
        </PlaylistItem>
      </li>
    {/each}
  </ul>
  {#if !inSidebar()}
    <button class="more" onclick={addMore}><Icon name="plus" size={18} /><span>Add playlists</span></button>
  {/if}
</div>
<ExportStep what={{ kind: 'playlists', names: picked.map((p) => p.id) }} onclose={() => {}} />

<style>
  .tab {
    padding: 4px 16px 12px;
  }

  .lists {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Small rows (lib/Row.svelte). */
  .lists li + li {
    margin-top: 6px;
  }

  .more {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 40px;
    margin-top: 8px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
</style>
