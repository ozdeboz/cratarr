<script lang="ts">
  // More from the artist (as Symfonium's, decided 2026-10-16): under an album's tracks, the artist's other albums of
  // the same server, newest first, a row that scrolls sideways. A cover views its album; the heading (›) goes to the
  // artist: the Library with their albums.
  import { albumCoverUrl } from './api';
  import { artistCredits, goToArtist, library } from './albums.svelte';
  import Icon from './Icon.svelte';
  import { albumSubject, viewPlaylist } from './panel.svelte';

  let { source, album, artist }: { source: string; album: string; artist: string } = $props();

  const others = $derived.by(() => {
    const name = artist.toLowerCase();
    const s = library.view?.sources.find((x) => x.id === source);
    return (s?.albums ?? [])
      .filter((a) => a.id !== album && (a.artists?.some((x) => x.toLowerCase() === name) || artistCredits(a.artist).includes(name)))
      .sort((a, b) => (b.year ?? 0) - (a.year ?? 0) || a.title.localeCompare(b.title));
  });
  let noCover = $state<Record<string, true>>({});
</script>

{#if others.length}
  <section aria-label="More from {artist}">
    <button class="head" onclick={() => goToArtist(artist)}>
      <h2>More from {artist}</h2>
      <Icon name="chevronRight" />
    </button>
    <ul>
      {#each others as a (a.id)}
        <li>
          <button onclick={() => viewPlaylist(albumSubject(source, a.id))} aria-label="View {a.title}">
            {#if noCover[a.id]}
              <span class="cover none"><Icon name="albums" size={40} /></span>
            {:else}
              <img class="cover" src={albumCoverUrl(source, a.id)} alt="" loading="lazy" decoding="async" onerror={() => (noCover[a.id] = true)} />
            {/if}
            <span class="title">{a.title}</span>
            {#if a.year}<span class="year">{a.year}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  section {
    margin-top: 24px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 8px 12px 8px 16px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  h2 {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: 18px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  ul {
    display: flex;
    gap: 12px;
    margin: 0;
    padding: 4px 16px 0;
    overflow-x: auto;
    overscroll-behavior-x: contain;
    scroll-padding-inline: 16px;
    scroll-snap-type: x proximity;
    scrollbar-width: none;
    list-style: none;
  }

  li {
    flex: none;
    width: 140px;
    scroll-snap-align: start;
  }

  li button {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: start;
    cursor: pointer;
  }

  .cover {
    width: 140px;
    height: 140px;
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
    font-size: 14px;
    font-weight: 500;
  }

  .year {
    color: var(--muted);
    font-size: 12px;
  }
</style>
