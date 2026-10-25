<script lang="ts">
  // An album in a panel's page (an album's More from, an artist's Albums and Appears on; as Symfonium's): its cover,
  // title and year (or the `lines` given: Home's artist and when it came). A tap views it over the page (back returns); a long-press or a right-click opens its menu (decision
  // 0058).
  import { albumCoverUrl, type ListedAlbum } from './api';
  import AlbumMenu from './AlbumMenu.svelte';
  import BandcampAlbumMenu from './BandcampAlbumMenu.svelte';
  import { BANDCAMP } from './bandcampPreview';
  import Cover from './Cover.svelte';
  import { longpress } from './longpress';
  import { albumSubject, viewOver } from './panel.svelte';
  import { libraryDot } from './albums.svelte';

  let { source, album, lines = null }: { source: string; album: Pick<ListedAlbum, 'id' | 'title' | 'year'> & Partial<ListedAlbum>; lines?: string[] | null } = $props();
  let menu = $state(false);
  // Its library, as a dot before its title (0.9.3): as the Library's lists show it.
  const dot = $derived(libraryDot(source, album));
</script>

{#if menu && source === BANDCAMP}<BandcampAlbumMenu album={{ id: album.id, title: album.title, artist: album.artist ?? null }} onclose={() => (menu = false)} />
{:else if menu}<AlbumMenu {source} album={{ ...album, artist: album.artist ?? null }} onclose={() => (menu = false)} />{/if}
<button class="card" onclick={() => viewOver(albumSubject(source, album.id))} aria-label="View {album.title}" use:longpress={() => (menu = true)}>
  <span class="cover"><Cover src={albumCoverUrl(source, album.id)} /></span>
  <span class="title">{#if dot}<i class="dot" style:background={dot.colour} role="img" aria-label={dot.name}></i>{/if}{album.title}</span>
  {#if lines}{#each lines.filter(Boolean) as l, i (i)}<span class="year">{l}</span>{/each}{:else if album.year}<span class="year">{album.year}</span>{/if}
</button>

<style>
  /* The library's dot, as an album list's tile has it. */
  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-right: 6px;
    border-radius: 50%;
    vertical-align: 2px;
  }

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
    /* Square whatever the image's shape: without it a portrait cover stretched its box, and its card (the owner,
       2026-10-24). */
    overflow: hidden;
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
