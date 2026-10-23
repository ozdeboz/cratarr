<script lang="ts">
  // Insights' own panel (the owner, 2026-10-23): how the open tab is viewed, out of the tabs' pill. What it holds is the
  // tab's controls (lib/insightsControls), each kind drawn one way here, as the settings' rows. Opened by the page row's
  // button, closed by its ✕; beside the page at a fixed width on a roomy window, never over it there (no place to
  // choose), over the page on a phone.
  import Icon from './Icon.svelte';
  import Seg from './Seg.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import { insightsControls } from './insightsControls.svelte';
  import { closePlaylist } from './panel.svelte';
  import { prefs } from './prefs.svelte';

  const TITLES = { collection: 'Collection', listening: 'Listening', map: 'Map' } as const;
  const SUBS: Record<string, string> = { overview: 'Overview', genres: 'Genres', artists: 'Artists', tracks: 'Tracks', history: 'History', profile: 'Profile' };
  // Where the panel's controls apply, as one breadcrumb (the owner, 2026-10-23): the tab, then its section.
  const sub = $derived(prefs.insightsTab === 'collection' ? SUBS[prefs.insightsCollection] : prefs.insightsTab === 'listening' ? SUBS[prefs.listeningView] : null);
  const groups = $derived(insightsControls());
</script>

<div class="view">
  <div class="bar">
    <span class="navpill"><button aria-label="Close" title="Close" onclick={() => closePlaylist()}><Icon name="close" size={22} /></button></span>
    <h2 class="title">{#if sub}<span class="up">{TITLES[prefs.insightsTab]}</span><span class="sep" aria-hidden="true">›</span>{sub}{:else}{TITLES[prefs.insightsTab]}{/if}</h2>
  </div>
  <div class="scroll">
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
</div>

<style>
  /* As a part of the settings in the panel (lib/SettingsPanel): its bar on top, what it holds under it. */
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 65px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 20px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .up {
    color: var(--muted);
  }

  .sep {
    margin: 0 8px;
    color: var(--muted);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px clamp(12px, 2vw, 24px) calc(var(--mini-h, 0px) + 16px);
  }

  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--text);
    cursor: pointer;
  }
</style>
