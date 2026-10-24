<script lang="ts">
  // Downloads (decision 0033, step 4; 0038 amended 2026-10-24): what is on this device and why, laid out as Insights,
  // Health and Settings (the owner, 2026-10-24): rows in groups, each its picture or icon, its name, a line, and ›.
  // This device: the tracks and their size against the budget (› Settings → Sync), what waits, what failed (Retry).
  // Synced: each item that follows its playlist, crate, album or artist (Sync to this device), its row filling as it
  // syncs. Downloaded once: the albums of the one-time copies, filling as they come. A tap opens the item in the panel:
  // beside the list on a roomy window, where one is always open (the first, until another is), its row tinted; over the
  // list on a phone. The pill: its search narrows Synced and Downloaded once (no filter or View, the owner: not
  // applicable), and its button syncs now.
  import { albumCoverUrl, crateImageUrl } from './lib/api';
  import Cover from './lib/Cover.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import { downloadedSize } from './lib/settings/editor.svelte';
  import { appInfo, askDevice, askDownloads, download, downloads, inApp, type DownloadedTrack } from './lib/app.svelte';
  import { albumOfDownload, library, readLibrary } from './lib/albums.svelte';
  import { askKept, kept, keptBusy, syncKept, type KeptItem } from './lib/kept.svelte';
  import { player } from './lib/live.svelte';
  import { albumSubject, artistSubject, openedHere, panel, SETTINGS, viewInPlace, viewPlaylist } from './lib/panel.svelte';
  import { matchesSearch, pageView, usePageAction } from './lib/pageView.svelte';

  askDownloads();
  askKept();
  askDevice();
  // The Library's list, for what each album is (the kept copy first; without a connection that is all there is).
  if (!library.view) readLibrary(false).catch(() => {});

  // --- This device ---
  const tracks = $derived(downloads.tracks);
  const waiting = $derived(tracks.filter((t) => t.download.state === 'waiting' || t.download.state === 'downloading').length);
  const failed = $derived(tracks.filter((t) => t.download.state === 'failed'));
  const used = $derived(kept.known ? kept.used : downloads.bytes);
  const quality = $derived(appInfo.downloadQuality === 'smaller' ? 'Smaller copies' : 'Originals');
  const WAITS = { wifi: 'Waiting for Wi-Fi', charging: 'Waiting to charge', network: 'Waiting for a connection' } as const;
  const SYNC_SETTINGS = `${SETTINGS}sync`;

  // Failed: a synced item's tracks are tried again by a sync; the one-time copies by asking for them again.
  function retry() {
    syncKept();
    const own = failed.filter((t) => t.manual !== false);
    if (own.length) download(own);
  }

  // The pill's search: an item or an album whose name (or artist) holds every word.
  const matches = matchesSearch;

  // --- Synced ---
  const KINDS = { album: 'Album', playlist: 'Playlist', crate: 'Crate', artist: 'Artist' } as const;
  function keptLine(i: KeptItem): string {
    if (i.state === 'error') return i.error ?? "Can't be read";
    const want = i.total - i.over_budget;
    const parts: string[] = [KINDS[i.kind]];
    if (i.state === 'waiting' && kept.waiting) parts.push(WAITS[kept.waiting]);
    else parts.push(i.state === 'done' ? String(i.done) : `${i.done} of ${want}`);
    if (i.over_budget) parts.push(`${i.over_budget} over budget`);
    if (i.failed) parts.push(`${i.failed} failed`);
    if (i.bytes) parts.push(downloadedSize(i.bytes));
    return parts.join(' · ');
  }
  const keptFill = (i: KeptItem) => {
    const want = i.total - i.over_budget;
    return i.state === 'done' || i.state === 'error' || want <= 0 ? null : Math.min(1, i.done / want);
  };
  // Its picture: an album's cover, a crate's image; a playlist's and an artist's (kept by name) the record mark.
  const keptPicture = (i: KeptItem) => (i.kind === 'album' ? albumCoverUrl(i.source, i.id) : i.kind === 'crate' ? crateImageUrl(i.id) : null);
  const synced = $derived(kept.items.filter((i) => matches(i.name, KINDS[i.kind])));
  const keptSubject = (i: KeptItem) =>
    i.kind === 'album' ? albumSubject(i.source, i.id) : i.kind === 'artist' ? artistSubject(i.source, i.id) : i.kind === 'crate' ? `crate:${i.id}` : i.id;

  // --- Downloaded once: the one-time copies by album (one compilation is one album, whatever its tracks' artists) ---
  type Album = { source: string; id: string; tracks: DownloadedTrack[] };
  const once = $derived.by(() => {
    const by = new Map<string, Album>();
    for (const t of tracks) {
      if (t.manual === false) continue;
      const id = albumOfDownload(t) ?? `${t.album ?? ''}\n${t.artist ?? ''}`;
      const key = `${t.source}/${id}`;
      const g = by.get(key) ?? { source: t.source, id, tracks: [] };
      g.tracks.push(t);
      by.set(key, g);
    }
    return [...by.values()].reverse();
  });
  const shownOnce = $derived(once.filter((g) => matches(titleOf(g), listedOf(g)?.artist ?? g.tracks[0].artist)));
  const listedOf = (g: Album) => library.view?.sources.find((s) => s.id === g.source)?.albums.find((a) => a.id === g.id) ?? null;
  const titleOf = (g: Album) => listedOf(g)?.title ?? g.tracks[0].album ?? '';
  function albumLine(g: Album): { text: string; fill: number | null; bad: boolean } {
    const listed = listedOf(g);
    const done = g.tracks.filter((t) => t.download.state === 'done').length;
    const bad = g.tracks.filter((t) => t.download.state === 'failed').length;
    if (done + bad < g.tracks.length) {
      const going = g.tracks.reduce((n, t) => n + (t.download.state === 'downloading' ? t.download.percent / 100 : 0), 0);
      return { text: `Downloading ${done} of ${g.tracks.length}`, fill: (done + going) / g.tracks.length, bad: false };
    }
    if (bad) return { text: `${bad} ${bad === 1 ? 'track' : 'tracks'} failed`, fill: null, bad: true };
    const of = listed?.tracks;
    const parts = [listed?.artist ?? g.tracks[0].artist, listed?.year ? String(listed.year) : null, listed?.format, of && of > g.tracks.length ? `${g.tracks.length} of ${of}` : null];
    return { text: parts.filter(Boolean).join(' · '), fill: null, bad: false };
  }

  // Beside the list one item is always open on a roomy window: the first, until another is, as Settings, Health and
  // Insights fill theirs (openedHere); not while leaving the page, whose address the opened item would be written into.
  const first = $derived(kept.items[0] ? keptSubject(kept.items[0]) : once[0] ? albumSubject(once[0].source, once[0].id) : null);
  $effect(() => {
    if (!player.roomy || openedHere() || panel.viewed === first || !first || !location.hash.startsWith('#/downloads')) return;
    viewInPlace(first);
  });

  // The pill's button: sync now, ringed while anything syncs or waits.
  usePageAction(() => {
    if (!inApp || !kept.items.length) return null;
    const want = kept.items.reduce((n, i) => n + i.total - i.over_budget, 0);
    const done = kept.items.reduce((n, i) => n + i.done, 0);
    const busy = keptBusy();
    return { icon: 'refresh', label: 'Sync now', run: syncKept, progress: busy ? (want ? done / want : 0) : null, line: busy ? `Syncing · ${done} of ${want}` : null };
  });
