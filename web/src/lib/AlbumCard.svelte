<script lang="ts">
  // An album in a panel's page (an album's More from, an artist's Albums and Appears on; as Symfonium's): its cover,
  // title and year. A tap views it over the page (back returns).
  import { albumCoverUrl, type ListedAlbum } from './api';
  import Icon from './Icon.svelte';
  import { albumSubject, viewOver } from './panel.svelte';

  let { source, album }: { source: string; album: Pick<ListedAlbum, 'id' | 'title' | 'year'> } = $props();
  let noCover = $state(false);
</script>

<button class="card" onclick={() => viewOver(albumSubject(source, album.id))} aria-label="View {album.title}">
  {#if noCover}
    <span class="cover none"><Icon name="albums" size={40} /></span>
  {:else}
    <img class="cover" src={albumCoverUrl(source, album.id)} alt="" loading="lazy" decoding="async" onerror={() => (noCover = true)} />
  {/if}
  <span class="title">{album.title}</span>
  {#if album.year}<span class="year">{album.year}</span>{/if}
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
    width: 100%;
    aspect-ratio: 1;
    margin-bottom: 6px;
    border-radius: 8px;
    object-fit: cover;
  }

  .none {
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--text) 9%, transparent);
    color: var(--muted);
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
