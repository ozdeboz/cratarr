<script lang="ts">
  // More from the artist (as Symfonium's, decided 2026-10-16): under an album's tracks, the artist's other albums of
  // the same server, newest first, a row that scrolls sideways. A cover views its album; the heading (›) the artist's
  // panel. `above`: the artists whose rows come before this one under the same record (MORE: one row per artist chip), whose
  // records it leaves out, so none shows twice (lib/albumMore).
  import { goToArtist, library } from './albums.svelte';
  import { moreFrom } from './albumMore';
  import AlbumCard from './AlbumCard.svelte';
  import SectionHead from './SectionHead.svelte';

  let { source, album, artist, above = [] }: { source: string; album: string; artist: string; above?: readonly string[] } = $props();

  const others = $derived(moreFrom(library.view?.sources.find((x) => x.id === source)?.albums ?? [], album, artist, above));
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
