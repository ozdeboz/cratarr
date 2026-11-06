<script lang="ts">
  // A playlist's Info (the owner, 2026-11-02: a step of its menu, as a track's, no longer a tab of the panel): how
  // focused it is beside the others, and the playlists closest to it, each opening over it.
  import { insights } from './insights.svelte';
  import Asking from './Asking.svelte';
  import Notice from './Notice.svelte';
  import { shownPlaylists } from './output.svelte';
  import PlaylistItem from './PlaylistItem.svelte';

  let { pid, onopen }: { pid: string; onopen: (name: string) => void } = $props();

  $effect(() => insights.load());
  const i = $derived(insights.indexOf(pid));
  const pt = $derived(i >= 0 ? insights.shown!.playlists[i] : null);
  const near = $derived(i >= 0 ? insights.closest(i) : []);
</script>

<div class="about" aria-label={`About ${pt?.name ?? pid}`}>
  {#if !pt}
    <!-- Failed: said, never the loader for ever (the owner, 2026-11-02: an error is never empty nor endless). -->
    {#if insights.shown}<p class="note">Nothing is known about it yet.</p>{:else if insights.error}<Notice text={insights.error} onretry={() => insights.load()} />{:else}<Asking />{/if}
  {:else}
    {#if pt.focus !== null && insights.focus[i] !== undefined}
      <p class="note">More focused than {Math.round(100 * insights.focus[i])}% of your playlists</p>
    {/if}
    {#if near.length}
      <h3 class="list-head">Closest playlists</h3>
      <ul class="plain">
        {#each near as c (c.index)}
          <li>
            <PlaylistItem
              p={shownPlaylists().find((x) => x.id === c.name) ?? { id: c.name, name: c.name }}
              size="large"
              sub={c.shares}
              label={`View ${c.name}`}
              onclick={() => onopen(c.name)}
            />
          </li>
        {/each}
      </ul>
    {:else if pt.focus === null}
      <p class="note">Nothing is known about it yet.</p>
    {/if}
  {/if}
</div>

<style>
  .about {
    padding: 4px 8px 16px;
  }

  .note {
    margin: 4px 8px;
    color: var(--muted);
    font-size: 13px;
  }

  /* A list's heading (Closest playlists): small, over its large rows. */
  .list-head {
    margin: 12px 8px 4px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 500;
  }

  ul.plain {
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
