<script lang="ts">
  // Downloads (decision 0033, step 4), in the Library's look (decided 2026-10-16: "a special case of the library page,
  // style-wise, layout-wise"): the albums kept on this device, as the app tells them, in the Library's list
  // (lib/AlbumShelf: rows or covers, the order, size, search and select). Each shows as the Library lists it when its
  // list is known (year, format, when added; the last copy kept serves without a connection), else as its download
  // says. A cover plays what is on the device, so the page plays without a connection; an album still coming says how
  // far it is. The menu is the album's one menu (Remove from this device in it).
  import { type ListedAlbum, type ShelfSource } from './lib/api';
  import AlbumShelf from './lib/AlbumShelf.svelte';
  import AlbumMenu from './lib/AlbumMenu.svelte';
  import Meter from './lib/Meter.svelte';
  import Icon from './lib/Icon.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { askDownloads, downloads, removeDownloads, type DownloadedTrack } from './lib/app.svelte';
  import { library, readLibrary } from './lib/albums.svelte';
  import { playQueue } from './lib/localPlayer.svelte';
  import { prefs } from './lib/prefs.svelte';

  askDownloads();
  // The Library's list, for what each album is (the kept copy first; without a connection that is all there is).
  if (!library.view) readLibrary(false).catch(() => {});

  type Group = { source: string; id: string; tracks: DownloadedTrack[] };
  // The downloads by album: an album's id where the track has one, else its name and artist.
  const groups = $derived.by(() => {
    const by = new Map<string, Group>();
    for (const t of downloads.tracks) {
      const id = t.album_id ?? `${t.album ?? ''}\n${t.artist ?? ''}`;
      const key = `${t.source}/${id}`;
      const g = by.get(key) ?? { source: t.source, id, tracks: [] };
      g.tracks.push(t);
      by.set(key, g);
    }
    return [...by.values()];
  });
  const listedOf = (g: Group) => library.view?.sources.find((s) => s.id === g.source)?.albums.find((a) => a.id === g.id) ?? null;
  const groupOf = (s: ShelfSource, a: ListedAlbum) => groups.find((g) => g.source === s.id && g.id === a.id);

  // One list per server, as the Library's.
  const sources = $derived.by<ShelfSource[]>(() => {
    const ids = [...new Set(groups.map((g) => g.source))];
    return ids.map((id) => ({
      id,
      label: library.view?.sources.find((s) => s.id === id)?.label ?? id,
      supported: true,
      error: null,
      as_of: 0,
      albums: groups
        .filter((g) => g.source === id)
        .map((g): ListedAlbum => {
          const listed = listedOf(g);
          const t = g.tracks[0];
          return {
            ...(listed ?? { id: g.id, title: t.album ?? '', artist: t.artist, year: null, added: null, path: null, format: null, bitrate_kbps: null, tags: [] }),
            tracks: g.tracks.length,
          };
        }),
    }));
  });

  const size = (bytes: number) => (bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`);
  // What plays is what is whole on the device.
  const whole = (g: Group | undefined) => g?.tracks.filter((t) => t.download.state === 'done') ?? [];
  const playable = (list: { s: ShelfSource; a: ListedAlbum }[]) => list.flatMap(({ s, a }) => whole(groupOf(s, a)));
  function play(s: ShelfSource, a: ListedAlbum) {
    const tracks = whole(groupOf(s, a));
    if (tracks.length) playQueue(tracks, 0, a.title);
  }

  let menu = $state<{ source: string; a: ListedAlbum } | null>(null);
  let menuShown = $state(false);
</script>

<main>
  {#if !downloads.known}
    <PageLoading />
  {:else if !groups.length}
    <p class="empty">Nothing downloaded.</p>
  {:else}
    <AlbumShelf
      {sources}
      empty="Nothing downloaded."
      bind:order={prefs.downloadsOrder}
      bind:down={prefs.downloadsDown}
      countMore={size(downloads.bytes)}
      onplay={play}
      onmenu={(s, a) => ((menu = { source: s.id, a }), (menuShown = true))}
    >
      {#snippet extra(s: ShelfSource, a: ListedAlbum)}
        {@const g = groupOf(s, a)}
        {#if g}
          {@const done = whole(g).length}
          {@const failed = g.tracks.filter((t) => t.download.state === 'failed').length}
          {@const of = listedOf(g)?.tracks}
          {#if done < g.tracks.length - failed}
            <Meter
              label={`Downloading ${done} of ${g.tracks.length}`}
              value={(done + g.tracks.filter((t) => t.download.state === 'downloading').reduce((n, t) => n + t.download.percent / 100, 0)) / g.tracks.length}
            />
          {:else if failed}
            <span class="note failed">{failed} {failed === 1 ? 'track' : 'tracks'} failed</span>
          {:else if of && of > g.tracks.length}
            <span class="note">{g.tracks.length} of {of} on this device</span>
          {/if}
        {/if}
      {/snippet}
      {#snippet actions(ticked: { s: ShelfSource; a: ListedAlbum }[], done: () => void)}
        <button
          aria-label="Play"
          title="Play"
          disabled={!playable(ticked).length}
          onclick={() => {
            const tracks = playable(ticked);
            done();
            if (tracks.length) playQueue(tracks, 0, ticked.length === 1 ? ticked[0].a.title : `${ticked.length} albums`);
          }}><Icon name="play" size={24} /></button
        >
        <button
          aria-label="Remove from this device"
          title="Remove from this device"
          disabled={!ticked.length}
          onclick={() => {
            const ids = ticked.flatMap(({ s, a }) => groupOf(s, a)?.tracks.map((t) => t.download.id) ?? []);
            done();
            removeDownloads(ids);
          }}><Icon name="trash" size={24} /></button
        >
      {/snippet}
    </AlbumShelf>
  {/if}
</main>

{#if menuShown && menu}
  {@const m = menu}
  <AlbumMenu source={m.source} album={m.a} onclose={() => (menuShown = false)} />
{/if}

<style>
  /* As wide as the page, as the Library. */
  main {
    padding: 6px clamp(12px, 2vw, 32px) 48px;
  }

  .empty {
    margin-top: 32px;
    color: var(--muted);
    font-size: 0.9rem;
  }

  .note {
    display: block;
    margin-top: 3px;
    color: var(--muted);
    font-size: 0.8rem;
  }

  .note.failed {
    color: var(--danger);
  }
</style>
