<script lang="ts">
  // The account's Spotify lists in Discover (decision 0073; the Library's before 2026-10-29; the owner, 2026-10-27: to see what of it is here and ask
  // for what is not, no playing on Spotify): its albums, tracks, playlists or followed artists, as the view menu says,
  // each saying whether it is here, partly, missing or asked for; the Filter leaves out what it is told, the search
  // narrows them. What is here is the library's own: an album's cover plays it and a tap opens it (lib/AlbumCard), a
  // track plays from it, an artist opens their page. What is not: an album as every missing one (lib/ElsewhereAlbum:
  // on Bandcamp in the panel, else its menu with Want), Play on its cover its Bandcamp preview where one is found; a
  // track its album so; an artist their page from Discover. A playlist opens its tracks here (`?spotify=` in the
  // address, so back returns).
  import { untrack } from 'svelte';
  import {
    getSpotifyAlbums,
    getSpotifyArtists,
    getSpotifyPlaylist,
    getSpotifyPlaylists,
    getSpotifyTracks,
    type SpotifyAlbum,
    type SpotifyArtist,
    type SpotifyPlaylist,
    type SpotifyStatus,
    type SpotifyTrack,
  } from './api';
  import AlbumCard from './AlbumCard.svelte';
  import { goToArtist } from './albums.svelte';
  import { bandcampFailed, bandcampPlay, playOnBandcamp } from './bandcampFound.svelte';
  import { previewPage } from './bandcampPreview';
  import Cover from './Cover.svelte';
  import CoverPlay from './CoverPlay.svelte';
  import { DISCOVER } from './discoverArtist';
  import ElsewhereAlbum, { elsewhereKey, type Elsewhere } from './ElsewhereAlbum.svelte';
  import type { SpotifyView } from './discoverModes';
  import type { LocalTrack } from './localPlayer.svelte';
  import { playPlaylistFrom } from './output.svelte';
  import PageLoading from './PageLoading.svelte';
  import { matchesSearch, pageView } from './pageView.svelte';
  import ProblemSheet from './ProblemSheet.svelte';
  import { markOf } from './requests.svelte';
  import Row from './Row.svelte';
  import SectionHead from './SectionHead.svelte';
  import { spotifyPasses } from './spotify.svelte';
  import { toast } from './toast.svelte';

  let { view }: { view: SpotifyView } = $props();

  let albums = $state<SpotifyAlbum[] | null>(null);
  let tracks = $state<SpotifyTrack[] | null>(null);
  let playlists = $state<SpotifyPlaylist[] | null>(null);
  let artists = $state<SpotifyArtist[] | null>(null);
  let error = $state<string | null>(null);
  const said = (e: unknown) => (e instanceof Error && e.message ? e.message : 'It could not be read.');
  // Each list read when it shows (again each time: matching and requests move on).
  $effect(() => {
    const v = view;
    error = null;
    untrack(() => {
      const fail = (e: unknown) => void (error = said(e));
      if (v === 'spotify-albums') getSpotifyAlbums().then((r) => (albums = r.albums), fail);
      else if (v === 'spotify-tracks') getSpotifyTracks().then((r) => (tracks = r.tracks), fail);
      else if (v === 'spotify-playlists') getSpotifyPlaylists().then((r) => (playlists = r.playlists), fail);
      else getSpotifyArtists().then((r) => (artists = r.artists), fail);
    });
  });

  // A playlist opened: its tracks, by `?spotify=<id>` in the address.
  const openedId = () => new URLSearchParams(location.hash.split('?')[1] ?? '').get('spotify');
  let opened = $state<string | null>(openedId());
  let openList = $state<(SpotifyPlaylist & { tracks: SpotifyTrack[] }) | null>(null);
  $effect(() => {
    const on = () => (opened = openedId());
    addEventListener('hashchange', on);
    return () => removeEventListener('hashchange', on);
  });
  $effect(() => {
    const id = view === 'spotify-playlists' ? opened : null;
    openList = null;
    if (id)
      getSpotifyPlaylist(id).then(
        (p) => opened === id && (openList = p),
        (e) => (error = said(e)),
      );
  });
  function openPlaylist(p: SpotifyPlaylist) {
    const [h, rest] = location.hash.split('?');
    const q = new URLSearchParams(rest ?? '');
    q.set('spotify', p.id);
    location.hash = `${h}?${q}`;
  }

  // What shows: through the Filter's Leave out and the search.
  const keep = <T extends { status: SpotifyStatus }>(list: T[] | null, texts: (x: T) => (string | null | undefined)[]) =>
    list?.filter((x) => spotifyPasses(x.status) && matchesSearch(...texts(x))) ?? null;
  const albumsShown = $derived(keep(albums, (a) => [a.title, a.artist, a.year?.toString()]));
  const tracksShown = $derived(keep(openList?.tracks ?? tracks, (t) => [t.title, t.artist, t.album]));
  const playlistsShown = $derived(keep(playlists, (p) => [p.name, p.owner]));
  const artistsShown = $derived(keep(artists, (a) => [a.name]));
  const all = $derived(view === 'spotify-albums' ? albums : view === 'spotify-tracks' || openList ? (openList?.tracks ?? tracks) : view === 'spotify-playlists' ? playlists : artists);
  const shown = $derived(view === 'spotify-albums' ? albumsShown : view === 'spotify-tracks' || openList ? tracksShown : view === 'spotify-playlists' ? playlistsShown : artistsShown);
  // How many the filter lets through, of how many (the Filter sheet's count).
  $effect(() => {
    pageView.counted = all && shown ? { shown: shown.length, total: all.length } : null;
    return () => (pageView.counted = null);
  });

  // Long lists drawn as they come near: so many at first, so many more each time the end shows.
  const STEP = 240;
  let drawn = $state(STEP);
  $effect(() => {
    void view;
    void opened;
    drawn = STEP;
  });
  function more(node: HTMLElement) {
    const watch = new IntersectionObserver((seen) => seen.some((s) => s.isIntersecting) && (drawn += STEP), { rootMargin: '800px' });
    watch.observe(node);
    return () => watch.disconnect();
  }

  // Not here: opened as every missing album, asked for once, its Bandcamp preview on its cover.
  let elsewhere = $state<ElsewhereAlbum>();
  let finding = $state<string | null>(null);
  let asked = $state<string[]>([]);
  const missingOf = (a: { title: string; artist: string; year?: number | null; cover?: string | null; release_group: string | null; preview?: SpotifyAlbum['preview'] }): Elsewhere => ({
    release_group: a.release_group,
    title: a.title,
    artist: a.artist,
    year: a.year ?? null,
    cover: a.cover ?? null,
    bandcamp: a.preview ? previewPage(a.preview.id) : null,
  });
  let playProblem = $state<{ title: string; text: string; retry: () => void } | null>(null);
  const markFor = (x: { request: SpotifyAlbum['request']; release_group: string | null; title: string; artist: string }) =>
    markOf(x.request) ?? (asked.includes(elsewhereKey(x)) ? { state: 'wanted' as const } : null);

  /** A track's state in its row: what it is here. */
  const trackState = (t: SpotifyTrack) => (t.status === 'requested' ? 'Requested' : t.status === 'missing' ? 'Not here' : null);
  /** A covered track plays from it, with the covered ones of the list around it; a missing one opens its album. */
  async function tapTrack(t: SpotifyTrack, list: SpotifyTrack[]) {
    if (!t.track) return elsewhere?.open(missingOf({ title: t.album, artist: t.artist, release_group: t.release_group }));
    const source = t.track.source;
    const here = list.filter((x) => x.track?.source === source);
    const queue: LocalTrack[] = here.map((x) => ({
      source,
      id: x.track!.id,
      name: x.title,
      artist: x.artist,
      album: x.album,
      album_id: null,
      duration_secs: x.length_ms ? x.length_ms / 1000 : null,
    }));
    try {
      await playPlaylistFrom(openList?.name ?? 'Spotify tracks', source, queue, here.indexOf(t));
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
  const length = (ms: number | null) => (ms ? `${Math.floor(ms / 60_000)}:${String(Math.round(ms / 1000) % 60).padStart(2, '0')}` : undefined);
  const haveLine = (have: number, of: number, status: SpotifyStatus) =>
    status === 'requested' ? 'Requested' : status === 'missing' ? 'Not here' : status === 'partly' ? `${have} of ${of} here` : 'Here';
</script>

{#snippet trackRows(list: SpotifyTrack[])}
  <ul class="rows">
    {#each list.slice(0, drawn) as t (t.id)}
      <li>
        <Row size="medium" title={t.title} sub={[t.artist, t.album, trackState(t)].filter(Boolean).join(' · ')} count={length(t.length_ms)} off={!t.track} onclick={() => tapTrack(t, list)} />
      </li>
    {/each}
  </ul>
  {#if list.length > drawn}<span class="more" {@attach more}></span>{/if}
{/snippet}

{#if error}
  <p class="error" role="alert">{error}</p>
{:else if shown === null}
  <PageLoading />
{:else if view === 'spotify-albums' && albumsShown}
  <ul class="grid">
    {#each albumsShown.slice(0, drawn) as a (a.id)}
      <li class="album">
        {#if a.album}
          <AlbumCard source={a.album.source} album={{ id: a.album.id, title: a.title, year: a.year, artist: a.artist }} lines={[a.artist, ...(a.status === 'partly' ? [haveLine(a.have, a.of, a.status)] : [])]} />
        {:else}
          {@const m = missingOf(a)}
          <AlbumCard source="spotify" album={{ id: a.id, title: a.title, year: a.year }} cover={a.cover} busy={finding === elsewhereKey(m)} request={markFor(a)} lines={[a.artist]} onopen={() => elsewhere?.open(m)} />
          <!-- On Bandcamp: Play on its cover, as every missing album's (lib/ArtistBody). -->
          {#if m.bandcamp}
            {@const page = m.bandcamp}
            <CoverPlay
              bandcamp
              label={a.title}
              busy={bandcampPlay.key === a.id}
              problem={bandcampFailed[a.id] ?? null}
              onplay={() => playOnBandcamp(a.id, page, a.title)}
              onproblem={() => (playProblem = { title: `${a.title} did not play`, text: bandcampFailed[a.id], retry: () => playOnBandcamp(a.id, page, a.title) })}
            />
          {/if}
        {/if}
      </li>
    {/each}
  </ul>
  {#if albumsShown.length > drawn}<span class="more" {@attach more}></span>{/if}
  {#if !albumsShown.length}<p class="empty">No albums.</p>{/if}
{:else if openList && tracksShown}
  <SectionHead title={openList.name} />
  {@render trackRows(tracksShown)}
  {#if !tracksShown.length}<p class="empty">No tracks.</p>{/if}
{:else if view === 'spotify-tracks' && tracksShown}
  {@render trackRows(tracksShown)}
  {#if !tracksShown.length}<p class="empty">No tracks.</p>{/if}
{:else if view === 'spotify-playlists' && playlistsShown}
  {#if opened}
    <PageLoading />
  {:else}
    <ul class="rows">
      {#each playlistsShown as p (p.id)}
        <li>
          <Row size="large" title={p.name} sub={[p.owner, haveLine(p.have, p.of, p.status)].filter(Boolean).join(' · ')} count={p.of} onclick={() => openPlaylist(p)}>
            {#snippet lead()}<span class="art"><Cover src={p.cover} /></span>{/snippet}
          </Row>
        </li>
      {/each}
    </ul>
    {#if !playlistsShown.length}<p class="empty">No playlists.</p>{/if}
  {/if}
{:else if artistsShown}
  <ul class="rows">
    {#each artistsShown.slice(0, drawn) as a (a.id)}
      <li>
        <Row
          size="medium"
          title={a.name}
          sub={a.status === 'missing' ? 'Not here' : `${a.have} ${a.have === 1 ? 'album' : 'albums'} here`}
          off={!a.artist}
          onclick={() => goToArtist(a.artist ? 'cratarr' : DISCOVER, a.name)}
        >
          {#snippet lead()}<span class="face"><Cover src={a.photo} /></span>{/snippet}
        </Row>
      </li>
    {/each}
  </ul>
  {#if artistsShown.length > drawn}<span class="more" {@attach more}></span>{/if}
  {#if !artistsShown.length}<p class="empty">No artists.</p>{/if}
{/if}

<ElsewhereAlbum bind:this={elsewhere} bind:finding bind:asked />
{#if playProblem}{@const x = playProblem}<ProblemSheet title={x.title} text={x.text} retry={x.retry} onclose={() => (playProblem = null)} />{/if}

<style>
  /* As an artist's albums (lib/ArtistBody): as many columns as fit. */
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 20px 12px;
    margin: 0;
    padding: 4px 0 0;
    list-style: none;
  }

  /* A missing album's card with Play over its cover (lib/CoverPlay is placed by it). */
  .album {
    position: relative;
  }

  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .art,
  .face {
    display: block;
    width: 48px;
    height: 48px;
    overflow: hidden;
    border-radius: 6px;
  }

  .face {
    width: 40px;
    height: 40px;
    border-radius: 50%;
  }

  .more {
    display: block;
    height: 1px;
  }

  .empty {
    color: var(--muted);
    font-size: 0.9rem;
  }

  .error {
    color: var(--danger-ink);
  }
</style>
