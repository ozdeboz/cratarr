<script lang="ts">
  // Local mode's Albums (decision 0052): the device's own albums, as the Library shows albums (lib/AlbumShelf), built
  // from the tracks the app lists (lib/deviceMusic). A tap views one in the panel; its cover's Play plays it here; its
  // menu and the select bar have what local music has: Play, Shuffle, Play next, Add to queue (nothing of Cratarr's).
  // In a server's mode (decision 0098), that server's albums, read a page at a time (lib/serverMode), as Records.
  import type { ListedAlbum, ShelfSource } from './lib/api';
  import AlbumShelf from './lib/AlbumShelf.svelte';
  import { plural } from './lib/insights.svelte';
  import { askLibrary, asLocal, device, DEVICE, deviceAlbums, type DeviceTrack } from './lib/deviceMusic.svelte';
  import Icon from './lib/Icon.svelte';
  import { enqueue, playQueue } from './lib/localPlayer.svelte';
  import MenuEntry from './lib/MenuEntry.svelte';
  import MenuSheet from './lib/MenuSheet.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { albumCoverUrl } from './lib/api';
  import type { CollectionOrder } from './lib/prefs.svelte';
  import { toast } from './lib/toast.svelte';
  import DownloadEntry from './lib/DownloadEntry.svelte';
  import type { LocalTrack } from './lib/localPlayer.svelte';
  import { serverAlbums, serverAlbumTracks, serverMode, serverSource } from './lib/serverMode.svelte';

  const server = serverMode();
  const source = server ? serverSource(server.id) : DEVICE;
  let fromServer = $state<{ albums: ListedAlbum[]; error: string | null } | null>(null);
  $effect(() => {
    if (!server) return askLibrary();
    serverAlbums(source).then(
      (albums) => (fromServer = { albums, error: null }),
      (e) => (fromServer = { albums: [], error: e instanceof Error ? e.message : "Can't reach the server" }),
    );
  });
  let order = $state<CollectionOrder>('added');
  let down = $state(true);
  const sources = $derived<ShelfSource[] | null>(
    server
      ? fromServer && [{ id: source, label: server.name, supported: true, albums: fromServer.albums, error: fromServer.error, as_of: 0 }]
      : device.tracks
        ? [{ id: DEVICE, label: 'Local', supported: true, albums: deviceAlbums(), error: null, as_of: 0 }]
        : null,
  );

  const byPlace = (a: DeviceTrack, b: DeviceTrack) => (a.disc ?? 1) - (b.disc ?? 1) || (a.track ?? 0) - (b.track ?? 0);
  const tracksOf = (ids: string[]) => ids.flatMap((id) => (device.tracks ?? []).filter((t) => t.album_id === id).sort(byPlace)).map(asLocal);
  async function play(ids: string[], mode: 'play' | 'shuffle' | 'next' | 'last', name: string) {
    navigator.vibrate?.(8);
    let tracks;
    try {
      tracks = server ? await serverAlbumTracks(source, ids) : tracksOf(ids);
    } catch (e) {
      return toast.fail(e, 'Could not start playing');
    }
    if (!tracks.length) return;
    if (mode === 'next' || mode === 'last') enqueue(tracks, mode);
    else playQueue(tracks, 0, name, 0, mode === 'shuffle');
  }

  let menu = $state<ListedAlbum | null>(null);
  // A server's record's tracks, for its menu's Download (decision 0098: kept per server).
  let menuTracks = $state<LocalTrack[] | null>(null);
  $effect(() => {
    const a = menu;
    menuTracks = null;
    if (a && server) serverAlbumTracks(source, [a.id]).then((t) => menu === a && (menuTracks = t), () => {});
  });
</script>

<main>
  {#if !server && device.denied}
    <p class="empty">Allow the app to read this device's music in Settings → This device.</p>
  {:else if !sources}
    <PageLoading />
  {:else}
    <AlbumShelf
      {sources}
      empty={server ? 'No records on this server.' : 'No records on this device.'}
      bind:order
      bind:down
      onplay={(_, a) => play([a.id], 'play', a.title)}
      onmenu={(_, a) => (menu = a)}
    >
      {#snippet actions(ticked, done)}
        {@const ids = ticked.map((x) => x.a.id)}
        <button aria-label="Play" title="Play" disabled={!ids.length} onclick={() => (play(ids, 'play', plural(ids.length, 'record')), done())}><Icon name="play" size={24} /></button>
        <button aria-label="Play next" title="Play next" disabled={!ids.length} onclick={() => (play(ids, 'next', ''), done())}><Icon name="playNext" size={24} /></button>
        <button aria-label="Add to queue" title="Add to queue" disabled={!ids.length} onclick={() => (play(ids, 'last', ''), done())}><Icon name="listAdd" size={24} /></button>
      {/snippet}
    </AlbumShelf>
  {/if}
</main>

{#if menu}
  {@const a = menu}
  <MenuSheet label="Album" title={a.title} images={[albumCoverUrl(source, a.id)]} chips={a.artist ? [a.artist] : []} onclose={() => (menu = null)}>
    <div role="menu">
      {#each [['play', 'Play'], ['shuffle', 'Shuffle'], ['playNext', 'Play next'], ['listAdd', 'Add to queue']] as [icon, label] (label)}
        <MenuEntry
          icon={icon as 'play'}
          {label}
          onclick={() => {
            menu = null;
            play([a.id], label === 'Play' ? 'play' : label === 'Shuffle' ? 'shuffle' : label === 'Play next' ? 'next' : 'last', a.title);
          }}
        />
      {/each}
      {#if server && menuTracks}<DownloadEntry tracks={menuTracks.map((t) => ({ ...t, album_id: t.album_id ?? null }))} />{/if}
    </div>
  </MenuSheet>
{/if}

<style>
  /* As wide as the page, as the Library. */
  main {
    /* Its side gutter, which Flow's covers reach past to the page's edges (lib/AlbumShelf). */
    --gutter: clamp(12px, 2vw, 32px);
    padding: 6px var(--gutter) 48px;
  }

  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }
</style>
