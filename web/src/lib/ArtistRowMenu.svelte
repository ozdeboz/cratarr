<script lang="ts">
  // An artist's menu from a list's row (the Library's Artists, its long-press; the owner, 2026-10-25: as the albums'):
  // the artist panel's own menu (lib/ArtistMenu), its tracks and heart read when it opens.
  import { artistView, type ArtistView } from './api';
  import { setArtistHeart } from './favourites.svelte';
  import ArtistMenu from './ArtistMenu.svelte';
  import type { LocalTrack } from './localPlayer.svelte';
  import { playCollection } from './play.svelte';
  import { toast } from './toast.svelte';

  let { source, name, cover, onclose }: { source: string; name: string; cover: string | null; onclose: () => void } = $props();

  let view = $state<ArtistView | null>(null);
  let favorite = $state<boolean | undefined>(undefined);
  $effect(() => {
    artistView(source, name).then(
      (v) => ((view = v), (favorite = v.favorite ?? undefined)),
      (e) => (toast.fail(e, "The artist couldn't be read"), onclose()),
    );
  });
  const tracks = $derived<LocalTrack[]>(
    (view?.rows ?? [])
      .filter((r) => r.id && !r.missing)
      .map((r) => ({ source: view!.source, id: r.id!, name: r.name ?? r.file, artist: r.artist, album: r.album, album_id: r.album_id ?? null, duration_secs: r.duration_secs })),
  );
  async function toggle() {
    const on = !favorite;
    favorite = on;
    try {
      await setArtistHeart(source, name, on);
    } catch (e) {
      favorite = !on;
      toast.fail(e, "The favourite didn't change");
    }
  }
  async function act(mode: 'play' | 'shuffle' | 'next' | 'last') {
    onclose();
    try {
      await playCollection({ kind: 'artist', source, name }, mode, name);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
</script>

{#if view}
  <ArtistMenu {source} {name} {cover} albums={view.albums.length} {tracks} {favorite} ontogglefavorite={toggle} onact={act} {onclose} />
{/if}
