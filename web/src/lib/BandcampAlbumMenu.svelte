<script lang="ts">
  // A Bandcamp album's menu in the panel (decision 0071, the owner 2026-10-25: "Bandcamp albums also deserve their own
  // context menu ... Want ... Go to artist"): Go to the artist (their page where the library has them, else theirs from
  // Discover, lib/discoverArtist; theirs on Bandcamp beside Open on Bandcamp), Discover's group where it is a suggestion (Want, Not interested), Open on Bandcamp, Share. The menu's
  // pieces as every menu's; what the server keeps of its own albums (heart, rating, playlists) it has none of.
  import { goToArtist, library, readLibrary } from './albums.svelte';
  import { albumCoverUrl } from './api';
  import { BANDCAMP, previewPage } from './bandcampPreview';
  import DiscoverEntries from './DiscoverEntries.svelte';
  import { DISCOVER } from './discoverArtist';
  import LinkSheet from './LinkSheet.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import { share } from './share';

  let { album, onclose }: { album: { id: string; title: string; artist: string | null }; onclose: () => void } = $props();

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

{#if link}
  <LinkSheet url={link.url} name={link.name} onclose={() => ((link = null), onclose())} />
{:else}
  <MenuSheet
    label={album.title}
    title={album.title}
    images={[albumCoverUrl(BANDCAMP, album.id)]}
    chips={album.artist ? [album.artist] : []}
    copy={[album.artist, album.title].filter(Boolean).join(' – ')}
    {onclose}
  >
    <MenuCards>
      <MenuGroup>
        {#if album.artist && inLibrary}
          <MenuEntry icon="user" label="Go to {album.artist}" onclick={act(() => goToArtist('cratarr', album.artist!))} />
        {:else if album.artist}
          <!-- Their page from Discover (lib/discoverArtist); Bandcamp's own, its site, in the third group. -->
          <MenuEntry icon="user" label="Go to {album.artist}" onclick={act(() => goToArtist(DISCOVER, album.artist!))} />
        {/if}
      </MenuGroup>
      <DiscoverEntries {page} {onclose} />
      <MenuGroup>
        {#if page}
          <MenuEntry icon="externalLink" label="Open on Bandcamp" onclick={act(() => (link = { url: page, name: 'Bandcamp' }))} />
          {#if album.artist && artistPage && !inLibrary}<MenuEntry icon="externalLink" label="{album.artist} on Bandcamp" onclick={act(() => (link = { url: artistPage, name: `${album.artist} on Bandcamp` }))} />{/if}
          <MenuEntry icon="share" label="Share" onclick={act(() => share(album.title, [album.artist, album.title].filter(Boolean).join(' – '), page))} />
        {/if}
      </MenuGroup>
    </MenuCards>
  </MenuSheet>
{/if}
