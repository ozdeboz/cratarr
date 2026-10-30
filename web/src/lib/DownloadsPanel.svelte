<script lang="ts">
  // A list of Downloads in the panel (the owner, 2026-10-24: Downloads as Requests, Insights, Health and Settings: the page
  // an index, its lists in its own panel, never the content panel): opened from the page's index, beside it on a roomy
  // window (no ✕ there), over it on a phone. This device: the tracks and their size against the budget (› Settings ›
  // Sync), what waits, what failed (Retry). Synced: each item that follows its playlist, crate, album or artist, filling
  // as it syncs. Downloaded once: the albums of the one-time copies, filling as they come. A release is a release row
  // (lib/ReleaseRow); a tap opens its menu (the owner, 2026-10-24: not the item itself): an album's own, a playlist's or
  // a crate's own; an artist (or what is not known well enough for its own menu) a short one: Go to it, Stop syncing.
  // The pill's search narrows the two lists. Select: the ticked ones played, queued or removed from this device (the
  // owner, 2026-10-27: what one has, several have).
  import { glass, handGlass } from './glass.svelte';
  import GlobalResults from './GlobalResults.svelte';
  import { everywhere } from './globalSearch.svelte';
  import { albumCoverUrl, crateImageUrl } from './api';
  import { albumSubject, artistSubject, closePlaylist, SETTINGS, upToPage, viewPlaylist } from './panel.svelte';
  import Crumbs from './Crumbs.svelte';
  import { appInfo, download, downloads, removeDownloads } from './app.svelte';
  import type { Collection } from './api';
  import { plural } from './insights.svelte';
  import { playCollection } from './play.svelte';
  import { toast } from './toast.svelte';
  import { library } from './albums.svelte';
  import { DOWNLOADS_LISTS, goTo, onceAlbums, WAITS, type DownloadsList, type OnceAlbum } from './downloadsLists.svelte';
  import Icon from './Icon.svelte';
  import { kept, setKept, syncKept, type KeptItem } from './kept.svelte';
  import { matchesSearch } from './pageView.svelte';
  import PageLoading from './PageLoading.svelte';
  import ReleaseRow from './ReleaseRow.svelte';
  import SelectBar from './SelectBar.svelte';
  import SelectSheet from './SelectSheet.svelte';
  import TopPill from './TopPill.svelte';
  import AlbumMenu from './AlbumMenu.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuSheet from './MenuSheet.svelte';
  import SubjectMenu from './SubjectMenu.svelte';
  import { shownPlaylists } from './output.svelte';
  import Group from './settings/Group.svelte';
  import Item from './settings/Item.svelte';
  import { downloadedSize } from './settings/editor.svelte';
  import { reach } from './reach.svelte';
  import { shelf, shelfCover } from './shelfStore.svelte';

  let { list, beside = false }: { list: DownloadsList; beside?: boolean } = $props();
  const L = $derived(DOWNLOADS_LISTS.find((x) => x.id === list) ?? DOWNLOADS_LISTS[0]);

  // --- This device ---
  const tracks = $derived(downloads.tracks);
  const waiting = $derived(tracks.filter((t) => t.download.state === 'waiting' || t.download.state === 'downloading').length);
  const failed = $derived(tracks.filter((t) => t.download.state === 'failed'));
  const used = $derived(kept.known ? kept.used : downloads.bytes);
  const quality = $derived(appInfo.downloadQuality === 'smaller' ? 'Smaller copies' : 'Originals');
  // Failed: a synced item's tracks are tried again by a sync; the one-time copies by asking for them again.
  function retry() {
    syncKept();
    const own = failed.filter((t) => t.manual !== false);
    if (own.length) download(own);
  }

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
  // Its picture: an album's cover, a crate's image; a playlist's and an artist's (kept by name) the stand-in.
  // An album's the shelf's small copy first (decision 0085, step 2: Downloads with no network).
  const keptPicture = (i: KeptItem) =>
    i.kind === 'album' ? (shelfCover(i.source, i.id) ?? albumCoverUrl(i.source, i.id)) : i.kind === 'crate' ? crateImageUrl(i.id) : null;
  const keptSubject = (i: KeptItem) =>
    i.kind === 'album' ? albumSubject(i.source, i.id) : i.kind === 'artist' ? artistSubject(i.source, i.id) : i.kind === 'crate' ? `crate:${i.id}` : i.id;
  const synced = $derived(kept.items.filter((i) => matchesSearch(i.name, KINDS[i.kind])));

  // --- A row's menu ---
  /** `album`: a one-time copy's album, its tracks here (for the menu offline: play them, take them off). */
  type Menu = { kind: KeptItem['kind']; source: string; id: string; subject: string; name: string; artist: string | null; cover: string | null; synced: boolean; album?: OnceAlbum };
  let menu = $state<Menu | null>(null);
  const keptMenu = (i: KeptItem): Menu => ({
    kind: i.kind,
    source: i.source,
    id: i.id,
    subject: keptSubject(i),
    name: i.name,
    artist: i.kind === 'album' ? (library.view?.sources.find((s) => s.id === i.source)?.albums.find((a) => a.id === i.id)?.artist ?? null) : null,
    cover: keptPicture(i),
    synced: true,
  });
  /** Its own menu where one can be made from what is known: an album's (a real id), a playlist's (one listed), a crate's. */
  const ownMenu = (m: Menu) =>
    (m.kind === 'album' && !m.id.includes('\n')) || m.kind === 'crate' || (m.kind === 'playlist' && shownPlaylists().some((p) => p.id === m.id));
  const GO_ICON = { album: 'disc', artist: 'user', playlist: 'playlists', crate: 'crates' } as const;

  // --- Downloaded once ---
  const listedOf = (g: OnceAlbum) => library.view?.sources.find((s) => s.id === g.source)?.albums.find((a) => a.id === g.id) ?? null;
  /** What the shelf keeps of an album's tracks here (lib/shelfStore): with no network, all there is. */
  const shelvedOf = (g: OnceAlbum) => g.tracks.map((t) => shelf.tracks[`app:${t.source}/${t.id}`]).filter((r) => !!r);
  const titleOf = (g: OnceAlbum) => listedOf(g)?.title ?? g.tracks[0].album ?? '';
  const artistOf = (g: OnceAlbum) => listedOf(g)?.artist ?? shelvedOf(g).find((r) => r.album_artist)?.album_artist ?? g.tracks[0].artist;
  const yearOf = (g: OnceAlbum) => listedOf(g)?.year ?? shelvedOf(g).find((r) => r.year)?.year ?? null;
  const onceCover = (g: OnceAlbum) => shelfCover(g.source, g.id) ?? albumCoverUrl(g.source, g.id);
  // The pill's search reads the album and, from the shelf, its tracks, genres and year (so it works with no network).
  const once = $derived(
    onceAlbums().filter((g) => {
      const s = shelvedOf(g);
      return matchesSearch(titleOf(g), artistOf(g), yearOf(g) ? String(yearOf(g)) : null, [...new Set(s.flatMap((r) => r.genres))].join(' '), s.map((r) => r.name).join(' '));
    }),
  );
  function albumLine(g: OnceAlbum): { text: string; fill: number | null; bad: boolean } {
    const listed = listedOf(g);
    const done = g.tracks.filter((t) => t.download.state === 'done').length;
    const bad = g.tracks.filter((t) => t.download.state === 'failed').length;
    if (done + bad < g.tracks.length) {
      const going = g.tracks.reduce((n, t) => n + (t.download.state === 'downloading' ? t.download.percent / 100 : 0), 0);
      return { text: `Downloading ${done} of ${g.tracks.length}`, fill: (done + going) / g.tracks.length, bad: false };
    }
    if (bad) return { text: `${bad} ${bad === 1 ? 'track' : 'tracks'} failed`, fill: null, bad: true };
    const of = listed?.tracks;
    const year = yearOf(g);
    const parts = [artistOf(g), year ? String(year) : null, listed?.format, of && of > g.tracks.length ? `${g.tracks.length} of ${of}` : null];
    return { text: parts.filter(Boolean).join(' · '), fill: null, bad: false };
  }
  // On a phone this panel's glass is App's (lib/glass, the owner 2026-10-25: one glass, page's and panel's): handed over,
  // its own bar not drawn, its list under the glass.
  // --- Select (the owner, 2026-10-26: no way to remove downloads in sight): many at once, as Requests' lists ---
  let selecting = $state<Set<string> | null>(null);
  let selectSheet = $state(false);
  const onceKey = (g: OnceAlbum) => `${g.source}/${g.id}`;
  const shownKeys = $derived(list === 'synced' ? synced.map((i) => i.key) : list === 'once' ? once.map(onceKey) : []);
  const ticked = $derived(selecting ? shownKeys.filter((k) => selecting!.has(k)) : []);
  function tick(k: string) {
    if (!selecting) return;
    const s = new Set(selecting);
    if (s.has(k)) s.delete(k);
    else s.add(k);
    selecting = s;
  }
  function selectWhich(which: 'all' | 'none' | 'others') {
    selecting = new Set(which === 'all' ? shownKeys : which === 'none' ? [] : shownKeys.filter((k) => !selecting?.has(k)));
  }
  // Another list: Select ends.
  $effect(() => {
    void list;
    selecting = null;
  });
  /** The ticked ones off this device: synced ones stop syncing (which takes their files), once ones are removed. */
  function removeTicked() {
    const on = new Set(ticked);
    if (list === 'synced') for (const i of synced) if (on.has(i.key)) setKept(i.kind, i.source, i.id, false, i.name);
    if (list === 'once') removeDownloads(once.filter((g) => on.has(onceKey(g))).flatMap((g) => g.tracks.map((t) => t.download.id)));
    selecting = null;
  }
  /** The ticked ones played or added to the queue, one after another: a synced item as its own Play plays it, a
   *  one-time copy's album its downloaded tracks. */
  async function playTicked(how: 'play' | 'last') {
    const on = new Set(ticked);
    const whats: { what: Collection; name: string }[] = [];
    if (list === 'synced')
      for (const i of synced.filter((x) => on.has(x.key)))
        whats.push({
          what: i.kind === 'album' ? { kind: 'album', source: i.source, id: i.id } : i.kind === 'artist' ? { kind: 'artist', source: i.source, name: i.id } : { kind: i.kind, id: i.id },
          name: i.name,
        });
    if (list === 'once')
      for (const g of once.filter((x) => on.has(onceKey(x)))) whats.push({ what: { kind: 'tracks', source: g.source, ids: g.tracks.map((t) => t.id) }, name: titleOf(g) });
    if (!whats.length) return;
    navigator.vibrate?.(8);
    selecting = null;
    const name = whats.length === 1 ? whats[0].name : plural(whats.length, list === 'once' ? 'album' : 'item');
    try {
      for (const [n, w] of whats.entries()) await playCollection(w.what, n ? 'last' : how, name);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }

  /** A row's menu out of reach: its album's tracks here, or the synced item, played or queued from this device. */
  async function playMenu(m: Menu, how: 'play' | 'last') {
    menu = null;
    const what: Collection = m.album
      ? { kind: 'tracks', source: m.source, ids: m.album.tracks.map((t) => t.id) }
      : m.kind === 'album'
        ? { kind: 'album', source: m.source, id: m.id }
        : m.kind === 'artist'
          ? { kind: 'artist', source: m.source, name: m.id }
          : { kind: m.kind, id: m.id };
    try {
      await playCollection(what, how, m.name);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }

  const handed = $derived(glass.one && !beside);
  const pill = () => (!beside ? { device: false, filter: false, view: false, search: 'downloads' } : null);
  handGlass(() => handed, { left: glassLeft, pill, extra: glassExtra });
</script>

<!-- This panel's glass: its own bar's parts, or App's on a phone (lib/glass). -->
{#snippet glassLeft()}
  {#if !beside}<span class="navpill"><button aria-label="Back" title="Back" onclick={() => closePlaylist()}><Icon name="back" size={22} /></button></span>{/if}
  <Crumbs trail={beside ? [] : [{ label: 'Downloads', go: () => upToPage('#/downloads') }]} here={L.label} />
{/snippet}

{#snippet glassExtra()}
  {#if shownKeys.length}
    <span class="navpill"
      ><button aria-label="Select" title="Select" aria-pressed={!!selecting} onclick={() => (selecting = selecting ? null : new Set())}><Icon name="listChecks" size={22} /></button></span
    >
  {/if}
{/snippet}

{#snippet tickOf(k: string, name: string)}
  {@const on = !!selecting?.has(k)}
  <button class="tick" class:on role="checkbox" aria-checked={on} aria-label={`Select ${name}`} onclick={() => tick(k)}
    ><Icon name={on ? 'circleCheck' : 'circle'} size={24} /></button
  >
{/snippet}

<div class="view" class:handed>
  {#if !handed}
    <div class="bar" class:beside>
      {@render glassLeft()}
      {#if pill()}<TopPill {...pill()!} />{/if}
      {@render glassExtra()}
    </div>
  {/if}
  <div class="scroll" class:away={handed && everywhere()}>
    {#if !downloads.known}
      <PageLoading />
    {:else if list === 'device'}
      <Group>
        <Item
          name={`${tracks.length.toLocaleString()} ${tracks.length === 1 ? 'track' : 'tracks'}`}
          line={`${kept.budget ? `${downloadedSize(used)} of ${downloadedSize(kept.budget)}` : downloadedSize(used)} · ${quality}`}
          progress={kept.budget ? Math.min(1, used / kept.budget) : null}
        />
        <!-- Where its room and quality are set: a row of its own, saying so (the owner, 2026-10-26: the bar's row did not
             make clear it went there). -->
        <Item name="Sync settings" onclick={() => viewPlaylist(`${SETTINGS}sync`)} />
        {#if waiting}<Item lead="warn" name={kept.waiting ? WAITS[kept.waiting] : `${waiting} waiting`} />{/if}
        {#if failed.length}
          <!-- Retry asks Cratarr: not offered out of its reach (decision 0085: never offer what cannot be upheld). -->
          <Item lead="bad" name={`${failed.length} failed`}>{#if !reach.away}<button class="retry" onclick={retry}>Retry</button>{/if}</Item>
        {/if}
      </Group>
    {:else if list === 'synced'}
      <Group>
        {#each synced as i (i.key)}
          {#if selecting}
            <ReleaseRow cover={keptPicture(i)} title={i.name} line={keptLine(i)} tone={i.state === 'error' || i.failed > 0 ? 'bad' : null}
              >{@render tickOf(i.key, i.name)}</ReleaseRow
            >
          {:else}
            <ReleaseRow
              cover={keptPicture(i)}
              title={i.name}
              line={keptLine(i)}
              tone={i.state === 'error' || i.failed > 0 ? 'bad' : null}
              fill={keptFill(i)}
              onclick={() => (menu = keptMenu(i))}
            />
          {/if}
        {:else}
          <Item name="Nothing synced" line="Sync to this device from an album's, a playlist's, a crate's or an artist's menu." />
        {/each}
      </Group>
    {:else}
      <Group>
        {#each once as g (`${g.source}/${g.id}`)}
          {@const l = albumLine(g)}
          {#if selecting}
            <ReleaseRow cover={onceCover(g)} title={titleOf(g)} line={l.text} tone={l.bad ? 'bad' : null}
              >{@render tickOf(onceKey(g), titleOf(g))}</ReleaseRow
            >
          {:else}
          <ReleaseRow
            cover={onceCover(g)}
            title={titleOf(g)}
            line={l.text}
            tone={l.bad ? 'bad' : null}
            fill={l.fill}
            onclick={() =>
              (menu = {
                kind: 'album',
                source: g.source,
                id: g.id,
                subject: albumSubject(g.source, g.id),
                name: titleOf(g),
                artist: artistOf(g),
                cover: onceCover(g),
                synced: false,
                album: g,
              })}
          />
          {/if}
        {:else}
          <Item name="Nothing downloaded once" />
        {/each}
      </Group>
    {/if}
      <!-- On a phone, over the page: the global results at the end of this list (decision 0072). -->
    {#if handed && everywhere()}<GlobalResults />{/if}
  </div>
</div>

{#if selecting}
  <SelectBar
    label={list === 'synced' ? 'Selected synced items' : 'Selected albums'}
    count="{ticked.length}/{shownKeys.length}"
    onclose={() => (selecting = null)}
    every={{ all: !!shownKeys.length && ticked.length === shownKeys.length, onall: () => selectWhich('all'), onnone: () => selectWhich('none') }}
  >
    <button aria-label="Select which" title="Select…" onclick={() => (selectSheet = true)}><Icon name="listChecks" size={24} /></button>
    <button aria-label="Play" title="Play" disabled={!ticked.length} onclick={() => playTicked('play')}><Icon name="play" size={24} /></button>
    <button aria-label="Add to queue" title="Add to queue" disabled={!ticked.length} onclick={() => playTicked('last')}><Icon name="listAdd" size={24} /></button>
    <button aria-label="Remove from this device" title="Remove from this device" disabled={!ticked.length} onclick={removeTicked}><Icon name="trash" size={24} /></button>
  </SelectBar>
{/if}
{#if selectSheet}
  <SelectSheet onall={() => selectWhich('all')} onnone={() => selectWhich('none')} onothers={() => selectWhich('others')} onclose={() => (selectSheet = false)} />
{/if}

{#if menu}
  {@const m = menu}
  {#if reach.away}
    <!-- Out of Cratarr's reach (decision 0085): only what this device can do itself. -->
    <MenuSheet label={m.name} title={m.name} images={m.cover ? [m.cover] : []} onclose={() => (menu = null)}>
      <div role="menu">
        <MenuEntry icon="play" label="Play" onclick={() => playMenu(m, 'play')} />
        <MenuEntry icon="listAdd" label="Add to queue" onclick={() => playMenu(m, 'last')} />
        {#if m.album}
          <MenuEntry icon="trash" label="Remove from this device" danger onclick={() => (removeDownloads(m.album!.tracks.map((t) => t.download.id)), (menu = null))} />
        {:else if m.synced}
          <MenuEntry icon="close" label="Stop syncing" danger onclick={() => (setKept(m.kind, m.source, m.id, false, m.name), (menu = null))} />
        {/if}
      </div>
    </MenuSheet>
  {:else if ownMenu(m) && m.kind === 'album'}
    <AlbumMenu source={m.source} album={{ id: m.id, title: m.name, artist: m.artist ?? '' }} cover={m.cover} onclose={() => (menu = null)} />
  {:else if ownMenu(m)}
    <SubjectMenu subject={m.subject} onclose={() => (menu = null)} />
  {:else}
    <MenuSheet label={m.name} title={m.name} images={m.cover ? [m.cover] : []} onclose={() => (menu = null)}>
      <div role="menu">
        <MenuEntry icon={GO_ICON[m.kind]} label="Go to {m.name}" onclick={() => (goTo(m.kind, m.subject), (menu = null))} />
        {#if m.synced}<MenuEntry icon="close" label="Stop syncing" danger onclick={() => (setKept(m.kind, m.source, m.id, false, m.name), (menu = null))} />{/if}
      </div>
    </MenuSheet>
  {/if}
{/if}

<style>
  /* As Insights' and Requests' view in the panel: its bar on top, what it holds under it. */
  .view {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 65px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .bar.beside {
    padding-left: clamp(12px, 2vw, 24px);
  }

  .scroll {
    display: grid;
    flex: 1;
    align-content: start;
    gap: 24px;
    min-height: 0;
    overflow-y: auto;
    padding: 16px clamp(12px, 2vw, 24px) calc(var(--mini-h, 0px) + 16px);
  }

  .navpill {
    display: flex;
    padding: 2px;
    border-radius: 26px;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
    box-shadow: var(--island-ring), 0 2px 12px rgb(0 0 0 / 0.3);
  }

  .navpill button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--text);
    cursor: pointer;
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

  /* Its glass handed to App's row on a phone (lib/glass): the list starts under it, its first heading where the page's
     is (the glass's room, then its own 16 px; it began 12 px higher, the owner 2026-10-25). */
  .view.handed .scroll {
    padding-top: calc(var(--bar-room, 0px) + var(--mini-top, 0px) + var(--glass-top) + 16px);
    padding-bottom: calc(var(--mini-h, 0px) + var(--glass-bottom) + 16px);
  }

  .navpill button[aria-pressed='true'] {
    color: var(--accent);
  }

  /* A row's tick in Select (as Requests' lists). */
  .tick {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .tick.on {
    color: var(--accent);
  }
</style>
