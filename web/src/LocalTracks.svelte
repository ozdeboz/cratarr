<script lang="ts">
  // Local mode's Tracks (decision 0052): every track of the device's own music, newest first, as the panel's rows (the
  // cover, the title, the artist and the album); the page's search filters them. A tap plays them from there; a swipe
  // right plays one next; its ⋮ has what local music has: Play next, Add to queue, its album and its artist.
  import { coverUrl } from './lib/api';
  import { askLibrary, asLocal, device, DEVICE, type DeviceTrack } from './lib/deviceMusic.svelte';
  import Icon from './lib/Icon.svelte';
  import { enqueue, local, playQueue } from './lib/localPlayer.svelte';
  import MenuEntry from './lib/MenuEntry.svelte';
  import MenuSheet from './lib/MenuSheet.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { albumSubject, artistSubject, viewPlaylist } from './lib/panel.svelte';
  import { pageView } from './lib/pageView.svelte';
  import { clockText } from './lib/remote.svelte';
  import Row from './lib/Row.svelte';
  import { swipe } from './lib/swipe';
  import SwipeBehind from './lib/SwipeBehind.svelte';
  import { fromAction } from 'svelte/attachments';
  import TrackCover from './lib/TrackCover.svelte';

  $effect(() => {
    askLibrary();
  });
  const shown = $derived.by(() => {
    const q = pageView.query.trim().toLowerCase();
    return (device.tracks ?? []).filter((t) => !q || [t.name, t.artist, t.album].some((x) => x?.toLowerCase().includes(q)));
  });
  const playingId = $derived(local.queue[local.index]?.source === DEVICE ? local.queue[local.index].id : null);

  function playFrom(t: DeviceTrack) {
    navigator.vibrate?.(8);
    const list = shown.map(asLocal);
    playQueue(list, Math.max(0, shown.indexOf(t)), 'Tracks');
  }
  const PLAY_NEXT = { icon: 'playNext', label: 'Play next' } as const;
  function playNext(t: DeviceTrack) {
    enqueue([asLocal(t)], 'next');
    return 'back' as const;
  }
  let menu = $state<DeviceTrack | null>(null);
</script>

<main>
  {#if device.denied}
    <p class="empty">Allow the app to read this device's music in Settings → This device.</p>
  {:else if !device.tracks}
    <PageLoading />
  {:else if !shown.length}
    <p class="empty">No tracks.</p>
  {:else}
    <ol class="rows">
      {#each shown as t (t.id)}
        <li class:now={t.id === playingId}>
          <SwipeBehind right={PLAY_NEXT} />
          <Row
            size="large"
            title={t.name}
            sub={[t.artist, t.album].filter(Boolean).join(' • ')}
            count={t.duration_secs ? clockText(t.duration_secs) : undefined}
            onclick={() => playFrom(t)}
            attach={fromAction(swipe, () => ({ enabled: true, right: () => playNext(t) }))}
          >
            {#snippet lead()}<TrackCover src={coverUrl({ source: DEVICE, id: t.id, album_id: t.album_id })} />{/snippet}
            {#snippet action()}<button class="tool" aria-label={`Menu of ${t.name}`} onclick={() => (menu = t)}><Icon name="moreVert" size={22} /></button>{/snippet}
          </Row>
        </li>
      {/each}
    </ol>
  {/if}
</main>

{#if menu}
  {@const t = menu}
  <MenuSheet label="Track" title={t.name} images={[coverUrl({ source: DEVICE, id: t.id, album_id: t.album_id })]} chips={t.artist ? [t.artist] : []} onclose={() => (menu = null)}>
    <div role="menu">
      <MenuEntry icon="playNext" label="Play next" onclick={() => ((menu = null), enqueue([asLocal(t)], 'next'))} />
      <MenuEntry icon="listAdd" label="Add to queue" onclick={() => ((menu = null), enqueue([asLocal(t)], 'last'))} />
      {#if t.album_id}
        {@const album = t.album_id}
        <MenuEntry icon="albums" label="Go to album" onclick={() => ((menu = null), viewPlaylist(albumSubject(DEVICE, album)))} />
      {/if}
      {#if t.artist}
        {@const artist = t.artist}
        <MenuEntry icon="user" label="Go to artist" onclick={() => ((menu = null), viewPlaylist(artistSubject(DEVICE, artist)))} />
      {/if}
    </div>
  </MenuSheet>
{/if}

<style>
  main {
    max-width: 900px;
    padding: 6px clamp(12px, 2vw, 32px) 48px;
  }

  .rows {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows li {
    position: relative;
    --row-bg: var(--bg);
  }

  .rows li.now {
    --row-bg: color-mix(in srgb, var(--accent) 18%, var(--bg));
  }

  .tool {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }
</style>
