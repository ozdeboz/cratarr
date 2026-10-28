<script lang="ts">
  // An album in a panel's page (an album's More from, an artist's Albums and Appears on; as Symfonium's): its cover,
  // title and year (or the `lines` given: Home's artist and when it came). A tap views it over the page (back returns); a long-press or a right-click opens its menu (decision
  // 0058). Not in the library (an artist's missing albums, 0.9.5): its `cover` and its own `onopen` (Bandcamp, else
  // its menu), no menu of the library's. A library album's cover has Play in its upper right corner, which alone plays,
  // and dims with the ring while it opens, when that takes long (lib/CoverPlay, lib/CoverWait; the owner, 2026-10-27).
  import { albumCoverUrl, type ListedAlbum, type RequestMark } from './api';
  import CoverBusy from './CoverBusy.svelte';
  import CoverStatus from './CoverStatus.svelte';
  import AlbumMenu from './AlbumMenu.svelte';
  import BandcampAlbumMenu from './BandcampAlbumMenu.svelte';
  import { BANDCAMP } from './bandcampPreview';
  import Cover from './Cover.svelte';
  import { longpress } from './longpress';
  import { albumSubject, viewOver } from './panel.svelte';
  import { albumPlay, libraryDot, playListedAlbum } from './albums.svelte';
  import CoverPlay from './CoverPlay.svelte';
  import { albumAway } from './away.svelte';
  import { DEVICE } from './deviceMusic.svelte';
  import CoverWait from './CoverWait.svelte';

  let {
    source,
    album,
    lines = null,
    cover = undefined,
    onopen = null,
    busy = false,
    request = undefined,
  }: {
    source: string;
    album: Pick<ListedAlbum, 'id' | 'title' | 'year'> & Partial<ListedAlbum>;
    lines?: string[] | null;
    /** Not the library's cover (an album not in it): its own, or none. */
    cover?: string | null;
    /** Not the library's album: what a tap does instead of viewing it. */
    onopen?: (() => void) | null;
    /** Its tap is at work (looking on Bandcamp): the busy round on its cover (lib/CoverBusy). */
    busy?: boolean;
    /** Not in the library, asked for: where it stands, on its cover (lib/CoverStatus); else the album's own. */
    request?: RequestMark | null;
  } = $props();
  let menu = $state(false);
  // Its library, as a dot before its title (0.9.3): as the Library's lists show it.
  const dot = $derived(libraryDot(source, album));
  /** The library's album: it plays from its cover, and opens in the panel. */
  const own = $derived(!onopen && cover === undefined && source !== BANDCAMP);
  /** Out of reach, none of it on this device (lib/away): dimmed, no Play; it still opens, as last seen. */
  const gone = $derived(own && albumAway(source, album.id));
</script>

{#if menu && source === BANDCAMP}<BandcampAlbumMenu album={{ id: album.id, title: album.title, artist: album.artist ?? null }} onclose={() => (menu = false)} />
{:else if menu}<AlbumMenu {source} album={{ ...album, artist: album.artist ?? null }} onclose={() => (menu = false)} />{/if}
<div class="card-box">
<button
  class="card"
  class:gone
  onclick={() => (onopen ? onopen() : viewOver(albumSubject(source, album.id)))}
  aria-label="{onopen ? 'Open' : 'View'} {album.title}"
  use:longpress={() => !onopen && source !== DEVICE && (menu = true)}
>
  <span class="cover"
    ><Cover src={cover === undefined ? albumCoverUrl(source, album.id) : cover} /><CoverStatus mark={request ?? album.request} />{#if busy}<CoverBusy />{/if}<CoverWait
      subject={own ? albumSubject(source, album.id) : null}
    /></span
  >
  <span class="title">{#if dot}<i class="dot" style:background={dot.colour} role="img" aria-label={dot.name}></i>{/if}{album.title}</span>
  {#if lines}{#each lines.filter(Boolean) as l, i (i)}<span class="year">{l}</span>{/each}{:else if album.year}<span class="year">{album.year}</span>{/if}
</button>
{#if own && !gone}<CoverPlay label={album.title} album={album.id} busy={albumPlay.starting === album.id} disabled={albumPlay.starting !== null} onplay={() => playListedAlbum(source, album)} />{/if}
</div>

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

  /* The card and its Play, over the cover's corner (the cover fills the card's width at its top). */
  .card-box {
    position: relative;
    min-width: 0;
  }

  .card {
    position: relative;
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
    position: relative;
    display: block;
    width: 100%;
    aspect-ratio: 1;
    /* Square whatever the image's shape: without it a portrait cover stretched its box, and its card (the owner,
       2026-10-24). */
    overflow: hidden;
    margin-bottom: 6px;
    border-radius: 8px;
  }

  /* Out of reach and not on this device: it cannot play (as a panel's rows, lib/PlaylistView). */
  .card.gone {
    opacity: 0.45;
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
