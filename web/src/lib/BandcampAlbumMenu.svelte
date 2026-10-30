<script lang="ts">
  // A Bandcamp album's menu in the panel (decision 0071, the owner 2026-10-25: "Bandcamp albums also deserve their own
  // context menu ... Want ... Go to artist"): Go to the artist (their page where the library has them, else theirs from
  // Discover, lib/discoverArtist; theirs on Bandcamp beside Open on Bandcamp), Discover's group (Want, and Not
  // interested where it is a suggestion), Open on Bandcamp, Share, in the table's order (lib/menuTable). Its previews
  // play and queue as any album's (0.10; the owner, 2026-10-30: "Bandcamp files cannot be queued, very annoying!"),
  // on any player: the server streams them. Never downloaded, kept or in a playlist (decision 0071): bought there. The
  // menu's pieces as every menu's; what the server keeps of its own albums (heart, rating, playlists) it has none of.
  import { goToArtist, library, readLibrary } from './albums.svelte';
  import { port } from './player/port.svelte';
  import { albumCoverUrl } from './api';
  import { BANDCAMP, previewPage } from './bandcampPreview';
  import DiscoverEntries from './DiscoverEntries.svelte';
  import { DISCOVER } from './discoverArtist';
  import LinkSheet from './LinkSheet.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import MenuTable from './MenuTable.svelte';
  import { inOrder, type EntryId } from './menuTable';
  import type { LocalTrack } from './localPlayer.svelte';
  import { share } from './share';
  import { toast } from './toast.svelte';

  let {
    album,
    tracks = [],
    onclose,
  }: {
    album: { id: string; title: string; artist: string | null };
    /** Its previews as the panel lists them: what plays and queues. */
    tracks?: LocalTrack[];
    onclose: () => void;
  } = $props();

  // This page plays them itself; another player is asked through the server, which resolves Bandcamp's ids.
  async function play(how: 'play' | 'shuffle' | 'next' | 'last') {
    const [list, name] = [[...tracks], album.title];
    onclose();
    navigator.vibrate?.(8);
    try {
      await port().playTracks(list, how, name, BANDCAMP);
      if (how === 'next' || how === 'last') toast.show({ text: how === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
  const actions = $derived(
    tracks.length
      ? inOrder('bandcamp', [
          { icon: 'play' as const, label: 'Play', onclick: () => play('play') },
          { icon: 'shuffle' as const, label: 'Shuffle', onclick: () => play('shuffle') },
          { icon: 'playNext' as const, label: 'Play next', onclick: () => play('next') },
          { icon: 'listAdd' as const, label: 'Add to queue', onclick: () => play('last') },
        ])
      : [],
  );

  const page = $derived(previewPage(album.id));
  // The artist's own page on Bandcamp: the album page's site.
  const artistPage = $derived.by(() => {
    try {
      return page ? new URL(page).origin : null;
    } catch {
      return null;
    }
  });
  if (!library.view) readLibrary(false).catch(() => {});
  const inLibrary = $derived(
    !!album.artist && (library.view?.sources ?? []).some((s) => s.albums.some((a) => a.artist?.toLowerCase() === album.artist!.toLowerCase())),
  );
  let link = $state<{ url: string; name: string } | null>(null);
  const act = (what: () => unknown) => () => {
    what();
    if (!link) onclose();
  };
</script>

{#snippet entry(e: EntryId)}
  {#if e === 'goArtist'}
    {#if album.artist && inLibrary}
      <MenuEntry icon="user" label="Go to {album.artist}" onclick={act(() => goToArtist('cratarr', album.artist!))} />
    {:else if album.artist}
      <!-- Their page from Discover (lib/discoverArtist); Bandcamp's own, its site, in Take away. -->
      <MenuEntry icon="user" label="Go to {album.artist}" onclick={act(() => goToArtist(DISCOVER, album.artist!, album))} />
    {/if}
  {:else if e === 'discover'}
    <DiscoverEntries {page} artist={album.artist} album={album.title} {onclose} />
  {:else if e === 'openOnBandcamp'}
    {#if page}<MenuEntry icon="externalLink" label="Open on Bandcamp" onclick={act(() => (link = { url: page, name: 'Bandcamp' }))} />{/if}
  {:else if e === 'artistOnBandcamp'}
    {#if page && album.artist && artistPage && !inLibrary}<MenuEntry icon="externalLink" label="{album.artist} on Bandcamp" onclick={act(() => (link = { url: artistPage, name: `${album.artist} on Bandcamp` }))} />{/if}
  {:else if e === 'share'}
    {#if page}<MenuEntry icon="share" label="Share" onclick={act(() => share(album.title, [album.artist, album.title].filter(Boolean).join(' – '), page))} />{/if}
  {/if}
{/snippet}

{#if link}
  <LinkSheet url={link.url} name={link.name} onclose={() => ((link = null), onclose())} />
{:else}
  <MenuSheet
    label={album.title}
    title={album.title}
    images={[albumCoverUrl(BANDCAMP, album.id)]}
    chips={album.artist ? [album.artist] : []}
    copy={[album.artist, album.title].filter(Boolean).join(' – ')}
    {actions}
    {onclose}
  >
    <MenuTable kind="bandcamp" {entry} />
  </MenuSheet>
{/if}
