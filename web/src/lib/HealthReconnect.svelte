<script lang="ts">
  // A track of Missing music, what it is now by a person's word (A5): what no rule could pair, a title and a file name changed at
  // once, say. The library searched by what the track was, as a playlist's missing line is (lib/HealthMissing), the words free to
  // change; a found track, tapped and said yes to, takes it for good, so its plays, hearts and lines follow.
  import { untrack } from 'svelte';
  import { albumCoverUrl, findTracks, reconnectTrack, type FoundTrack } from './api';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import Notice from './Notice.svelte';
  import ConfirmSheet from './settings/ConfirmSheet.svelte';
  import { toast } from './toast.svelte';

  let { track, title, ongone }: { track: string; title: string; ongone: () => void } = $props();

  let query = $state(untrack(() => title));
  let found = $state<FoundTrack[] | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let asking = $state<FoundTrack | null>(null);

  async function search() {
    if (!query.trim()) return;
    busy = true;
    try {
      found = await findTracks(query);
      error = null;
    } catch (e) {
      found = null;
      error = e instanceof Error ? e.message : "The library couldn't be searched.";
    } finally {
      busy = false;
    }
  }
  $effect(() => void untrack(search));

  async function reconnect(t: FoundTrack) {
    try {
      await reconnectTrack(track, t.id);
      ongone();
    } catch (e) {
      toast.fail(e, "It wasn't reconnected");
    }
  }
</script>

<form class="search" onsubmit={(e) => (e.preventDefault(), search())}>
  <Icon name="search" size={18} />
  <input bind:value={query} aria-label="Search the library" />
  <button type="submit" disabled={busy}>Search</button>
</form>
{#if error}
  <Notice text={error} onretry={search} />
{:else if found}
  {#if !found.length}
    <p class="muted">Nothing found.</p>
  {:else}
    <ul class="found">
      {#each found as t (t.id)}
        <li>
          <button onclick={() => (asking = t)}>
            <span class="cover"><Cover src={albumCoverUrl('cratarr', t.album_id)} /></span>
            <span class="words"><b>{t.title}</b><small>{t.artist} · {t.album}</small></span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}
{#if asking}
  {@const t = asking}
  <ConfirmSheet title={`${title} is ${t.title}?`} text="Its plays and favourites move there, for good." label="Reconnect" onconfirm={() => reconnect(t)} onclose={() => (asking = null)} />
{/if}

<style>
  .muted {
    margin: 0;
    color: var(--muted);
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 4px 4px 12px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    color: var(--muted);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    outline: none;
  }

  .search button {
    padding: 6px 14px;
    border: 0;
    border-radius: 999px;
    background: var(--accent);
    color: var(--accent-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .found {
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
  }

  .found button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 4px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .found button:hover {
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .cover {
    flex: none;
    width: 40px;
    height: 40px;
    overflow: hidden;
    border-radius: 6px;
  }

  .words {
    display: grid;
    min-width: 0;
  }

  .words b,
  .words small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .words b {
    font-weight: 500;
  }

  .words small {
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>
