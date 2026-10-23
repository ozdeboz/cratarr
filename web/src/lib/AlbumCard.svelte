<script lang="ts">
  // An album in a panel's page (an album's More from, an artist's Albums and Appears on; as Symfonium's): its cover,
  // title and year (or the `lines` given: Home's artist and when it came). A tap views it over the page (back returns); a long-press or a right-click opens its menu (decision
  // 0058).
  import { albumCoverUrl, type ListedAlbum } from './api';
  import AlbumMenu from './AlbumMenu.svelte';
  import Cover from './Cover.svelte';
  import { longpress } from './longpress';
  import { albumSubject, viewOver } from './panel.svelte';

  let { source, album, lines = null }: { source: string; album: Pick<ListedAlbum, 'id' | 'title' | 'year'> & Partial<ListedAlbum>; lines?: string[] | null } = $props();
  let menu = $state(false);
</script>

{#if menu}<AlbumMenu {source} album={{ ...album, artist: album.artist ?? null }} onclose={() => (menu = false)} />{/if}
<button class="card" onclick={() => viewOver(albumSubject(source, album.id))} aria-label="View {album.title}" use:longpress={() => (menu = true)}>
  <span class="cover"><Cover src={albumCoverUrl(source, album.id)} /></span>
  <span class="title">{album.title}</span>
  {#if lines}{#each lines.filter(Boolean) as l, i (i)}<span class="year">{l}</span>{/each}{:else if album.year}<span class="year">{album.year}</span>{/if}
</button>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .cover {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    margin-bottom: 6px;
    border-radius: 8px;
  }

  .title,
  .year {
    width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    font-size: 15px;
  }

  .year {
    color: var(--muted);
    font-size: 13px;
  }
</style>