</script>

{#snippet row(subject: string, name: string, line: string, picture: { src: string | null } | { icon: IconName }, fill: number | null = null, bad = false)}
  {@const open = panel.viewed === subject}
  <button class="row" class:open class:filling={fill !== null} style:--fill={fill === null ? null : `${fill * 100}%`} aria-pressed={open} onclick={() => viewPlaylist(subject)}>
    {#if 'icon' in picture}<Icon name={picture.icon} size={24} />{:else}<span class="pic"><Cover src={picture.src} /></span>{/if}
    <span class="text">
      <span class="name">{name}</span>
      {#if line}<small class:bad>{line}</small>{/if}
    </span>
    <span class="chev"><Icon name="chevronRight" size={20} /></span>
  </button>
{/snippet}

<main>
  {#if !downloads.known}
    <PageLoading />
  {:else}
    <section class="group" aria-labelledby="downloads-device">
      <h2 id="downloads-device">This device</h2>
      <div class="list">
        {@render row(
          SYNC_SETTINGS,
          `${tracks.length.toLocaleString()} ${tracks.length === 1 ? 'track' : 'tracks'}`,
          `${kept.budget ? `${downloadedSize(used)} of ${downloadedSize(kept.budget)}` : downloadedSize(used)} · ${quality}`,
          { icon: 'download' },
          kept.budget ? Math.min(1, used / kept.budget) : null,
        )}
        {#if waiting}
          <div class="row still">
            <Icon name="arrowDownToLine" size={24} />
            <span class="text"><span class="name">{kept.waiting ? WAITS[kept.waiting] : `${waiting} waiting`}</span></span>
          </div>
        {/if}
        {#if failed.length}
          <div class="row still">
            <span class="bad-icon"><Icon name="ban" size={24} /></span>
            <span class="text"><span class="name">{failed.length} failed</span></span>
            <button class="retry" onclick={retry}>Retry</button>
          </div>
        {/if}
      </div>
    </section>

    {#if synced.length}
      <section class="group" aria-labelledby="downloads-synced">
        <h2 id="downloads-synced">Synced</h2>
        <div class="list">
          {#each synced as i (i.key)}
            {@render row(keptSubject(i), i.name, keptLine(i), { src: keptPicture(i) }, keptFill(i), i.state === 'error' || i.failed > 0)}
          {/each}
        </div>
      </section>
    {/if}

    {#if shownOnce.length}
      <section class="group" aria-labelledby="downloads-once">
        <h2 id="downloads-once">Downloaded once</h2>
        <div class="list">
          {#each shownOnce as g (`${g.source}/${g.id}`)}
            {@const l = albumLine(g)}
            {@render row(albumSubject(g.source, g.id), titleOf(g), l.text, { src: albumCoverUrl(g.source, g.id) }, l.fill, l.bad)}
          {/each}
        </div>
      </section>
    {/if}
  {/if}
</main>

<style>
  /* As Insights' and Settings' list: one column; beside an item it is the list's own column (App). */
  main {
    padding: 16px clamp(12px, 2vw, 32px) 96px;
  }

  .group {
    max-width: 640px;
  }

  .group + .group {
    margin-top: 24px;
  }

  h2 {
    margin: 0 4px 8px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
  }

  .list {
    overflow: hidden;
    border-radius: 14px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    min-height: 64px;
    padding: 10px 14px 10px 16px;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row + .row {
    border-top: 1px solid var(--border);
  }

  .row:not(.still):hover,
  .row:not(.still):focus-visible {
    background: color-mix(in srgb, var(--text) 5%, transparent);
  }

  .row.still {
    cursor: default;
  }

  /* Filled as far as it has come (as Download once's row), the budget used on the device's row. */
  .row.filling {
    background: linear-gradient(to right, color-mix(in srgb, var(--accent) 14%, transparent) var(--fill), transparent var(--fill));
  }

  /* The item open beside the list. */
  .row.open {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .row > :global(svg) {
    flex: none;
    color: var(--accent);
  }

  .bad-icon {
    display: flex;
    flex: none;
    color: var(--danger);
  }

  .pic {
    flex: none;
    width: 40px;
    height: 40px;
    overflow: hidden;
    border-radius: 6px;
  }

  .text {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    font-size: 15px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .text small {
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .text small.bad {
    color: var(--danger-ink);
  }

  .chev {
    display: grid;
    flex: none;
    place-items: center;
    color: var(--muted);
  }

  .retry {
    flex: none;
    padding: 8px 14px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--danger) 16%, transparent);
    color: var(--danger-ink);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
</style>
