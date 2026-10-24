<script lang="ts">
  // Downloads (decision 0033, step 4; 0038 amended 2026-10-24: Finamp's page in Cratarr's look, the owner's "make it
  // so"): what is on this device and why. At the top what it comes to (tracks, size against the budget, the quality),
  // what waits and what failed (Retry), and the gear to Settings → Sync. Then two groups, side by side where the page
  // is wide enough: Synced, each item that follows its playlist, crate, album or artist (Sync to this device), its row
  // filling as it syncs; and Downloaded once, the albums of the one-time copies. A row's ⋮ is its menu; an album's row
  // plays what is on the device, so the page plays without a connection. The pill's button syncs now.
  import { albumCoverUrl, crateImageUrl, type ListedAlbum } from './lib/api';
  import AlbumMenu from './lib/AlbumMenu.svelte';
  import Cover from './lib/Cover.svelte';
  import Icon from './lib/Icon.svelte';
  import MenuCards from './lib/MenuCards.svelte';
  import MenuEntry from './lib/MenuEntry.svelte';
  import MenuGroup from './lib/MenuGroup.svelte';
  import MenuSheet from './lib/MenuSheet.svelte';
  import PageLoading from './lib/PageLoading.svelte';
  import Group from './lib/settings/Group.svelte';
  import { downloadedSize } from './lib/settings/editor.svelte';
  import { appInfo, askDevice, askDownloads, download, downloads, inApp, type DownloadedTrack } from './lib/app.svelte';
  import { albumOfDownload, goToArtist, library, readLibrary } from './lib/albums.svelte';
  import { askKept, kept, keptBusy, setKept, syncKept, type KeptItem } from './lib/kept.svelte';
  import { playQueue } from './lib/localPlayer.svelte';
  import { albumSubject, openStep, SETTINGS, viewOver } from './lib/panel.svelte';
  import { usePageAction } from './lib/pageView.svelte';

  askDownloads();
  askKept();
  askDevice();
  // The Library's list, for what each album is (the kept copy first; without a connection that is all there is).
  if (!library.view) readLibrary(false).catch(() => {});

  // --- What it comes to ---
  const tracks = $derived(downloads.tracks);
  const waiting = $derived(tracks.filter((t) => t.download.state === 'waiting' || t.download.state === 'downloading').length);
  const failed = $derived(tracks.filter((t) => t.download.state === 'failed'));
  const used = $derived(kept.known ? kept.used : downloads.bytes);
  const quality = $derived(appInfo.downloadQuality === 'smaller' ? 'Smaller copies' : 'Originals');
  const WAITS = { wifi: 'Waiting for Wi-Fi', charging: 'Waiting to charge', network: 'Waiting for a connection' } as const;

  // Failed: a synced item's tracks are tried again by a sync; the one-time copies by asking for them again.
  function retry() {
    syncKept();
    const own = failed.filter((t) => t.manual !== false);
    if (own.length) download(own);
  }

  // --- Synced ---
  const KINDS = { album: 'Album', playlist: 'Playlist', crate: 'Crate', artist: 'Artist' } as const;
  function line(i: KeptItem): string {
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
  const fill = (i: KeptItem) => {
    const want = i.total - i.over_budget;
    return i.state === 'done' || i.state === 'error' || want <= 0 ? null : Math.min(1, i.done / want);
  };
  // Its picture: an album's cover, a crate's image; a playlist's and an artist's (kept by name) the record mark.
  const picture = (i: KeptItem) => (i.kind === 'album' ? albumCoverUrl(i.source, i.id) : i.kind === 'crate' ? crateImageUrl(i.id) : null);
  function openItem(i: KeptItem) {
    if (i.kind === 'album') viewOver(albumSubject(i.source, i.id));
    else if (i.kind === 'artist') goToArtist(i.source, i.id);
    else if (i.kind === 'crate') viewOver(`crate:${i.id}`);
    else viewOver(i.id);
  }
  let itemMenu = $state<KeptItem | null>(null);

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
  const listedOf = (g: Album) => library.view?.sources.find((s) => s.id === g.source)?.albums.find((a) => a.id === g.id) ?? null;
  const asListed = (g: Album): ListedAlbum => {
    const t = g.tracks[0];
    return {
      ...(listedOf(g) ?? { id: g.id, title: t.album ?? '', artist: t.artist, year: null, added: null, path: null, format: null, bitrate_kbps: null, tags: [] }),
      tracks: g.tracks.length,
    };
  };
  const whole = (g: Album) => g.tracks.filter((t) => t.download.state === 'done');
  function albumLine(g: Album): { text: string; fill: number | null; bad: boolean } {
    const a = asListed(g);
    const done = whole(g).length;
    const bad = g.tracks.filter((t) => t.download.state === 'failed').length;
    if (done + bad < g.tracks.length) {
      const going = g.tracks.reduce((n, t) => n + (t.download.state === 'downloading' ? t.download.percent / 100 : 0), 0);
      return { text: `Downloading ${done} of ${g.tracks.length}`, fill: (done + going) / g.tracks.length, bad: false };
    }
    if (bad) return { text: `${bad} ${bad === 1 ? 'track' : 'tracks'} failed`, fill: null, bad: true };
    const of = listedOf(g)?.tracks;
    const parts = [a.artist, a.year ? String(a.year) : null, a.format, of && of > g.tracks.length ? `${g.tracks.length} of ${of}` : null];
    return { text: parts.filter(Boolean).join(' · '), fill: null, bad: false };
  }
  function play(g: Album) {
    const t = whole(g);
    if (t.length) playQueue(t, 0, asListed(g).title);
  }
  let albumMenu = $state<{ source: string; a: ListedAlbum } | null>(null);

  // The pill's button: sync now, ringed while anything syncs or waits.
  usePageAction(() => {
    if (!inApp || !kept.items.length) return null;
    const want = kept.items.reduce((n, i) => n + i.total - i.over_budget, 0);
    const done = kept.items.reduce((n, i) => n + i.done, 0);
    return { icon: 'refresh', label: 'Sync now', run: syncKept, progress: keptBusy() ? (want ? done / want : 0) : null };
  });
</script>

<main>
  {#if !downloads.known}
    <PageLoading />
  {:else}
    <section class="summary">
      <div class="count">
        <strong>{tracks.length.toLocaleString()} {tracks.length === 1 ? 'track' : 'tracks'}</strong>
        <span>{kept.budget ? `${downloadedSize(used)} of ${downloadedSize(kept.budget)}` : downloadedSize(used)} · {quality}</span>
      </div>
      {#if kept.budget}<span class="budget" aria-hidden="true"><span style:width="{Math.min(1, used / kept.budget) * 100}%"></span></span>{/if}
      <div class="state">
        {#if waiting}<span class="wait">{kept.waiting ? WAITS[kept.waiting] : `${waiting} waiting`}</span>{/if}
        {#if failed.length}<button class="retry" onclick={retry}>{failed.length} failed · Retry</button>{/if}
      </div>
      <button class="gear" aria-label="Sync settings" title="Sync settings" onclick={() => openStep(`${SETTINGS}sync`)}><Icon name="settings" size={22} /></button>
    </section>

    {#if !kept.items.length && !once.length}
      <p class="empty">Nothing downloaded.</p>
    {:else}
      <div class="groups">
        {#if kept.items.length}
          <Group title="Synced">
            {#each kept.items as i (i.key)}
              {@const f = fill(i)}
              <div class="row" class:filling={f !== null} style:--fill={f === null ? null : `${f * 100}%`}>
                <button class="open" onclick={() => openItem(i)}>
                  <span class="pic"><Cover src={picture(i)} /></span>
                  <span class="text"><span class="name">{i.name}</span><small class:bad={i.state === 'error' || i.failed > 0}>{line(i)}</small></span>
                </button>
                <button class="more" aria-label={`More for ${i.name}`} onclick={() => (itemMenu = i)}><Icon name="moreVert" size={22} /></button>
              </div>
            {/each}
          </Group>
        {/if}
        {#if once.length}
          <Group title="Downloaded once">
            {#each once as g (`${g.source}/${g.id}`)}
              {@const a = asListed(g)}
              {@const l = albumLine(g)}
              <div class="row" class:filling={l.fill !== null} style:--fill={l.fill === null ? null : `${l.fill * 100}%`}>
                <button class="open" disabled={!whole(g).length} onclick={() => play(g)}>
                  <span class="pic"><Cover src={albumCoverUrl(g.source, g.id)} /></span>
                  <span class="text"><span class="name">{a.title}</span><small class:bad={l.bad}>{l.text}</small></span>
                </button>
                <button class="more" aria-label={`More for ${a.title}`} onclick={() => (albumMenu = { source: g.source, a })}><Icon name="moreVert" size={22} /></button>
              </div>
            {/each}
          </Group>
        {/if}
      </div>
    {/if}
  {/if}
</main>

{#if itemMenu}
  {@const i = itemMenu}
  <MenuSheet label={i.name} title={i.name} images={picture(i) ? [picture(i)!] : null} icon="download" chips={[KINDS[i.kind]]} onclose={() => (itemMenu = null)}>
    <MenuCards>
      <MenuGroup>
        <MenuEntry icon="squareArrowOutUpRight" label="View" onclick={() => ((itemMenu = null), openItem(i))} />
        <MenuEntry icon="refresh" label="Sync now" onclick={() => ((itemMenu = null), syncKept())} />
      </MenuGroup>
      <MenuGroup>
        <MenuEntry icon="close" label="Stop syncing" danger onclick={() => ((itemMenu = null), setKept(i.kind, i.source, i.id, false))} />
      </MenuGroup>
    </MenuCards>
  </MenuSheet>
{/if}
{#if albumMenu}
  {@const m = albumMenu}
  <AlbumMenu source={m.source} album={m.a} onclose={() => (albumMenu = null)} />
{/if}

<style>
  /* As wide as the page, as the Library. */
  main {
    container-type: inline-size;
    padding: 6px clamp(12px, 2vw, 32px) 48px;
  }

  .summary {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 6px 16px;
    padding: 16px 12px 16px 20px;
    border-radius: 16px;
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }

  .count {
    display: grid;
    gap: 2px;
  }

  .count strong {
    font-size: 26px;
    font-weight: 600;
  }

  .count span {
    color: var(--muted);
    font-size: 14px;
  }

  .gear {
    grid-row: 1;
    grid-column: 2;
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--accent);
    cursor: pointer;
  }

  .gear:hover {
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }

  .budget {
    grid-column: 1 / -1;
    display: block;
    height: 4px;
    margin-right: 8px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    overflow: hidden;
  }

  .budget span {
    display: block;
    height: 100%;
    background: var(--accent);
  }

  .state {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    font-size: 14px;
  }

  .state:empty {
    display: none;
  }

  .wait {
    color: var(--warn-ink);
  }

  .retry {
    padding: 0;
    border: 0;
    background: none;
    color: var(--danger-ink);
    font: inherit;
    cursor: pointer;
  }

  /* Wide: the count, the bar and the state in one line. */
  @container (min-width: 760px) {
    .summary {
      grid-template-columns: auto 1fr auto auto;
    }

    .budget {
      grid-row: 1;
      grid-column: 2;
      margin: 0 16px;
    }

    .state {
      grid-row: 1;
      grid-column: 3;
      flex-direction: column;
      align-items: flex-end;
    }

    .gear {
      grid-column: 4;
    }
  }

  .empty {
    margin-top: 32px;
    color: var(--muted);
    font-size: 0.9rem;
  }

  .groups {
    display: grid;
    align-items: start;
    gap: 24px;
    margin-top: 24px;
  }

  @container (min-width: 760px) {
    .groups {
      grid-template-columns: 1fr 1fr;
    }
  }

  /* The groups side by side: Group's own spacing between them goes. */
  .groups > :global(.group + .group) {
    margin-top: 0;
  }

  .row {
    display: flex;
    align-items: center;
  }

  /* Filled as far as it has come (as Download once's row). */
  .row.filling {
    background: linear-gradient(
      to right,
      color-mix(in srgb, var(--accent) 16%, transparent) var(--fill),
      transparent var(--fill)
    );
  }

  .open {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 14px;
    min-width: 0;
    padding: 10px 0 10px 14px;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .open:disabled {
    cursor: default;
  }

  .pic {
    flex: none;
    width: 48px;
    height: 48px;
    overflow: hidden;
    border-radius: 6px;
  }

  .text {
    display: grid;
    min-width: 0;
  }

  .name,
  small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name {
    font-size: 16px;
  }

  small {
    color: var(--muted);
    font-size: 13px;
  }

  small.bad {
    color: var(--danger-ink);
  }

  .more {
    display: grid;
    flex: none;
    place-items: center;
    width: 48px;
    height: 48px;
    margin-right: 4px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .more:hover {
    background: color-mix(in srgb, var(--text) 7%, transparent);
    color: var(--text);
  }
</style>
