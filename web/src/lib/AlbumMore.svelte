<script lang="ts">
  // More from the artist (as Symfonium's, decided 2026-10-16): under an album's tracks, the artist's other albums of
  // the same server, newest first, a row that scrolls sideways. A cover views its album; the heading (›) the artist's
  // panel.
  import { artistCredits, goToArtist, library } from './albums.svelte';
  import AlbumCard from './AlbumCard.svelte';
  import SectionHead from './SectionHead.svelte';

  let { source, album, artist }: { source: string; album: string; artist: string } = $props();

  const others = $derived.by(() => {
    const name = artist.toLowerCase();
    const s = library.view?.sources.find((x) => x.id === source);
    return (s?.albums ?? [])
      .filter((a) => a.id !== album && (a.artists?.some((x) => x.toLowerCase() === name) || artistCredits(a.artist).includes(name)))
      .sort((a, b) => (b.year ?? 0) - (a.year ?? 0) || a.title.localeCompare(b.title));
  });
</script>

{#if others.length}
  <section aria-label="More from {artist}">
    <SectionHead title="More from {artist}" onmore={() => goToArtist(source, artist)} />
    <ul>
      {#each others as a (a.id)}<li><AlbumCard {source} album={a} /></li>{/each}
    </ul>
  </section>
{/if}

<style>
  section {
    margin-top: 24px;
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
</style>
