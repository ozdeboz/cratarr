<script lang="ts">
  // Downloads in the installed page (0.9.8, lib/offline): what this browser holds, album by album, each its cover, its
  // name, how many tracks and how much, in the order kept (newest first). A tap plays it (from this browser, no server
  // needed); its trash takes it out. Settings › Sync says the quality and the total.
  import { albumCoverUrl } from './api';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import { plural } from './insights.svelte';
  import { playQueue } from './localPlayer.svelte';
  import { forgetOffline, offline, type KeptTrack } from './offline.svelte';
  import Row from './Row.svelte';
  import { downloadedSize } from './settings/editor.svelte';

  type Group = { key: string; title: string; artist: string | null; source: string; album: string | null; tracks: KeptTrack[]; bytes: number; at: number };
  const groups = $derived.by(() => {
    const by = new Map<string, Group>();
    for (const t of Object.values(offline.kept)) {
      const key = `${t.source}/${t.album_id ?? t.id}`;
      const g = by.get(key) ?? { key, title: t.album ?? t.name, artist: t.artist, source: t.source, album: t.album_id, tracks: [], bytes: 0, at: 0 };
      g.tracks.push(t);
      g.bytes += t.bytes;
      g.at = Math.max(g.at, t.at);
      by.set(key, g);
    }
    return [...by.values()].sort((a, b) => b.at - a.at);
  });
  const play = (g: Group) => playQueue(g.tracks.map(({ source, id, name, artist, album, album_id, duration_secs }) => ({ source, id, name, artist, album, album_id, duration_secs })), 0, g.title);
</script>

{#if !groups.length}
  <p class="empty">Nothing kept in this browser yet. Download…, in an album's or a playlist's menu, keeps it here.</p>
{:else}
  <ul>
    {#each groups as g (g.key)}
      <li>
        <Row size="large" title={g.title} sub={[g.artist, plural(g.tracks.length, 'track'), downloadedSize(g.bytes)].filter(Boolean).join(' · ')} label={`Play ${g.title}`} onclick={() => play(g)}>
          {#snippet front()}<span class="cover"><Cover src={g.album ? albumCoverUrl(g.source, g.album) : null} /></span>{/snippet}
          {#snippet action()}<button class="icon" aria-label={`Remove ${g.title} from this browser`} title="Remove from this browser" onclick={() => void forgetOffline(g.tracks)}
              ><Icon name="trash" size={22} /></button
            >{/snippet}
        </Row>
      </li>
    {/each}
  </ul>
{/if}

<style>
  ul {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .cover {
    display: block;
    width: 48px;
    height: 48px;
    overflow: hidden;
    border-radius: 4px;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .empty {
    margin-top: 32px;
    text-align: center;
    color: var(--muted);
  }
</style>
