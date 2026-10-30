<script lang="ts">
  // Similar albums, at an album's foot (the owner, 2026-10-23: every list with a profile gets its own kind's similar
  // ones at the bottom): the albums of Cratarr's own library closest to it by the one similarity (the server's), as
  // More from's row (lib/AlbumMore). Left out when there are none.
  import { similarAlbums, type ListedAlbum } from './api';
  import AlbumCard from './AlbumCard.svelte';
  import { library, readLibrary } from './albums.svelte';
  import SectionHead from './SectionHead.svelte';

  let { source, album }: { source: string; album: string } = $props();

  let ids = $state<string[]>([]);
  $effect(() => {
    const [s, a] = [source, album];
    ids = [];
    if (s !== 'cratarr') return;
    if (!library.view) void readLibrary(false);
    similarAlbums(s, a).then(
      (found) => {
        if (s === source && a === album) ids = found.map((f) => f.id);
      },
      () => {},
    );
  });
  const albums = $derived.by(() => {
    const listed = library.view?.sources.find((x) => x.id === source)?.albums ?? [];
    const byId = new Map(listed.map((a) => [a.id, a]));
    return ids.flatMap((id): ListedAlbum[] => (byId.has(id) ? [byId.get(id)!] : []));
  });
</script>

{#if albums.length}
  <section aria-label="Similar albums">
    <SectionHead title="Similar albums" />
    <ul>
      {#each albums as a (a.id)}<li><AlbumCard {source} album={a} lines={[a.artist ?? '']} /></li>{/each}
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
