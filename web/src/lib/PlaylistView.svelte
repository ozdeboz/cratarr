<script lang="ts">
  import PlayingDots from './PlayingDots.svelte';
  import { askIfMany, fewAtATime } from './many.svelte';
  import { untrack } from 'svelte';
  import { holdUntil } from './holdPlace.svelte';
  // A viewed playlist (decision 0023), in the panel beside the page or in its own layer, styled as Finamp's playlist
  // screen (lib/components/AlbumScreen) in less room: a bar with ✕ and the menu; the covers beside the name, "N tracks
  // (length)" and the genres; Play, Shuffle, Play next and Add to queue as icons over labels; what Insights knows of it
  // (Related: the Info tab); then the tracks in file order. A tap on a track plays the playlist from
  // there on the chosen output, a swipe right plays it next, a swipe left takes it out of the crate or the playlist (with
  // Undo; not an album's or an artist's), a long-press is its menu; the heart adds it to playlists
  // (long-press: favourite). Lines the server does not know, or files that are missing, show by file name, dimmed.
  // Edit mode (the pencil; Finamp's playlist edit screen): rows get the reorder handle and ✕ (a swipe left removes too),
  // removed tracks wait in their own section to be restored, and Save writes the playlist once (Cancel drops it).
  // A crate (decision 0024, `?view=crate:…`) shows the same way, without the statistics (genres, length, Related) and
  // without editing; its rows come in pages (newest first), and the save icon makes a playlist of it (a snapshot).
  // Playlists show in tabs (decision 0025): Tracks, Info, Export, Repair, Sync, under the header, for the selection: one
  // playlist as above, or several (the Playlists page's Select): their covers and count in the header, and on Tracks a
  // section per playlist, in the order ticked, each folding, read-only. A crate has no tabs.
  // An album (`?view=album:<server>/<id>`, from the Incoming page or a track's Go to album) shows as a crate does, its
  // tracks in play order, with all four actions: an album, like a crate, is a list of tracks but no playlist file.
  import {
    albumResume,
    albumView,
    type AlbumView,
    artistFavorite,
    artistView,
    type ArtistView,
    coverUrl,
    crateRows,
    filterList,
    likeCurve,
    type LikeCurveData,
    type ListProfileData,
    type CrateLook,
    crateToPlaylist,
    createCrate,
    createPlaylist,
    deleteCrate,
    getCrate,
    artistPicks,
    crateLeftOut,
    appendTracks,
    leaveOutOfCrate,
    updateCrate,
    editPlaylist,
    albumCoverUrl,
    playlistCovers,
    playlistRows,
    queueTrack,
    setTrackFavorite,
    setAlbumFavorite,
    getArtistAbout,
    artistPhotoUrl,
    getDiscoverArtist,
    getDiscoverArtistAlbums,
    discoverArtistPhotoUrl,
    type ListedAlbum,
    type PlaylistRow,
  } from './api';
  import {
    builderOf,
    CRATE,
    crateArt,
    crateName,
    crateNameKnown,
    homeCrate,
    crateOf,
    crates,
    crateRadio as crateRadioOf,
    cycleFavouriteAlbums,
    cycleFavourites,
    EDIT_CRATE,
    GENRE,
    FAVOURITE_ALBUMS,
    FAVOURITES,
    favouriteAlbumsOf,
    favouritesOf,
    leaveOut,
    favouritesSaved,
    forgetDraft,
    keepDraft,
    likeParts,
    nameLike,
    seeded,
    viewLike,
    keptDraft,
    loadCrates,
    setCrateArt,
    startCrate,
    viewGenre,
    type CrateDraft,
  } from './crates.svelte';
  import { DEVICE, deviceAlbumView, deviceArtistView } from './deviceMusic.svelte';
  import { BANDCAMP } from './bandcampPreview';
  import { DISCOVER, discoverArtistView } from './discoverArtist';
  import RowHeart from './RowHeart.svelte';
  import type { ExcludeEntry } from './crates.svelte';
  import CrateEditor from './CrateEditor.svelte';
  import SelectSheet from './SelectSheet.svelte';
  import { Reorder } from './reorder.svelte';
  import { followCreate, followDelete, followRename, renameEverywhere } from './follow.svelte';
  import { undos, withUndo } from './undo.svelte';
  import { isFavorite, isPlaylistFavorite, loadAlbumFavorites, loadFavorites, loadPlaylistFavorites, setArtistHeart, setFavorite, setPlaylistFavorites } from './favourites.svelte';
  import Icon, { type IconName } from './Icon.svelte';
  import ProgressRing from './ProgressRing.svelte';
  import { untilAudible } from './audible.svelte';
  import AlbumMenu, { offerIn } from './AlbumMenu.svelte';
  import BandcampAlbumMenu from './BandcampAlbumMenu.svelte';
  import CrateMenu from './CrateMenu.svelte';
  import SubjectMenu from './SubjectMenu.svelte';
  import SelectionMenu from './SelectionMenu.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import { pickOf, playCollection } from './play.svelte';
  import { away, here } from './away.svelte';
  import { addedOn, artistPictureId, goToArtist, library, libraryDot, mainArtist, readLibrary } from './albums.svelte';
  import AlbumAbout from './AlbumAbout.svelte';
  import AlbumMore from './AlbumMore.svelte';
  import SimilarAlbums from './SimilarAlbums.svelte';
  import CoverFlow from './CoverFlow.svelte';
  import SimilarLists from './SimilarLists.svelte';
  import SuggestedTracks from './SuggestedTracks.svelte';
  import { glass, handGlass } from './glass.svelte';
  import { excludedArtists, loadCrateArtists, setNotInCrate } from './excludedArtists.svelte';
  import GlobalResults from './GlobalResults.svelte';
  import { everywhere } from './globalSearch.svelte';
  import AvoidLine from './AvoidLine.svelte';
  import ListHead from './ListHead.svelte';
  import SectionHead from './SectionHead.svelte';
  import { crateViewOf, holdsLine, orderRecords, RECORD_ORDERS, recordsOf, type CrateRecord, type RecordOrder } from './crateRecords';
  import { insights, plural } from './insights.svelte';
  import { refresh } from './live.svelte';
  import { enqueue, startRadio, type LocalTrack } from './localPlayer.svelte';
  import { inApp } from './app.svelte';
  import { isKept, setKept } from './kept.svelte';
  import { longpress } from './longpress';
  import { playlistsKnown, playPlaylistFrom, shownPlaylists, startPlaylists } from './output.svelte';
  import { port } from './player/port.svelte';
  import { shuffled } from './shuffle';
  import { albumMoved, albumOf, albumSubject, artistOf, artistSubject, backToTracks, closePlaylist, goHome, NEW_PLAYLIST, panelDepth, viewOver, viewPlaylist, panel, TABS } from './panel.svelte';
  import ArtistBody from './ArtistBody.svelte';
  import ArtistMenu from './ArtistMenu.svelte';
  import { coverKey, localCovers } from './localCovers.svelte';
  import ExportTab from './ExportTab.svelte';
  import PlaylistItem from './PlaylistItem.svelte';
  import Row from './Row.svelte';
  import TrackCover from './TrackCover.svelte';
  import Section from './Section.svelte';
  import { layerColor } from './Spread.svelte';
  import ListProfile from './ListProfile.svelte';
  import LikeCurve from './LikeCurve.svelte';
  import type { Cell } from './Row.svelte';
  import { fit } from './columns.svelte';
  import { fromAction } from 'svelte/attachments';
  import RepairTab from './RepairTab.svelte';
  import SyncTab from './SyncTab.svelte';
  import PlaylistMenu from './PlaylistMenu.svelte';
  import PanelPlace from './PanelPlace.svelte';
  import TopPill from './TopPill.svelte';
  import SelectBar from './SelectBar.svelte';
  import { work } from './panelWork.svelte';
  import { prefs, type ListSort, type PanelKind } from './prefs.svelte';
  import { filtersOn, hiddenLibraries, inView, passing } from './listFilter.svelte';
  import { matchesSearch, pageView, trackOrders, type TrackOrder } from './pageView.svelte';
  import { words } from './search';
  import { clockText, paused, playing } from './remote.svelte';
  import { on } from './events.svelte';
  import { swipe } from './swipe';
  import SwipeBehind from './SwipeBehind.svelte';
  import ExcludedSheet from './ExcludedSheet.svelte';
  import { toast } from './toast.svelte';
  import { me } from './me.svelte';
  import TrackMenu, { type MenuTrack } from './TrackMenu.svelte';
  import MarkLoader from './MarkLoader.svelte';
  import Cover, { coverBroke, coverBroken, mosaicOf } from './Cover.svelte';
  import PageLoading from './PageLoading.svelte';
  import type { PlayMode } from './api';
  import { flowOpened } from './backdrop.svelte';

  const id = $derived(panel.viewed ?? '');
  /** New playlist: edit mode with no playlist yet (no tracks, no file); Save makes it. */
  const creating = $derived(panel.viewed === NEW_PLAYLIST);
  /** A crate being made (`crate-new`) or one's own being changed (`crate-edit:<id>`): its edit mode, the editor where
   *  its tracks were (decision 0024). */
  const builder = $derived(builderOf(panel.viewed));
  const crateEditing = $derived(builder !== null);
  let crateDraft = $state<CrateDraft | null>(null);
  /** Edit mode's picture, saved with Save as the rest is: one picked (shown at once), or the crate's own removed. */
  let crateImage = $state<{ file: Blob; url: string } | 'remove' | null>(null);
  /** The viewed crate's id, when a crate is viewed (not a playlist), or changed. */
  const crate = $derived(crateOf(panel.viewed) ?? builder?.id ?? null);
  // The artists left out of it by hand (0.10, decision 0083), read as it shows.
  $effect(() => {
    if (crate && !crateEditing) untrack(() => loadCrateArtists(crate));
  });
  // Its name comes from the crates' list, read on the Crates page: opened by a link or after a reload, it is read here
  // (or the title would be the crate's id).
  $effect(() => {
    if (crate && crates.list === null) untrack(() => loadCrates());
  });
  /** The viewed album (an Incoming one, or a track's), when an album is viewed. */
  const album = $derived(albumOf(panel.viewed));
  /** The viewed artist (their page), or all their tracks (`tracks`: their Top recommendations' ›), when an artist is viewed. */
  const artist = $derived(artistOf(panel.viewed));
  /** A crate, an album or an artist: tracks without a playlist's file (no tabs, no editing, no statistics). */
  const plain = $derived(!!crate || !!album || !!artist || crateEditing || creating);
  /** What the artist's answer says besides their tracks: their albums, what they appear on, the heart. */
  let artistHead = $state<(Pick<ArtistView, 'albums' | 'appears_on' | 'favorite' | 'mbid'> & { studio?: number | null }) | null>(null);
  /** What the album's tracks say of it: its name, artist and year; its files' format and weakest bitrate. */
  // The album's library, as a dot before its details (0.9.3): which copy this is, beside its year.
  const albumDot = $derived(album ? libraryDot(album.source, { id: album.id }) : null);
  let albumHead = $state<Pick<AlbumView, 'title' | 'artist' | 'year' | 'format' | 'added' | 'played' | 'plays' | 'released' | 'label' | 'about' | 'credits' | 'tags' | 'links' | 'listeners'> & { kbps: number | null } | null>(null);
  /** An album's or a crate's genres, most shared first, as its first page came (a playlist's are its Insights'). */
  let listGenres = $state<string[] | null>(null);
  /** The selected playlists; several: Tracks shows a section per playlist (decision 0025). */
  const ids = $derived(panel.selection);
  const multi = $derived(ids.length > 1);
  const tab = $derived(plain ? 'tracks' : panel.tab);
  // One of the playlists' other views (from their menu): a plain bar with back to the tracks, no cover.
  const other = $derived(tab !== 'tracks');
  const title = $derived(
    crateEditing
      ? crateDraft?.name.trim() || 'New crate'
      : creating
        ? 'New playlist'
        : crate
          ? (crateNameKnown(crate) ?? '')
          : album
            ? (albumHead?.title ?? '')
            : artist
              ? artist.tracks
                ? 'All tracks'
                : artist.name
              : multi
              ? `${ids.length} playlists`
              : id,
  );
  const isPermanentCrate = $derived(!!crate && (crates.list?.find((c) => c.id === crate)?.permanent ?? true));
  const p = $derived(plain || multi ? null : (shownPlaylists().find((x) => x.id === id) ?? null));
  /** The selected playlists, in the order ticked (for the menu, the counts). */
  const selected = $derived(ids.flatMap((x) => shownPlaylists().filter((y) => y.id === x)));
  const chosen = $derived(multi ? selected : []);
  const chosenDead = $derived(chosen.reduce((n, x) => n + x.dead, 0));
  // The heart: on when every selected playlist is a favourite; a tap sets all of them.
  const allFav = $derived(ids.length > 0 && ids.every((x) => isPlaylistFavorite(x)));
  const chosenCount = $derived(chosen.reduce((n, x) => n + x.count, 0));
  const np = $derived(playing());
  /** A crate's size (its rows come in pages). */
  let total = $state(0);
  const PAGE = 200;
  // A crate changes as tracks are sorted into playlists (Unlisted) or hearted: asked for again when the playlists change.
  const listsKey = $derived(crate ? shownPlaylists().map((x) => x.count).join(',') : '');
  // An Incoming album changes as Lidarr adds to it (or it is kept or deleted elsewhere): asked for again when Incoming
  // changes.
  let incomingChanged = $state(0);
  $effect(() => on<{ area: string }>('changed', ({ area }) => (area === 'incoming' || area === 'collection') && album && incomingChanged++));

  // The album's place in the Library's list (lib/albums.svelte.ts): its library, and what Cratarr may do with it there,
  // for its menu. Read when an album shows and the list is not known yet, and again when an album list changed (the
  // Library page, while it shows, keeps it fresh itself).
  let seenChange = 0;
  $effect(() => {
    const a = album;
    const n = incomingChanged;
    if (!a) return;
    loadAlbumFavorites(a.source, [a.id]);
    if (untrack(() => library.view) === null || n !== seenChange) {
      seenChange = n;
      readLibrary(n > 0).catch(() => {});
    }
  });
  const listedHere = $derived.by(() => {
    const a = album;
    const s = a ? library.view?.sources.find((x) => x.id === a.source) : undefined;
    const la = s?.albums.find((x) => x.id === a!.id);
    return s && la ? { s, a: la } : null;
  });

  // The album's artist chip: who it opens (their panel), and whether they are a favourite (a heart in the chip, as on
  // their own panel's chip).
  const albumBy = $derived(album && albumHead?.artist ? (listedHere?.a.artists?.[0] ?? mainArtist(albumHead.artist)) : null);
  let albumByFavorite = $state<boolean | null>(null);
  $effect(() => {
    const a = album;
    const by = albumBy;
    albumByFavorite = null;
    if (!a || !by) return;
    let live = true;
    artistFavorite(a.source, by).then(
      (r) => live && (albumByFavorite = r.favorite),
      () => {},
    );
    return () => {
      live = false;
    };
  });

  // Where the album comes from (its About): the server, and its library as the Library's list names it.
  const provider = $derived.by(() => {
    const s = album ? library.view?.sources.find((x) => x.id === album.source) : undefined;
    const l = listedHere?.a.library ? s?.library_info?.find((x) => x.id === listedHere.a.library) : undefined;
    return { server: s?.label ?? album?.source ?? '', library: l?.name ?? null, colour: l?.colour ?? null };
  });

  // The album's heart: shows the change at once, back as it was when the server refuses.
  const albumFav = $derived(album ? isFavorite(album.source, album.id) : false);
  /** An album or artist of the device's own music (local mode, decision 0052): played and played next, nothing of
   *  Cratarr's (hearts, menus, About, radio, select). */
  const onDevice = $derived((album?.source ?? artist?.source) === DEVICE);
  // Played here and kept by none of Cratarr's: the device's own music, or a Bandcamp album (a Discover suggestion's,
  // decision 0071, the owner 2026-10-25): its tracks, no heart, rating, Resume or similar albums of the server's.
  // An artist from Discover, or one asked for on Bandcamp (no server of that name): Discover's page of them.
  const fromDiscover = (source: string | undefined) => source === DISCOVER || source === BANDCAMP;
  const offServer = $derived(onDevice || album?.source === BANDCAMP || fromDiscover(artist?.source));
  async function toggleAlbumFavorite() {
    if (!album) return;
    const { source: s, id: a } = album;
    const on = !albumFav;
    setFavorite(s, a, on);
    navigator.vibrate?.(8);
    try {
      await setAlbumFavorite(s, a, on);
    } catch (e) {
      setFavorite(s, a, !on);
      toast.fail(e, "The favourite didn't change");
    }
  }

  // The rows: asked for when the playlist is viewed and again when it changes (its counts, from the shell's state).
  let rows = $state<PlaylistRow[] | null>(null);
  let source = $state('');
  let error = $state<string | null>(null);
  let shownFor = '';
  /** Bumped after a save: the rows are asked for again (a reorder changes no count), as after an undo or redo. */
  let saved = $state(0);
  $effect(() => {
    const want = id;
    void p?.count;
    void p?.dead;
    void saved;
    // An artist left out of this crate or taken back (0.10, decision 0083): its rows again.
    void excludedArtists.version;
    void undos.n;
    void listsKey;
    void incomingChanged;
    if (!want || multi || builderOf(want) || want === NEW_PLAYLIST) return;
    const c = crateOf(want);
    const a = albumOf(want);
    const ar = artistOf(want);
    // A crate's rows wait for the crates (read on opening, above): asked before, they were asked again with its
    // favourites setting, the list showing, then loading, then again (the owner, 2026-10-23).
    if (c && !crates.list && !crates.error) return;
    // As the toggle shows it (the server's saved setting until the crates are known).
    const shownFav = () => (c && crates.list ? favouritesOf(c) : undefined);
    const fav = shownFav();
    const alb = c && crates.list ? favouriteAlbumsOf(c) : undefined;
    const look = c ? untrack(() => crateLook) : {};
    const key = `${want}\n${fav}\n${alb}\n${c ? lookKey : ''}`;
    if (key !== shownFor) {
      rows = null;
      error = null;
      albumHead = null;
      artistHead = null;
      listGenres = null;
    }
    // A crate keeps the pages it has (at least the first) when it is asked for again.
    const asked = c
      ? crateRows(c, 0, Math.max(PAGE, untrack(() => rows?.length ?? 0)), prefs.source || undefined, fav, alb, look)
      : a
        ? (a.source === DEVICE ? Promise.resolve(deviceAlbumView(a.id)) : albumView(a.source, a.id)).then((r) => {
            if (want === id && r.id !== a.id) albumMoved(a.source, a.id, r.id);
            if (want === id)
              albumHead = { title: r.title, artist: r.artist, year: r.year, format: r.format ?? null, kbps: r.bitrate_kbps ?? null, added: r.added, played: r.played, plays: r.plays, released: r.released, label: r.label, about: r.about, credits: r.credits, tags: r.tags, links: r.links, listeners: r.listeners };
            return { ...r, total: r.rows.length };
          })
        : ar
          ? (ar.source === DEVICE ? Promise.resolve(deviceArtistView(ar.name)) : fromDiscover(ar.source) ? discoverArtistView(ar.name) : artistView(ar.source, ar.name)).then((r) => {
              if (want === id) {
                artistHead = { albums: r.albums, appears_on: r.appears_on, favorite: r.favorite, mbid: (r as { mbid?: string }).mbid, studio: (r as { studio?: number | null }).studio };
                // Their picture: their newest album's cover (Symfonium's, without an artist's photo); one from Discover's
                // albums are Bandcamp's.
                const first = r.albums[0] ?? r.appears_on[0];
                covers = first ? [albumCoverUrl(r.source === DISCOVER ? BANDCAMP : r.source, first.id)] : [];
              }
              return { ...r, total: r.rows.length, genres: null };
            })
          : playlistRows(want, prefs.source || undefined).then((r) => ({ ...r, total: r.rows.length, genres: null }));
    asked.then(
      (r) => {
        if (want !== id || shownFav() !== fav) return;
        shownFor = key;
        error = null;
        rows = r.rows;
        rowsFor = want;
        source = r.source;
        total = r.total;
        listGenres = r.genres;
        if (r.source !== DEVICE && r.source !== BANDCAMP && r.source !== DISCOVER) loadFavorites(r.source, r.rows.flatMap((x) => (x.id ? [x.id] : [])));
      },
      (e) => {
        if (want !== id) return;
        error = e instanceof Error ? e.message : 'Unexpected error';
        rowsFor = want;
      },
    );
  });
  /** The item the rows (or their error) are of. */
  let rowsFor = $state('');
  // One commit (lib/holdPlace; the owner, 2026-10-26, "All together"): the panel keeps what it showed until this item's
  // picture, header and rows are in; Similar and what else is asked for later may follow.
  holdUntil(
    () => id,
    () =>
      !id ||
      multi ||
      crateEditing ||
      creating ||
      (rowsFor === id && (error !== null || (!heroPending && (!album || albumHead !== null) && (!artist || artistHead !== null)))),
  );

  // The covers: the playlist's first albums, four as a mosaic (decision 0021).
  let covers = $state<string[]>([]);
  /** The covers asked for have come (or are not asked for): until then the hero holds its place (below). */
  let coversKnown = $state(false);
  $effect(() => {
    const want = id;
    covers = [];
    coversKnown = true;
    // An artist's comes with their albums (above).
    if (!want || crateOf(want) || builderOf(want) || want === NEW_PLAYLIST || artistOf(want)) return;
    const a = albumOf(want);
    if (a) {
      covers = [albumCoverUrl(a.source, a.id)];
      return;
    }
    coversKnown = false;
    if (multi) {
      // Several: the first cover of each of the first four, as a mosaic.
      const first = ids.slice(0, 4);
      Promise.all(first.map((x) => playlistCovers(x).catch(() => []))).then((lists) => {
        if (want !== id) return;
        const urls = lists.flatMap((l) => (l[0] ? [albumCoverUrl(l[0].source, l[0].album_id)] : []));
        covers = urls.length >= 4 ? urls.slice(0, 4) : urls.slice(0, 1);
        coversKnown = true;
      });
      return;
    }
    playlistCovers(want).then(
      (list) => {
        if (want !== id) return;
        const urls = list.map((c) => albumCoverUrl(c.source, c.album_id));
        covers = urls.length >= 4 ? urls.slice(0, 4) : urls.slice(0, 1);
        coversKnown = true;
      },
      () => want === id && (coversKnown = true),
    );
  });

  // An artist's photo, where the server's artist store has one (TheAudioDB's): the header shows it rather than an album
  // cover (the owner, 2026-10-23); a picture chosen on this device still comes first.
  let artistPhoto = $state<string | null>(null);
  /** Whether they have one is known: until then neither the photo nor an album's cover shows (no cover that the photo
   *  then replaces). */
  let artistPhotoKnown = $state(true);
  $effect(() => {
    const ar = artist;
    artistPhoto = null;
    artistPhotoKnown = true;
    if (!ar || ar.tracks) return;
    // One from Discover (lib/discoverArtist): their picture as New to you has it.
    if (fromDiscover(ar.source)) {
      artistPhotoKnown = false;
      let gone = false;
      getDiscoverArtist(ar.name).then(
        (a) => {
          if (gone) return;
          if (a.photo) artistPhoto = discoverArtistPhotoUrl(ar.name);
          artistPhotoKnown = true;
        },
        () => !gone && (artistPhotoKnown = true),
      );
      return () => (gone = true);
    }
    if (ar.source !== 'cratarr') return;
    artistPhotoKnown = false;
    let gone = false;
    getArtistAbout(ar.name).then(
      (a) => {
        if (gone) return;
        if (a.photo && a.id) artistPhoto = artistPhotoUrl(a.id);
        artistPhotoKnown = true;
      },
      () => !gone && (artistPhotoKnown = true),
    );
    return () => (gone = true);
  });

  // An artist from Discover without the look's count of their studio albums: the server's, counted when first asked (a
  // few seconds the first time), filled in when it comes (the owner, 2026-10-25).
  $effect(() => {
    const ar = artist;
    const h = artistHead;
    if (!ar || !fromDiscover(ar.source) || !h || h.studio) return;
    let gone = false;
    getDiscoverArtistAlbums(ar.name).then(
      (r) => {
        if (!gone && r.albums && artistHead === h) artistHead = { ...h, studio: r.albums };
      },
      () => {},
    );
    return () => (gone = true);
  });

  // The cover's place: a crate's picture (its own, else its albums' covers; in edit mode as Save will leave it).
  const heroArt = $derived.by(() => {
    if (crateEditing) {
      if (crateImage && crateImage !== 'remove') return [crateImage.url];
      return builder?.id ? crateArt(builder.id, crateImage !== 'remove') : [];
    }
    if (artist) {
      const own = localCovers[coverKey(artist.source, artistPictureId(artist.name))];
      return own ? [own] : artistPhoto ? [artistPhoto] : covers;
    }
    if (crate && seeded(crate)) return rowCovers;
    return crate ? crateArt(crate) : covers;
  });
  /** A Sounds like or genre crate's covers: its first rows' first four albums, as a mosaic (it has no picture of its
   *  own, and showed Cratarr's mark). */
  const rowCovers = $derived.by(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const r of rows ?? []) {
      if (!r.id || r.missing) continue;
      const key = r.album_id ?? `${r.album ?? ''}
${r.album_artist ?? r.artist ?? ''}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(coverUrl({ source, id: r.id, album_id: r.album_id }));
      if (out.length === 4) break;
    }
    return out.length >= 4 ? out : out.slice(0, 1);
  });
  /** A crate's albums to flip through in its header (lib/CoverFlow; the owner, 2026-10-23): its rows' albums as loaded,
   *  each once. A playlist's too where its header is too wide for its cover (the owner, 2026-10-26: the same rule as an
   *  album's single cover, less than 90% of it shown). */
  const flowCovers = $derived.by(() => {
    if (crateEditing || !(crate || (kind === 'playlist' && heroWide))) return [];
    // One cover an album (the owner, 2026-10-23: duplicates consolidated): by its name and album artist in any case (the
    // same album in Music and in Incoming, or without its id), and no cover twice.
    const seen = new Set<string>();
    const out: { url: string; album: { id: string; title: string; artist: string | null } | null }[] = [];
    for (const r of rows ?? []) {
      if (!r.id || r.missing) continue;
      const named = r.album ? `${r.album}\n${r.album_artist ?? r.artist ?? ''}`.toLowerCase() : null;
      const url = coverUrl({ source, id: r.id, album_id: r.album_id });
      const keys = [named, r.album_id, url].filter((k): k is string => !!k);
      if (keys.some((k) => seen.has(k))) continue;
      for (const k of keys) seen.add(k);
      // Its album, for the long-press's menu (the owner, 2026-10-24).
      out.push({ url, album: r.album_id ? { id: r.album_id, title: r.album ?? '', artist: r.album_artist ?? r.artist ?? null } : null });
      if (out.length === 60) break;
    }
    return out;
  });
  // Each fetched ahead: an album without art answers 404 and is left out (the owner, 2026-10-23: no empty covers here).
  $effect(() => {
    for (const { url } of flowCovers) {
      if (coverBroken(url)) continue;
      const probe = new Image();
      probe.onerror = () => coverBroke(url);
      probe.src = url;
    }
  });
  const flowItems = $derived(flowCovers.filter((c) => !coverBroken(c.url)));
  const flowShown = $derived(flowItems.map((c) => c.url));
  /** A flow cover's album, its menu open (a long-press or a right-click on it). */
  let flowMenu = $state<{ id: string; title: string; artist: string | null } | null>(null);
  const flowing = $derived(flowShown.length >= 3);
  // A crate's flow opening: Shuffle's next (Settings › Appearance).
  $effect(() => {
    const of = id;
    if (flowing && of) untrack(() => flowOpened(of));
  });
  // One cover in a header too wide for it (the owner, 2026-10-26): the square fills the width up to 60% of the window's
  // height, so a wide panel shows only a band of it. Where less than 90% of it would show, the flow's canvas instead,
  // the cover whole in the middle (lib/CoverFlow, one cover alone).
  let heroW = $state(0);
  let heroH = $state(0);
  const heroWide = $derived(heroW > 0 && heroH / heroW < 0.9);
  /** The hero's art is not known yet (the owner, 2026-10-23: nothing shown that something else then replaces): it keeps
   *  its full size, empty; a crate's on the coverflow's canvas, which most become. */
  const heroPending = $derived.by(() => {
    if (crateEditing) return false;
    if (artist) return !artistPhotoKnown || rows === null;
    if (crate) return rows === null || (!seeded(crate) && !crates.list);
    return !coversKnown;
  });
  // As drawn: covers that fail to load leave a mosaic (lib/Cover).
  const coversShown = $derived(mosaicOf(covers));
  const heroShown = $derived(mosaicOf(heroArt));
  /** One cover, too wide for it: the flow's canvas instead (above). Not while a crate is edited (its tools sit there). */
  const oneWide = $derived(!heroPending && !flowing && !crateEditing && heroShown.length === 1 && heroWide);
  /** Edit mode: there is a picture of its own to remove (as saved, or picked). */
  const ownImage = $derived(crateImage ? crateImage !== 'remove' : !!crates.list?.find((c) => c.id === builder?.id)?.image);

  /** Offline (lib/reach), a track that cannot play: not on this device (the owner, 2026-10-26: dimmed; lib/away). */
  const awayDim = (r: PlaylistRow, src: string) => away() && !here(src, r.id);
  const length = $derived(rows ? rows.reduce((a, r) => a + (r.duration_secs ?? 0), 0) : 0);
  const playable = $derived(rows ? rows.filter((r) => r.id && !r.missing) : []);

  // Symfonium's header over the tracks: their order (the subject's own, or by a field, either way round), how many, and
  // select mode. The list shows and plays in that order; an album's rows keep their track numbers. Every list of tracks
  // offers all its rows know, remembered per kind of list (decision 0060); an artist's All tracks start most played
  // first and are not remembered.
  const kind = $derived<PanelKind>(album ? 'album' : crate || crateEditing || creating ? 'crate' : artist ? 'artist' : 'playlist');
  const topTracks = $derived(!!artist?.tracks);
  let topSort = $state<ListSort>({ key: 'plays', down: true });
  const sortFields = $derived(trackOrders(kind));
  const sort = $derived.by(() => {
    const s = topTracks ? topSort : prefs.panelSorts[kind];
    return s && sortFields.some((f) => f.value === s.key) ? s : { key: 'own', down: false };
  });
  const sortKey = $derived(sort.key as TrackOrder);
  const sortDown = $derived(sort.down);
  function setSort(key: string, down: boolean) {
    if (topTracks) topSort = { key, down };
    else prefs.panelSorts[kind] = { key, down };
  }
  let sortOpen = $state(false);
  // Columns where the list has room (decision 0042): as many as its own width allows, in the owner's order; not on an
  // album's numbered tracks.
  let listWidth = $state(0);
  const cols = $derived(album ? [] : fit('tracks', listWidth, (id) => sortFields.some((f) => f.value === id)));
  const shows = (id: string) => cols.some((c) => c.id === id);
  /** A track's value in a column. */
  function cellText(id: string, r: PlaylistRow): string {
    if (id === 'album') return r.album ?? '';
    if (id === 'albumArtist') return r.album_artist ?? '';
    if (id === 'tempo') return r.bpm ? String(Math.round(r.bpm)) : '';
    if (id === 'plays') return r.play_count ? r.play_count.toLocaleString() : '';
    if (id === 'year') return r.year ? String(r.year) : '';
    if (id === 'lastPlayed') return r.last_played ? addedOn(r.last_played) : '';
    if (id === 'added') return r.added ? addedOn(r.added) : '';
    return '';
  }
  const cellsOf = (r: PlaylistRow): Cell[] | undefined =>
    cols.length ? cols.map((c) => ({ ...c, text: cellText(c.id, r) })) : undefined;
  /** The list's head: its column names, a tap orders by one, a second turns it round. Plays and dates start most and
   *  newest first. */
  const headCells = $derived<Cell[]>(
    cols.map((c) => ({
      ...c,
      text: c.label,
      on: sortKey === c.id,
      onclick: () => setSort(c.id, sortKey === c.id ? !sortDown : !!sortFields.find((f) => f.value === c.id)?.most),
    })),
  );
  // The same order in the pill's View (lib/ViewSheet).
  const listOrder = $derived({
    fields: sortFields,
    value: sortKey,
    down: sortDown,
    set: setSort,
    columns: { kind: 'tracks' as const, shown: cols.map((c) => c.id), offered: sortFields.map((f) => f.value) },
    view: crate && !crateEditing ? { value: crateViewOf(crate, prefs.crateViews), set: (v: 'records' | 'tracks') => (prefs.crateViews = { ...prefs.crateViews, [crate]: v }) } : undefined,
  });
  // The list's filter, remembered per kind of list (decisions 0060 and 0061): its switches (Favourites only, Unplayed
  // only, Hide missing tracks) and its conditions (any of what the server knows of a track: genre, mood, tempo, …),
  // both the server's, which says which rows pass (the owner, 2026-10-29: one filter, one engine). A crate's rows come
  // filtered and ordered by the server, as they are paged, from the libraries switched on.
  // A playlist's only (the owner, 2026-10-24): an album is too short and alike to filter, a crate is a filter already,
  // an artist's lists are theirs; a filter left on there from before goes by nothing.
  const filterable = $derived(kind === 'playlist');
  // A Home shelf's crate shows what Home showed, of every library, whatever is switched off (the owner, 2026-10-24).
  const crateLook = $derived<CrateLook>({ without: homeCrate(crate) ? undefined : hiddenLibraries(), filter: undefined, order: sortKey, down: sortDown });
  const lookKey = $derived(JSON.stringify(crateLook));
  const pass = passing(() => kind, () =>
    filterable && !crate && !album && !artist && p && !multi ? { list: { kind: 'playlist', id: p.id }, source: prefs.source || undefined } : null,
  );
  const keeps = (r: PlaylistRow) => !pass.keys || pass.keys.has(r.entry);
  const ordered = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base', numeric: true });
  /** The rows as the list shows them, each with its place in the subject (`i`). */
  const shown = $derived.by(() => {
    // What a filter goes by not in yet: loading, not the rows unfiltered or none (the owner, 2026-10-23).
    if (!rows || (pass.pending && !pass.keys)) return null;
    // The page's search narrows what its panel shows too (the owner, 2026-10-25: the page finds, the panel shows the
    // item narrowed to the same words): a track whose title, artist or album holds every word.
    const list = rows.map((r, i) => ({ r, i })).filter((x) => keeps(x.r) && matchesSearch(x.r.name ?? x.r.file, x.r.artist, x.r.album));
    const text = (r: PlaylistRow) =>
      sortKey === 'title' ? (r.name ?? r.file) : sortKey === 'artist' ? (r.artist ?? '') : sortKey === 'albumArtist' ? (r.album_artist ?? '') : (r.album ?? '');
    // A number to order by; none (a value the file or the plays do not have) comes first, in the playlist's order.
    const num = (r: PlaylistRow): number | null =>
      sortKey === 'length' ? (r.duration_secs ?? null)
      : sortKey === 'tempo' ? (r.bpm ?? null)
      : sortKey === 'year' ? (r.year ?? null)
      : sortKey === 'track' ? (r.track == null ? null : (r.disc ?? 1) * 10_000 + r.track)
      : sortKey === 'plays' ? (r.play_count ?? 0)
      : sortKey === 'lastPlayed' ? (r.last_played ?? null)
      : (r.added ?? null);
    const byNumber = (x: number | null, y: number | null) => (x === y ? 0 : x === null ? -1 : y === null ? 1 : x - y);
    // Turned round, equals keep the subject's order (an artist's most played: as their All tracks show them).
    const way = sortDown ? -1 : 1;
    if (crate) {
      // As the server ordered them.
    } else if (sortKey === 'own') {
      if (sortDown) list.reverse();
    } else if (['length', 'tempo', 'year', 'track', 'plays', 'lastPlayed', 'added'].includes(sortKey)) list.sort((a, b) => way * byNumber(num(a.r), num(b.r)));
    else list.sort((a, b) => way * ordered(text(a.r), text(b.r)));
    return list;
  });
  const shownRows = $derived(shown ? shown.map((x) => x.r) : []);
  // A crate as records or tracks (0.10; the owner, 2026-10-30: lib/crateRecords): its tracks grouped by album, in the
  // crate's order or an album order, each saying how much of its album the crate holds; a record opened shows its crate
  // tracks, a step of the history. Playing the crate is as ever: its tracks in its order, whichever way it shows.
  const recordsView = $derived(!!crate && !crateEditing && crateViewOf(crate, prefs.crateViews) === 'records');
  const records = $derived(recordsView && shown ? orderRecords(recordsOf(shown), prefs.crateRecordSort.key, prefs.crateRecordSort.down) : []);
  // An album's own count, from the Library's list (Cratarr's own albums: a row of another source has no album id).
  $effect(() => {
    if (recordsView && !library.view) untrack(() => void readLibrary(false));
  });
  // As its rows say (album_tracks), else as the Library's list has the album.
  const albumTotal = (rec: { of: number | null; albumId: string | null }) =>
    rec.of ?? (rec.albumId ? (library.view?.sources.find((s) => s.id === source)?.albums.find((a) => a.id === rec.albumId)?.tracks ?? null) : null);
  let recordOpen = $state<string | null>(null);
  const openRecord = $derived(recordsView && recordOpen ? (records.find((r) => r.key === recordOpen) ?? null) : null);
  const recordShown = $derived(openRecord && shown ? shown.filter((x) => openRecord.at.includes(x.i)) : []);
  function showRecord(key: string) {
    history.pushState({ ...(history.state ?? {}), crateRecord: key }, '');
    recordOpen = key;
  }
  $effect(() => {
    const back = () => (recordOpen = (history.state?.crateRecord as string | undefined) ?? null);
    addEventListener('popstate', back);
    return () => removeEventListener('popstate', back);
  });
  // Another crate, or tracks chosen: no record open.
  $effect(() => {
    void crate;
    void recordsView;
    untrack(() => (recordOpen = null));
  });
  /** A record's menu: its crate tracks as a selection's (lib/SelectionMenu), with Go to album and out of this crate. */
  let recordMenu = $state.raw<CrateRecord | null>(null);
  const recordRows = (rec: CrateRecord) => rec.at.map((i) => rows![i]).filter(Boolean);
  /** In select mode a record ticks its crate tracks, or unticks them where all were. */
  function tickRecord(rec: CrateRecord) {
    if (!selecting) return;
    const t = new Set(selecting);
    const all = rec.at.every((i) => t.has(i));
    for (const i of rec.at) {
      if (all) t.delete(i);
      else t.add(i);
    }
    selecting = t;
  }
  const shownPlayable = $derived(shownRows.filter((r) => r.id && !r.missing));
  const sorted = $derived(sortKey !== 'own' || sortDown);
  /** What shows is not the subject as it is: ordered otherwise, or filtered; it plays as it shows. */
  const reshaped = $derived(sorted || (filterable && filtersOn(kind) > 0));

  // Select mode (Symfonium's): a long-press on a track or the header's icon starts it, a tap ticks a track, and a bar
  // over the list acts on the ticked ones. A tick is a row (its place), so a track that is there twice is ticked once.
  let selecting = $state<Set<number> | null>(null);
  // In select mode a long-press asks what to tick among the tracks shown: all, none or the others (lib/SelectSheet).
  let selectSheet = $state(false);
  function selectWhich(which: 'all' | 'none' | 'others') {
    const all = (shown ?? []).map((x) => x.i);
    selecting = new Set(which === 'all' ? all : which === 'none' ? [] : all.filter((i) => !selecting?.has(i)));
  }
  function startSelect(i?: number) {
    navigator.vibrate?.(8);
    selecting = new Set(i === undefined ? [] : [i]);
  }
  function tick(i: number) {
    if (!selecting) return;
    const s = new Set(selecting);
    if (s.has(i)) s.delete(i);
    else s.add(i);
    selecting = s;
  }
  const ticked = $derived(selecting && shown ? shown.filter((x) => selecting!.has(x.i) && x.r.id && !x.r.missing).map((x) => x.r) : []);
  const tickedFav = $derived(ticked.length > 0 && ticked.every((r) => isFavorite(source, r.id!)));
  $effect(() => {
    // Another subject: nothing ticked; an artist's All tracks most played first again.
    void id;
    topSort = { key: 'plays', down: true };
    selecting = null;
  });
  async function actOnTicked(mode: 'play' | 'next' | 'last') {
    const tracks = tracksOf(ticked);
    if (!tracks.length) return;
    navigator.vibrate?.(8);
    try {
      if (mode === 'play') await playPlaylistFrom(title, source, tracks, 0);
      else await port().enqueue(tracks, mode, source);
      selecting = null;
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
  /** The heart on the ticked tracks: all favourites, or none when all already are. Shown at once, back on a refusal.
   *  Many ask first (lib/many), the selection kept on Cancel. */
  function favouriteTicked() {
    const list = ticked;
    const on = !tickedFav;
    const [src, n] = [source, plural(list.length, 'track')];
    askIfMany(list.length, { title: on ? `Favourite ${n}?` : `Unfavourite ${n}?`, label: on ? `Favourite ${n}` : `Unfavourite ${n}`, icon: 'heart' }, async () => {
      navigator.vibrate?.(8);
      for (const r of list) setFavorite(src, r.id!, on);
      selecting = null;
      const failed = await fewAtATime(list, (r) => setTrackFavorite(src, r.id!, on));
      for (const r of failed) setFavorite(src, r.id!, !on);
      if (failed.length) toast.fail(null, "The favourites didn't all change");
    });
  }
  // An album's tracks carry its id: its cover, and the Incoming page's mark on what plays.
  const tracksOf = (list: PlaylistRow[], src = source): LocalTrack[] =>
    list.map((x) => ({ source: src, id: x.id!, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: album?.id ?? x.album_id ?? null, duration_secs: x.duration_secs, ...(src === BANDCAMP ? { addable: false } : {}) }));

  // Several playlists: each section's rows, asked for when it comes into view; a section folds.
  let sections = $state<Record<string, { rows: PlaylistRow[]; source: string } | { error: string }>>({});
  /** The tempos of what is viewed (decision 0041): the filter's spread. A selection counts its loaded sections. */
  const listTempos = $derived(
    multi ? Object.values(sections).flatMap((s) => ('rows' in s ? s.rows.map((r) => r.bpm) : [])) : (rows?.map((r) => r.bpm) ?? null),
  );
  $effect(() => {
    if (tab !== 'info' && !multi) return;
    // Info's spread, and a selection's stacked one in the header (decision 0041).
    for (const pid of ids) untrack(() => loadSection(pid));
  });
  /** The list's profile (the owner, 2026-10-23: lib/ListProfile), worked out on the server over all its tracks, as
   *  the Filter sheet's list is (lib/listFilter inView); asked again after an edit. Not a selection's. */
  let profile = $state<ListProfileData | null>(null);
  /** While the profile (or a Sounds like crate's curve) is on its way: the header holds its place rather than showing
   *  the tempo spread that would make way for it (the owner, 2026-10-23: no flash of what is not going to stay). */
  let profileComing = $state(false);
  /** The list the profile is of. */
  let profileOf = '';
  $effect(() => {
    const at = inView[kind];
    void saved;
    void undos.n;
    if (!at || multi || crateEditing || creating) {
      profile = null;
      profileComing = false;
      profileOf = '';
      return;
    }
    // An edit asks again with the old profile still shown; another list starts empty and waits. A Sounds like crate's
    // cut moved is the same list (its seed): the profile stays until the new one comes, not the empty place between.
    const seed = likeParts(at.list.kind === 'crate' ? at.list.id : null);
    const key = seed ? `like:${seed.item}` : JSON.stringify([at.list, at.source]);
    if (key !== profileOf) {
      profileOf = key;
      profile = null;
    }
    if (untrack(() => profile) === null) profileComing = true;
    let gone = false;
    filterList(at.list, [], at.source, at.list.kind === 'crate' && !homeCrate(at.list.id) ? hiddenLibraries() : undefined).then(
      (f) => {
        if (gone) return;
        profile = f.profile ?? null;
        profileComing = false;
      },
      () => {
        /* no profile without it */
        if (!gone) profileComing = false;
      },
    );
    return () => {
      gone = true;
    };
  });
  /** A Sounds like crate (`like:<item>@<count>`): its seed and how many, and its curve (lib/LikeCurve), read from its
   *  condition's seeds; moving the cut opens the crate of that many in its place. */
  const like = $derived(likeParts(crate));
  let curve = $state<LikeCurveData | null>(null);
  let curveComing = $state(false);
  /** Where the curve's handle is while dragged: the count and the profile follow it (the owner, 2026-10-23), from the
   *  profiles sent with the curve, so a drag asks nothing of the server; the tracks follow on release. */
  let preview = $state<number | null>(null);
  const previewProfile = $derived.by(() => {
    if (preview === null || !curve) return null;
    let best: LikeCurveData['profiles'][number] | null = null;
    for (const p of curve.profiles) if (p.count <= preview && (!best || p.count > best.count)) best = p;
    return best?.profile ?? null;
  });
  /** The profile shown: the preview's or the list's, while music profiles are on (Settings › Display › Content). */
  // Beside the similarity curve always, whatever Settings says of music profiles (the owner, 2026-10-25): what the
  // curve's cut takes in is read off it.
  const shownProfile = $derived(prefs.profile || !!(like && curve?.alike.length) ? (previewProfile ?? profile) : null);
  /** The seed the curve is of: moving the cut (another crate of the same seed) keeps it shown until the new one comes. */
  let curveOf = '';
  $effect(() => {
    const c = crate;
    const seed = like?.item ?? '';
    if (seed !== curveOf) {
      curveOf = seed;
      curve = null;
    }
    curveComing = false;
    if (!c || !like) return;
    // Read untracked: its own answer must not ask again.
    if (!untrack(() => curve)) curveComing = true;
    let gone = false;
    getCrate(c)
      .then((d) => {
        nameLike(c, d.name);
        const v = d.conditions.find((x) => x.value.kind === 'like')?.value;
        return v && v.kind === 'like' ? likeCurve(v.of, prefs.source || undefined, hiddenLibraries(), v.fresh, v.weights) : null;
      })
      .then(
        (k) => {
          if (gone) return;
          curve = k;
          curveComing = false;
        },
        () => {
          /* no curve without it */
          if (!gone) curveComing = false;
        },
      );
    return () => {
      gone = true;
    };
  });
  let folded = $state<Record<string, boolean>>({});
  // A playlist left out of the selection drops its section; the others keep theirs.
  const sectionsAsked = new Set<string>();
  $effect(() => {
    const keep = new Set(ids);
    untrack(() => {
      for (const k of Object.keys(sections)) if (!keep.has(k)) delete sections[k];
      for (const k of [...sectionsAsked]) if (!keep.has(k)) sectionsAsked.delete(k);
    });
  });
  function loadSection(sid: string) {
    if (sectionsAsked.has(sid)) return;
    sectionsAsked.add(sid);
    playlistRows(sid, prefs.source || undefined).then(
      (r) => {
        sections[sid] = { rows: r.rows, source: r.source };
        loadFavorites(r.source, r.rows.flatMap((x) => (x.id ? [x.id] : [])));
      },
      (e) => (sections[sid] = { error: e instanceof Error ? e.message : 'Unexpected error' }),
    );
  }
  /** Asks for a section's rows once it is near the screen. */
  function seen(el: HTMLElement, sid: string) {
    const o = new IntersectionObserver(([e]) => e.isIntersecting && loadSection(sid), { rootMargin: '400px' });
    o.observe(el);
    return { destroy: () => o.disconnect() };
  }

  // Related: what Insights knows of it; its genres go in the header, as Finamp's genre line.
  $effect(() => insights.load());
  const at = $derived(insights.indexOf(id));
  const point = $derived(at >= 0 ? insights.shown!.playlists[at] : null);

  $effect(() => void loadPlaylistFavorites());

  // Viewing it again scrolls it to the top.
  let scroller = $state<HTMLElement>();
  $effect(() => {
    void panel.again;
    scroller?.scrollTo({ top: 0, behavior: 'smooth' });
  });

  let menuOpen = $state(false);
  /** The crate's menu (its header's ⋮, decision 0058). */
  let crateMenuOpen = $state(false);
  /** A badge's menu (decision 0058: a genre badge, a related playlist: what its tap opens, by its subject). */
  let badgeMenu = $state<string | null>(null);
  /** The ticked tracks' menu (the select bar's ⋮, lib/SelectionMenu): their rows, for its Remove from and Exclude from. */
  let tickedMenu = $state.raw<{ rows: PlaylistRow[]; tracks: LocalTrack[] } | null>(null);
  /** The menu opens at its entries, or at Delete's confirmation (the edit bar's trash). */
  let menuStart = $state<'menu' | 'delete'>('menu');
  /** `outs`: its list's Remove from (one's own playlist) or Exclude from (a crate: the track, its album). */
  let trackMenu = $state<{ track: MenuTrack; start: 'menu' | 'lists'; outs?: { label: string; icon: IconName; run: () => void }[] } | null>(null);
  const asTrack = (r: PlaylistRow, src = source): MenuTrack => ({ source: src, id: r.id!, name: r.name ?? r.file, artist: r.artist, album: r.album, album_id: album?.id ?? r.album_id ?? null });

  // The action bar's playback actions, icons only as Symfonium's: a new queue (Play, Shuffle), or the whole of it next
  // or at the end of the queue; an album also Resume. A crate plays and shuffles; its favourites toggle sits at the
  // bar's end (decision 0024, amended): its icon and label say what the crate shows now.
  type BarAction = { key: string; icon: IconName; label: string; run: () => void; disabled: boolean };
  const canPlay = $derived(multi ? chosenCount > 0 : playable.length > 0);
  const barActions = $derived.by((): BarAction[] => {
    // An artist from Discover: nothing of theirs to play from here (their albums play on their own panels).
    if (artist && fromDiscover(artist.source)) return [];
    const play = (key: PlayMode, icon: IconName, label: string): BarAction => ({ key, icon, label, run: () => act(key), disabled: !canPlay });
    const list = [play('play', 'play', 'Play')];
    if (album && !offServer) list.push({ key: 'resume', icon: 'resume', label: 'Resume', run: resumeAlbum, disabled: !resumeAt });
    list.push(play('shuffle', 'shuffle', 'Shuffle'));
    // Play next and Add to queue: an album's and an artist's, not a playlist's or a crate's (the owner, 2026-10-23).
    if (album || artist) list.push(play('next', 'playNext', 'Play next'), play('last', 'listAdd', 'Add to queue'));
    // An artist as the radio's input (decision 0036), as a crate is: their tracks, and like them from anywhere.
    if (artist && port().radio && source === 'cratarr') list.push({ key: 'radio', icon: 'radio', label: 'Start radio', run: artistRadio, disabled: !canPlay });
    // A crate as the radio's input (decision 0036): like it, from anywhere; or only its tracks, endlessly.
    else if (crate && port().radio) {
      list.push({ key: 'radio', icon: 'radio', label: 'Start radio', run: () => crateRadio(false), disabled: !canPlay });
      list.push({ key: 'station', icon: 'radioTower', label: 'Play as radio', run: () => crateRadio(true), disabled: !canPlay });
    }
    return list;
  });
  // A genre shows all its tracks: no favourites toggles.
  // Not before the crates are known: it showed No favourites first.
  const favState = $derived(crate && !seeded(crate) && crates.list ? FAVOURITES[favouritesOf(crate)] : null);
  // Beside it, its favourite albums toggle (decision 0024, amended 2026-10-19).
  const albState = $derived(crate && !seeded(crate) ? FAVOURITE_ALBUMS[favouriteAlbumsOf(crate)] : null);
  const closeLabel = $derived(
    crate ? 'Close the crate' : album ? 'Close the album' : artist ? 'Close the artist' : multi ? 'Close the playlists' : 'Close the playlist',
  );

  // Under the cover, as Symfonium's: the name large, then one line of what it is (an album: its length, year, format
  // and bitrate; a playlist or a crate: how many tracks, and the length), and the genres.
  const details = $derived.by(() => {
    if (crateEditing) return crateTotal === null ? '' : plural(crateTotal, 'track');
    if (crate && preview !== null) return plural(preview, 'track');
    if (crate) return rows ? plural(total, 'track') : '';
    if (multi) return playlistsKnown() ? plural(chosenCount, 'track') : '';
    if (!rows) return '';
    // An artist from Discover: how many studio albums they have made (the owner, 2026-10-25: "the golden standard"); no
    // tracks of a server's.
    if (artist && fromDiscover(artist.source)) return artistHead?.studio ? plural(artistHead.studio, 'studio album') : '';
    if (artist) {
      const albums = artist.tracks ? null : plural(artistHead?.albums.length ?? 0, 'album');
      return [albums, plural(rows.length, 'track'), length ? clockText(length) : null].filter(Boolean).join(' • ');
    }
    const h = album ? albumHead : null;
    const parts = album
      ? [length ? clockText(length) : null, h?.year, h?.format, h?.kbps ? `${h.kbps} kb/s` : null]
      : [plural(p?.count ?? rows.length, 'track'), length ? clockText(length) : null];
    return parts.filter((x) => x !== null && x !== undefined && x !== '').join(' • ');
  });
  // Not while a crate is changed: its genres are the editor's to choose.
  const shownArtists = $derived(!album && !artist && !multi ? ((previewProfile ?? profile)?.artists ?? []) : []);
  const shownGenres = $derived(multi || crateEditing ? [] : plain ? (listGenres ?? []) : (point?.genres ?? []));

  // As Symfonium's album screen: two floating pills over the cover (back and ✕; the filter and where it plays), and the
  // action bar at the cover's foot. Scrolled up to the top, the action bar stays there, and the pills float just under
  // it (chosen 2026-10-15), over the list that scrolls under them.
  let actbar = $state<HTMLElement>();
  let stuck = $state(false);
  function onScroll() {
    const sc = scroller;
    const a = actbar;
    if (!sc || !a) return;
    // Its distance from the top: where it docks while it stays there (a sticky element's offset is where it shows);
    // not sticky (Display), never.
    stuck = prefs.stickyActions && a.offsetTop - sc.scrollTop <= (parseFloat(getComputedStyle(a).top) || 0) + 0.5;
  }
  $effect(() => {
    // Another subject shows from its cover.
    void id;
    stuck = false;
  });
  // How it is left (ui-conventions, Panels; the owner, 2026-10-24): full screen (a phone; a window without room beside
  // the page), Back always, closing it where there is no view before this one, and Home beside it, as the navigation
  // under it is out of reach; beside the page, ✕, and Back where there is a view before this one. Every mode and view
  // of the panel the same, in the same place.
  let { full = false }: { full?: boolean } = $props();
  /** The page under the panel (its address), whose search the panel carries while it covers it (a phone). */
  const pageHere = location.hash.split('?')[0].replace(/^#\/?/, '') || 'playlists';
  let canGoBack = $state(false);
  $effect(() => {
    void panel.viewed;
    void panel.tab;
    const on = () => (canGoBack = panelDepth() > 1);
    on();
    addEventListener('popstate', on);
    return () => removeEventListener('popstate', on);
  });

  // Resume (Symfonium's clock): where an album of Cratarr's own library was left, from the bookmarks apps keep
  // (decision 0030). Other sources' albums have none to offer.
  let resumeAt = $state<{ track: string; position: number } | null>(null);
  $effect(() => {
    const a = album;
    void undos.n;
    resumeAt = null;
    if (!a || !a.id.startsWith('al-')) return;
    albumResume(a.id).then(
      (r) => {
        if (album?.id === a.id && r.track) resumeAt = { track: r.track, position: r.position_secs ?? 0 };
      },
      () => {},
    );
  });
  // An album's menu (its ⋮ in the action bar).
  let albumMenuOpen = $state(false);
  let bandcampMenuOpen = $state(false);
  let artistMenuOpen = $state(false);
  async function resumeAlbum() {
    const at = resumeAt;
    const i = at ? playable.findIndex((r) => r.id === at.track) : -1;
    if (!at || i < 0) return;
    navigator.vibrate?.(8);
    try {
      await playPlaylistFrom(title, source, tracksOf(playable), i, at.position);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
  // A crate's next page, when the end of the list comes into view.
  let more = $state<HTMLElement>();
  let loadingMore = false;
  $effect(() => {
    const el = more;
    const c = crate;
    if (!el || !c) return;
    const seen = new IntersectionObserver(async ([e]) => {
      if (!e.isIntersecting || loadingMore || !rows || rows.length >= total) return;
      loadingMore = true;
      try {
        const r = await crateRows(c, rows.length, PAGE, prefs.source || undefined, favouritesOf(c), favouriteAlbumsOf(c), crateLook);
        if (crate === c && rows) {
          rows = [...rows, ...r.rows];
          loadFavorites(r.source, r.rows.flatMap((x) => (x.id ? [x.id] : [])));
        }
      } catch (err) {
        toast.fail(err, 'More tracks could not be loaded');
      } finally {
        loadingMore = false;
      }
    });
    seen.observe(el);
    return () => seen.disconnect();
  });

  /** A Play or a Shuffle under way: its button the ring until the music can be heard (the owner, 2026-10-26), then the
   *  player changes all at once. */
  let starting = $state<PlayMode | null>(null);
  async function act(mode: PlayMode) {
    if (starting) return;
    navigator.vibrate?.(8);
    const plays = mode === 'play' || mode === 'shuffle';
    if (plays) starting = mode;
    try {
      const heard = plays ? untilAudible() : null;
      if (plain || (sorted && !multi)) await actOnRows(mode);
      else await startPlaylists([...ids], mode);
      if (mode === 'next' || mode === 'last') toast.show({ text: mode === 'next' ? 'Plays next.' : 'Added to the queue.', tone: 'done' });
      await heard;
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    } finally {
      starting = null;
    }
  }

  /** The crate as the radio's input (decision 0036). Start radio: tracks like the crate's, from the whole library (up
   *  to 300 of its tracks as the seed). Play as radio: only the crate's tracks, as its favourites toggle is set, never
   *  ending and never in the same order; Cratarr reads the crate itself on each refill. */
  async function artistRadio() {
    navigator.vibrate?.(8);
    const a = artist;
    try {
      // The seed: 300 of all their tracks, the server's fair draw (decision 0059), not the rows shown.
      const ids = a ? (await pickOf({ kind: 'artist', source: a.source, name: a.name }, 'shuffle', 'cratarr')).tracks.slice(0, 300).map((t) => t.id) : playable.map((r) => r.id!);
      await startRadio(ids, title);
    } catch (e) {
      toast.fail(e, "The radio didn't start");
    }
  }

  // The artist's heart: shows the change at once, back as it was when the server refuses.
  async function toggleArtistFavorite() {
    const h = artistHead;
    if (!artist || !h || h.favorite === undefined) return;
    const on = !h.favorite;
    h.favorite = on;
    try {
      await setArtistHeart(artist.source, artist.name, on);
    } catch (e) {
      h.favorite = !on;
      toast.fail(e, "The favourite didn't change");
    }
  }
  // Top recommendations (the owner, 2026-10-23, in place of their most played): the artist's tracks that fit one's
  // taste best, the server's; on another server, or until they come, none. › shows all their tracks.
  let picks = $state<{ name: string; ids: string[] } | null>(null);
  $effect(() => {
    const a = artist;
    picks = null;
    if (!a || a.tracks || a.source !== 'cratarr') return;
    artistPicks(a.name, hiddenLibraries()).then(
      (r) => {
        if (artist?.name === a.name) picks = { name: a.name, ids: r.ids };
      },
      () => {},
    );
  });
  const topRows = $derived(
    (artist && !artist.tracks && rows && picks?.name === artist.name ? picks.ids.flatMap((x) => playable.find((r) => r.id === x) ?? []) : []).filter(
      (r) => !own || matchesSearch(r.name ?? r.file, r.artist, r.album),
    ),
  );
  // Seen through a search, on every device (the owner, 2026-10-25: beside the page and over it they differed): the
  // cover and the action bar go; the name and its lines, the About stay; the tracks and the albums narrow to the
  // words; what leads off the panel (similar ones, More by, suggestions, related artists) goes.
  const own = $derived(words(pageView.query).length > 0);

  async function crateRadio(inside: boolean) {
    const c = crate;
    if (!c) return;
    navigator.vibrate?.(8);
    try {
      if (inside) return await startRadio([], title, c);
      await crateRadioOf(c, title);
    } catch (e) {
      toast.fail(e, "The radio didn't start");
    }
  }

  /** An album's actions (a sorted playlist's too) work on the tracks it has shown, in the order shown; a crate's on the
   *  whole crate, not the pages shown so far (crates.svelte.ts: its shuffle from all of it; 500 at most). */
  async function actOnRows(mode: PlayMode) {
    const c = crate;
    if (c && !crateEditing) return playCollection({ kind: 'crate', id: c }, mode, title, undefined, crateLook);
    // The server's library (decision 0059): an album or an artist whole, in its order; a sorted view what shows.
    if (!offServer) {
      if (album && !reshaped) return playCollection({ kind: 'album', source: album.source, id: album.id }, mode, title);
      if (artist && !reshaped) return playCollection({ kind: 'artist', source: artist.source, name: artist.name }, mode, title);
      return playCollection({ kind: 'tracks', source, ids: shownPlayable.map((r) => r.id!) }, mode, title);
    }
    const tracks = tracksOf(shownPlayable);
    if (mode === 'play' || mode === 'shuffle') {
      const order = mode === 'shuffle' ? shuffled(tracks) : tracks;
      return playPlaylistFrom(title, source, order, 0);
    }
    return port().enqueue(tracks, mode, source);
  }

  // Make playlist (the save icon): the crate's tracks now as a new playlist, which then shows here. Its name in progress
  // outlives this view (panelWork: going over the page or back beside it builds the view again).
  type Making = { name: string; saving: boolean };
  let making = $state<Making | null>(work.making && work.making.for === untrack(() => panel.viewed) ? (work.making.state as Making) : null);
  $effect(() => {
    const m = making;
    work.making = m ? { for: untrack(() => panel.viewed) ?? '', state: m } : null;
  });
  async function makePlaylist() {
    if (!making || !crate || !making.name.trim()) return;
    making.saving = true;
    const name = making.name.trim();
    try {
      await favouritesSaved();
      const { added, undo } = await crateToPlaylist(crate, name, prefs.source || undefined);
      withUndo(`Made ${name}: ${added} ${added === 1 ? 'track' : 'tracks'}.`, [undo], {
        after: (undone) => (undone ? followDelete(name) : followCreate(name)),
      });
      making = null;
      refresh();
      loadCrates();
      viewPlaylist(name);
    } catch (e) {
      toast.fail(e, "The playlist wasn't made");
      if (making) making.saving = false;
    }
  }

  /** A tap on a row: its playlist (`name`, these rows) plays from it. */
  async function playFrom(name: string, src: string, list: PlaylistRow[], r: PlaylistRow) {
    if (!r.id || r.missing) return;
    navigator.vibrate?.(8);
    const ok = list.filter((x) => x.id && !x.missing);
    try {
      // The device's own music: its rules here (decision 0059). The server's library: the server's choice from the
      // tapped track (500 at most): the whole crate, album, artist or playlist in its order; a sorted view or several
      // lists, what shows, in the order it shows (a queue's order is what is seen).
      if (offServer) return await playPlaylistFrom(name, src, tracksOf(ok, src), ok.indexOf(r));
      const c = crate;
      if (c && !crateEditing) return await playCollection({ kind: 'crate', id: c }, 'play', name, r.id, crateLook);
      if (album && !reshaped) return await playCollection({ kind: 'album', source: album.source, id: album.id }, 'play', name, r.id);
      if (artist && !reshaped) return await playCollection({ kind: 'artist', source: artist.source, name: artist.name }, 'play', name, r.id);
      if (p && !multi && !reshaped) return await playCollection({ kind: 'playlist', id: p.id }, 'play', name, r.id);
      await playCollection({ kind: 'tracks', source: src, ids: ok.map((x) => x.id!) }, 'play', name, r.id);
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }

  // A swipe right plays the track next (it was Finamp's swipe left until the owner gave left to Remove, 2026-10-19); the
  // row springs back.
  function playNext(r: PlaylistRow, src = source) {
    (async () => {
      try {
        await port().enqueueOne(tracksOf([r], src)[0], 'next');
        toast.show({ text: 'Plays next.', tone: 'done' });
        setTimeout(refresh, 700);
      } catch (e) {
        toast.fail(e, "It couldn't be put next");
      }
    })();
    return 'back' as const;
  }

  // Swipes on a list's rows (the owner, 2026-10-19: "on any library content list in the panel, swiping to the left should
  // remove it from that list"): right plays the track next on every list; left excludes it from the crate (decision
  // 0024 amended: "exclude, not remove") or takes it out of the playlist file (one's own), at once, with Undo. An album's or an artist's
  // tracks are no list of one's own: no left. What each does shows in the strip the row uncovers (lib/SwipeBehind).
  const PLAY_NEXT = { icon: 'playNext', label: 'Play next' } as const;
  const OUT_OF_CRATE = { icon: 'crateOut', label: 'Exclude', danger: true } as const;
  const OUT_OF_PLAYLIST = { icon: 'listRemove', label: 'Remove', danger: true } as const;
  /** A swipe left on a row of `list` (a playlist's id; the crate viewed): null where it takes nothing out. */
  function outOf(r: PlaylistRow, list: string): (() => 'away') | null {
    const c = crate;
    if (c) return r.id && !r.missing ? () => outOfCrate(c, [r]) : null;
    if (album || artist) return null;
    const pl = shownPlaylists().find((x) => x.id === list);
    if (!pl || pl.mine === false || (multi ? !(list in sections) : list !== id)) return null;
    return () => outOfPlaylist(list, [r]);
  }
  /** How many tracks and albums the crate viewed excludes, and their sheet. */
  // A playlist's suggestions crate (lib/SuggestedTracks) is not among one's crates: its count of what was turned down
  // for the playlist is read on its own.
  let suggestionsLeftOut = $state(0);
  $effect(() => {
    const c = crate;
    void saved;
    suggestionsLeftOut = 0;
    if (!c || !likeParts(c)?.item.startsWith('list:')) return;
    crateLeftOut(c).then(
      (r) => {
        if (c === crate) suggestionsLeftOut = r.rows.length + (r.albums?.length ?? 0);
      },
      () => {},
    );
  });
  /** The playlist whose suggestions this crate is (`like:list:<id>`, lib/SuggestedTracks): its rows add to it or are
   *  turned down for it. */
  const suggestsFor = $derived.by(() => {
    const item = crate ? likeParts(crate)?.item : null;
    if (!item?.startsWith('list:')) return null;
    const id = item.slice('list:'.length);
    return shownPlaylists().find((x) => x.id === id && x.mine !== false) ?? null;
  });
  async function suggestion(r: PlaylistRow, src: string, add: boolean) {
    const s = suggestsFor;
    if (!s || !crate) return;
    try {
      if (add) {
        const done = await appendTracks(s.id, src, [r.id!]);
        withUndo(`Added to ${s.name}.`, [done.undo], { after: () => saved++ });
      } else {
        const done = await leaveOutOfCrate(crate, src, { ids: [r.id!] });
        withUndo(`Not suggested for ${s.name} again.`, [done.undo], { after: () => saved++ });
      }
      saved++;
    } catch (e) {
      toast.fail(e, add ? "The track wasn't added" : "The suggestion wasn't turned down");
    }
  }
  const leftOut = $derived(crate && !crateEditing ? (crates.list?.find((x) => x.id === crate)?.left_out ?? suggestionsLeftOut) : 0);
  let leftOutOpen = $state(false);
  /** A row's menu: Remove from one's own playlist, as its swipe left does. A crate's Excludes are its heart's (one
   *  thing in one place, as the full player's: the owner, "Heart sheet only"). */
  function menuOuts(out: (() => 'away') | null, list: string) {
    return out && !crate ? [{ label: `Remove from ${list}`, icon: 'listRemove' as IconName, run: () => void out() }] : [];
  }
  /** A row's heart's sheet (lib/HeartSheet) in a crate: Exclude the track (as its swipe left), or its album. */
  function crateExcludes(out: (() => 'away') | null, r: PlaylistRow): ExcludeEntry[] {
    const c = crate;
    if (!c || !out) return [];
    const name = crateName(c);
    return [
      { label: `Exclude from ${name}`, icon: 'crateOut', run: () => void out() },
      ...(r.album_id ? [{ label: `Exclude album from ${name}`, icon: 'crateOut' as const, run: () => albumOutOfCrate(c, [r]) }] : []),
    ];
  }
  /** The ticked rows' Remove from and Exclude from, in the select bar's ⋮ (the owner, 2026-10-27: what one has, several
   *  have): where each row's own menu has them, all at once. Many ask first (lib/many). */
  function tickedOuts(rs: PlaylistRow[]): { label: string; icon: IconName; run: () => void }[] {
    if (!rs.length || !rs.every((r) => outOf(r, title))) return [];
    const [c, list, n] = [crate, title, plural(rs.length, 'track')];
    const asked = (what: string, icon: IconName, run: () => void) => () => askIfMany(rs.length, { title: `${what}?`, label: what, icon }, run);
    if (!c) return [{ label: `Remove from ${list}`, icon: 'listRemove', run: asked(`Remove ${n} from ${list}`, 'listRemove', () => void outOfPlaylist(list, rs)) }];
    const name = crateName(c);
    const albums = new Set(rs.flatMap((r) => (r.album_id ? [r.album_id] : []))).size;
    return [
      { label: `Exclude from ${name}`, icon: 'crateOut', run: asked(`Exclude ${n} from ${name}`, 'crateOut', () => void outOfCrate(c, rs)) },
      ...(albums
        ? [{ label: albums === 1 ? `Exclude album from ${name}` : `Exclude albums from ${name}`, icon: 'crateOut' as const, run: () => albumOutOfCrate(c, rs) }]
        : []),
    ];
  }
  /** The row leaves the list as it slides out (the list is keyed by place, so the next takes its row). */
  function dropShown(list: string, r: PlaylistRow) {
    setTimeout(() => {
      const s = sections[list];
      if (multi && s && 'rows' in s) sections[list] = { ...s, rows: s.rows.filter((x) => x !== r) };
      else if (rows) rows = rows.filter((x) => x !== r);
    }, 200);
  }
  /** Rows out of the crate (a swipe's one, or the ticked ones: one call, one Undo). */
  function outOfCrate(c: string, rs: PlaylistRow[]) {
    for (const r of rs) dropShown(id, r);
    leaveOut(c, source, { ids: rs.map((r) => r.id!) }).catch((e) => {
      toast.fail(e, "It wasn't excluded");
      saved++;
    });
    return 'away' as const;
  }
  /** The rows' whole albums out of the crate (the owner: "exclude album from crate"): their rows leave the list. */
  function albumOutOfCrate(c: string, rs: PlaylistRow[]) {
    const albums = [...new Set(rs.flatMap((r) => (r.album_id ? [r.album_id] : [])))];
    if (!albums.length) return;
    for (const x of rows?.filter((x) => !!x.album_id && albums.includes(x.album_id)) ?? []) dropShown(id, x);
    leaveOut(c, source, { albums }).catch((e) => {
      toast.fail(e, "It wasn't excluded");
      saved++;
    });
  }
  // A playlist's line out of its file. Swipes in a row wait for each other: each save names the lines as the one before
  // left them (the server refuses a save over lines that changed), and the list is read again once they are all done.
  const leaving: Record<string, PlaylistRow[]> = {};
  let outs = 0;
  let outChain: Promise<unknown> = Promise.resolve();
  /** A swipe's row, or the ticked ones (one save, one Undo). */
  function outOfPlaylist(list: string, rs: PlaylistRow[]) {
    const s = sections[list];
    const before = leaving[list] ?? (multi ? (s && 'rows' in s ? s.rows : null) : rows);
    if (!before) return 'away' as const;
    const after = before.filter((x) => !rs.includes(x));
    leaving[list] = after;
    outs++;
    for (const r of rs) dropShown(list, r);
    outChain = outChain
      .then(async () => {
        const { undo } = await editPlaylist(
          list,
          before.map((x) => x.entry),
          after.map((x) => x.entry),
        );
        withUndo(`Removed from ${list}.`, [undo]);
      })
      .catch((e) => toast.fail(e, "It wasn't taken out"))
      .finally(() => {
        if (--outs) return;
        for (const k of Object.keys(leaving)) delete leaving[k];
        saved++;
        refresh();
      });
    return 'away' as const;
  }

  // Edit mode: a working copy of the rows (each with its place in the file, so a restored one goes back there), and of
  // the playlist's name: its title is editable where it is, looking the same (a tap puts the caret in it; Enter or a
  // tap elsewhere ends it), and Save renames it with the rest.
  // The edit under way outlives this view (panelWork), as Make playlist's name does.
  type Item = { r: PlaylistRow; at: number };
  type Editing = { rows: Item[]; removed: Item[]; name: string; saving: boolean };
  let editing = $state<Editing | null>(work.editing && work.editing.for === untrack(() => panel.viewed) ? (work.editing.state as Editing) : null);
  $effect(() => {
    const e = editing;
    work.editing = e ? { for: untrack(() => panel.viewed) ?? '', state: e } : null;
  });
  const editingFor = $derived(editing ? id : null);
  $effect(() => {
    // Another playlist viewed (or this one closed): the edit is dropped.
    if (editingFor !== null && editingFor !== id) editing = null;
  });
  // New playlist: the same edit mode, empty (kept like any edit until Save or Cancel).
  $effect(() => {
    if (creating && !untrack(() => editing)) editing = { rows: [], removed: [], name: '', saving: false };
  });
  const tracksChanged = $derived(!!editing && (editing.removed.length > 0 || editing.rows.some((x, i) => x.at !== i)));
  const newName = $derived(editing ? editing.name.trim() : '');
  const nameChanged = $derived(!!editing && newName !== '' && newName !== id && !creating);
  // A crate's edit mode: the draft (kept, else the crate as saved, else empty; above), its size as the editor works it
  // out.
  let crateTotal = $state<number | null>(null);
  let crateSaving = $state(false);
  /** Delete: a second tap on the trash confirms (Undo follows). */
  let crateDeleteArmed = $state(false);
  const forgetPicked = () => {
    if (crateImage && crateImage !== 'remove') URL.revokeObjectURL(crateImage.url);
    crateImage = null;
  };
  function pickCrateImage(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    forgetPicked();
    crateImage = { file, url: URL.createObjectURL(file) };
  }
  function removeCrateImage() {
    const own = !!crates.list?.find((c) => c.id === builder?.id)?.image;
    forgetPicked();
    if (own) crateImage = 'remove';
  }
  $effect(() => {
    const b = builder;
    void panel.again;
    crateDraft = null;
    crateTotal = null;
    crateDeleteArmed = false;
    untrack(forgetPicked);
    if (!b) return;
    const kept = keptDraft(b.id);
    if (kept) return void (crateDraft = kept);
    if (!b.id) return void (crateDraft = { name: '', conditions: [] });
    const which = b.id;
    getCrate(which).then(
      (c) => builder?.id === which && (crateDraft = { name: c.name, conditions: c.conditions }),
      (e) => (error = e instanceof Error ? e.message : 'Unexpected error'),
    );
  });
  $effect(() => {
    const b = builder;
    const d = crateDraft;
    if (b && d) keepDraft(b.id, JSON.parse(JSON.stringify(d)));
  });
  const permanentNames = $derived((crates.list ?? []).filter((c) => c.permanent).map((c) => c.name.toLowerCase()));

  /** After a crate's undo or redo: the list follows, and a crate that is gone no longer shows in the panel. */
  async function followCrate(c: string) {
    await loadCrates();
    if (panel.viewed === CRATE + c && !crates.list?.some((x) => x.id === c)) closePlaylist();
  }
  async function saveCrate() {
    const b = builder;
    const d = crateDraft;
    if (!b || !d || !d.name.trim() || crateSaving) return;
    crateSaving = true;
    const name = d.name.trim();
    try {
      const saved = b.id ? await updateCrate(b.id, name, d.conditions) : await createCrate(name, d.conditions);
      withUndo(b.id ? `Saved ${name}.` : `Created ${name}.`, [saved.undo], { after: () => followCrate(saved.id) });
      const img = crateImage;
      if (img) await setCrateArt(saved.id, img === 'remove' ? null : img.file).catch((e) => toast.fail(e, "The crate's picture wasn't saved"));
      forgetDraft(b.id);
      await loadCrates();
      viewPlaylist(CRATE + saved.id);
    } catch (e) {
      toast.fail(e, "The crate wasn't saved");
    } finally {
      crateSaving = false;
    }
  }
  function cancelCrate() {
    const b = builder;
    if (!b) return;
    forgetDraft(b.id);
    if (b.id) viewPlaylist(CRATE + b.id);
    else closePlaylist();
  }
  async function deleteOwnCrate() {
    const c = builder?.id;
    if (!c) return;
    if (!crateDeleteArmed) return void (crateDeleteArmed = true);
    const name = crateDraft?.name.trim() || crateName(c);
    try {
      const { undo } = await deleteCrate(c);
      withUndo(`Deleted ${name}.`, [undo], { after: () => followCrate(c) });
      forgetDraft(c);
      await loadCrates();
      closePlaylist();
    } catch (e) {
      toast.fail(e, "The crate wasn't deleted");
    }
  }

  // The pencil beside the name in edit mode: the caret goes to the name's end, as a tap on the name puts it there.
  let nameEl = $state<HTMLElement>();
  function editName() {
    const el = nameEl;
    if (!el) return;
    el.focus();
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    const sel = getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);
  }
  // A new playlist or crate: the caret in its name at once, as a tap on the name puts it there.
  let named = '';
  $effect(() => {
    const key = creating ? 'playlist' : builder && !builder.id ? 'crate' : '';
    const el = nameEl;
    if (key === named || (key && !el)) return;
    named = key;
    if (key) editName();
  });
  function startEdit() {
    if (!rows) return;
    navigator.vibrate?.(8);
    editing = { rows: rows.map((r, at) => ({ r, at })), removed: [], name: id, saving: false };
  }
  const reorder = new Reorder(
    () => scroller,
    () => editing?.rows.length ?? 0,
    (from, to) => {
      if (!editing) return;
      const list = [...editing.rows];
      const [x] = list.splice(from, 1);
      list.splice(to, 0, x);
      editing.rows = list;
    },
  );
  function removeAt(i: number) {
    if (!editing) return;
    const [x] = editing.rows.splice(i, 1);
    editing.removed = [...editing.removed, x];
  }
  function restore(x: Item) {
    if (!editing) return;
    editing.removed = editing.removed.filter((y) => y !== x);
    const before = editing.rows.findIndex((y) => y.at > x.at);
    editing.rows.splice(before < 0 ? editing.rows.length : before, 0, x);
  }
  const removeSwiped = (i: number) => {
    setTimeout(() => removeAt(i), 200);
    return 'away' as const;
  };
  // New playlist's Save: made under its name (Undo deletes it again), and the panel shows it.
  async function create() {
    const name = newName;
    if (!editing || !name || editing.saving) return;
    editing.saving = true;
    try {
      const { undo } = await createPlaylist(name);
      withUndo(`Created ${name}.`, [undo], { after: (undone) => (undone ? followDelete(name) : followCreate(name)) });
      editing = null;
      await refresh();
      viewPlaylist(name);
    } catch (e) {
      toast.fail(e, "The playlist wasn't made");
      if (editing) editing.saving = false;
    }
  }
  async function save() {
    if (creating) return create();
    if (!editing || !rows) return;
    if (!tracksChanged && !nameChanged) return void (editing = null);
    editing.saving = true;
    // Fixed now: once renamed, the panel follows the new name, and what changed would no longer show.
    const [from, to, renamed] = [id, newName, nameChanged];
    const tokens: (string | null)[] = [];
    try {
      if (tracksChanged)
        tokens.push(
          (
            await editPlaylist(
              id,
              rows.map((r) => r.entry),
              editing.rows.map((x) => x.r.entry),
            )
          ).undo,
        );
      // Last: the writes above still name it as it was. The panel's view follows the new name.
      if (renamed) tokens.push(await renameEverywhere(from, to));
      // One Undo for both (the rename undone first, then the tracks under the old name).
      withUndo(`Saved ${to}.`, tokens, {
        after: (undone) => renamed && (undone ? followRename(to, from) : followRename(from, to)),
      });
      editing = null;
      saved++;
      refresh();
    } catch (e) {
      toast.fail(e, "The playlist wasn't saved");
      if (editing) editing.saving = false;
    }
  }

  // Esc closes it (when no sheet of its own is open; in edit mode Cancel is the way out).
  $effect(() => {
    const esc = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || menuOpen || crateMenuOpen || trackMenu || editing || crateEditing || making || sortOpen || albumMenuOpen || bandcampMenuOpen || artistMenuOpen) return;
      if (selecting) selecting = null;
      else if (other) backToTracks();
      else closePlaylist();
    };
    addEventListener('keydown', esc);
    return () => removeEventListener('keydown', esc);
  });
  // On a phone this panel's glass is App's (lib/glass, the owner 2026-10-25: one glass, page's and panel's): its pills
  // handed over while it shows its tracks (making a playlist and the playlists' other views keep their own bar).
  const handed = $derived(glass.one && full && !making && !other);
  const pill = () =>
    !editing && !crateEditing
      ? {
          view: true,
          order: listOrder,
          device: !playing(),
          filter: filterable,
          tempos: listTempos,
          kind,
          counted: rows && shown ? { shown: shown.length, total: rows.length, pending: pass.pending } : null,
          search: full ? pageHere : null,
        }
      : null;
  handGlass(() => handed, { left: glassLeft, pill, extra: glassExtra, below: () => stuck && !own });
</script>

<!-- This panel's glass: its floating pills, or App's row on a phone (lib/glass). -->
{#snippet glassLeft()}<span class="navpill">{@render leave()}</span>{/snippet}
{#snippet glassExtra()}<PanelPlace />{/snippet}

{#snippet leave()}
  {#if full}
    <button aria-label="Back" title="Back" onclick={() => (canGoBack ? history.back() : closePlaylist())}><Icon name="back" size={22} /></button>
    <button aria-label="Home" title="Home" onclick={goHome}><Icon name="home" size={22} /></button>
  {:else}
    {#if canGoBack}<button aria-label="Back" title="Back" onclick={() => history.back()}><Icon name="back" size={22} /></button>{/if}
    <button aria-label={closeLabel} title="Close" onclick={() => closePlaylist()}><Icon name="close" size={22} /></button>
  {/if}
{/snippet}

<section class="view" aria-label={title}>
  {#if making}
    <div class="bar">
      <span class="navpill">{@render leave()}</span>
      <span class="end">
        <button class="barbtn iconic" aria-label="Cancel" title="Cancel" onclick={() => (making = null)} disabled={making.saving}><Icon name="reset" size={20} /></button>
        <button class="barbtn save" onclick={makePlaylist} disabled={making.saving || !making.name.trim()} aria-busy={making.saving}>Make playlist</button>
        <PanelPlace />
      </span>
    </div>
  {:else if other}
    <!-- Another of the playlists' views (Info, Export, Repair, Sync; from their menu): back to the tracks, its name over
         theirs, the panel's place. -->
    <div class="bar sub">
      <span class="navpill"
        ><button aria-label="Back to the tracks" title="Back" onclick={backToTracks}><Icon name="back" size={22} /></button
        >{#if full}<button aria-label="Home" title="Home" onclick={goHome}><Icon name="home" size={22} /></button>{/if}</span
      >
      <div class="subtitle">
        <h2>{TABS.find((t) => t.tab === tab)?.label}</h2>
        <p>{multi ? ids.join(', ') : title}</p>
      </div>
      <span class="end"><PanelPlace /></span>
    </div>
  {:else}
    <!-- Symfonium's floating pills over the cover: back (when there is a view before this one) and ✕ on the left; the
         filter, View and where it plays, and the panel's place, on the right (in edit mode the place only). Once the
         action bar has the top they float just under it. -->
    {#if !handed}
      <div class="pills" class:below={stuck}>
        {@render glassLeft()}
        <span class="end">{#if pill()}<TopPill {...pill()!} />{/if}<PanelPlace /></span>
      </div>
    {/if}
  {/if}

  <div class="scroll" class:away={(handed || full) && everywhere()} class:own bind:this={scroller} onscroll={onScroll}>
    {#if making}
      <header class="head">
        <div class="top">
        <div class="art" class:mosaic={coversShown.length === 4}>
          {#each coversShown as c, i (i)}<Cover src={c} />{:else}<Cover src={null} />{/each}
        </div>
          <div class="info fields">
            <label class="field">
              <span>Name of the new playlist</span>
              <!-- svelte-ignore a11y_autofocus -->
              <input type="text" maxlength="100" autofocus bind:value={making.name} disabled={making.saving} onkeydown={(e) => e.key === 'Enter' && makePlaylist()} />
            </label>
          </div>
      </div>
      </header>
    {:else if !other}
      <div
        class="hero"
        class:mosaic={!heroPending && heroShown.length === 4 && !flowing}
        class:bare={!heroPending && !heroShown.length && !flowing}
        class:flowing={flowing || (heroPending && !!crate) || oneWide}
        bind:clientWidth={heroW}
        bind:clientHeight={heroH}
      >
        {#if heroPending}
          <!-- Its art on its way: the place kept, empty. -->
        {:else if flowing}
          <!-- A crate's albums as a coverflow on a light canvas (the owner, 2026-10-23); a drag or a tap moves through them. -->
          <CoverFlow covers={flowShown} onmenu={(i) => (flowMenu = flowItems[i]?.album ?? null)} />
        {:else if oneWide}
          <!-- Alone, it takes the top row's middle too (the glass is at that row's far ends) and comes nearer the action
               bar (the owner, 2026-10-26: at least twice the size it had). -->
          <CoverFlow covers={heroShown} top={8} foot={40} />
        {:else}
          {#each heroShown as c, i (i)}<Cover src={c} lazy={false} />{:else}{#if !crateEditing}<Cover src={null} />{/if}{/each}
        {/if}
        {#if crateEditing}
          <!-- The crate's own picture, set or removed with Save. -->
          <span class="art-tools">
            <label class="art-tool"
              ><Icon name="image" size={18} /><span>Change image</span><input type="file" accept="image/*" hidden onchange={pickCrateImage} /></label
            >
            {#if ownImage}<button class="art-tool" onclick={removeCrateImage}><Icon name="imageOff" size={18} /><span>Remove image</span></button>{/if}
          </span>
        {/if}
      </div>
      {#if editing}
      <!-- Edit mode's bar in the action bar's place, its buttons where the bar's were: Delete, then Cancel (in red once
           something changed) in the pencil's place, the heart staying, Save in the menu's place. -->
      <div class="actbar" class:stuck class:loose={!prefs.stickyActions} bind:this={actbar} role="toolbar" aria-label="Edit">
        <span class="grow"></span>
        {#if creating}
          <!-- New playlist: nothing to delete or favour yet; Cancel leaves, Save makes it once it has a name. -->
          <button class="icon" aria-label="Cancel" title="Cancel" onclick={() => closePlaylist()} disabled={editing.saving}><Icon name="reset" size={24} /></button>
          <button class="icon save" aria-label="Save" title="Save" onclick={save} disabled={editing.saving || !newName} aria-busy={editing.saving}
            ><Icon name="arrowDownToLine" size={24} /></button
          >
        {:else}
        <button
          class="icon"
          aria-label={`Delete ${id}`}
          title="Delete"
          disabled={editing.saving}
          onclick={() => {
            menuStart = 'delete';
            menuOpen = true;
          }}><Icon name="trash" size={22} /></button
        >
        <button class="icon" class:destructive={tracksChanged || nameChanged} aria-label="Cancel" title="Cancel" onclick={() => (editing = null)} disabled={editing.saving}
          ><Icon name="reset" size={24} /></button
        >
        <button class="icon" class:fav={allFav} aria-pressed={allFav} aria-label="Favourite" onclick={() => setPlaylistFavorites([...ids], !allFav)}
          ><Icon name="heart" size={24} /></button
        >
        <button class="icon save" aria-label="Save" title="Save" onclick={save} disabled={editing.saving} aria-busy={editing.saving}><Icon name="arrowDownToLine" size={24} /></button>
        {/if}
      </div>
      {:else if crateEditing}
      <!-- A crate's edit bar, as a playlist's: Delete (one's own; a second tap confirms), Cancel in the pencil's place,
           the favourites toggle staying, Save in the last place. -->
      <div class="actbar" class:stuck class:loose={!prefs.stickyActions} bind:this={actbar} role="toolbar" aria-label="Edit">
        <span class="grow"></span>
        {#if builder?.id}
          <button
            class="icon"
            class:destructive={crateDeleteArmed}
            aria-label={crateDeleteArmed ? `Tap again to delete ${title}` : `Delete ${title}`}
            title="Delete"
            disabled={crateSaving}
            onclick={deleteOwnCrate}><Icon name="trash" size={22} /></button
          >
        {/if}
        <button class="icon" aria-label="Cancel" title="Cancel" onclick={cancelCrate} disabled={crateSaving}><Icon name="reset" size={24} /></button>
        {#if favState && crate}
          {@const c = crate}
          <button class="icon" class:fav={favState.filled} aria-label={`Showing: ${favState.label}. Tap for the next`} title={favState.label} onclick={() => cycleFavourites(c)}
            ><Icon name={favState.icon} size={24} /></button
          >
        {/if}
        {#if albState && crate}
          {@const c = crate}
          <button class="icon" class:fav={albState.filled} aria-label={`Showing: ${albState.label}. Tap for the next`} title={albState.label} onclick={() => cycleFavouriteAlbums(c)}
            ><Icon name={albState.icon} size={24} /></button
          >
        {/if}
        <button class="icon save" aria-label="Save" title="Save" onclick={saveCrate} disabled={crateSaving || !crateDraft?.name.trim()} aria-busy={crateSaving}
          ><Icon name="arrowDownToLine" size={24} /></button
        >
      </div>
      {:else}
      <!-- The action bar, at the cover's foot; at the top it stays. -->
      <div class="actbar" class:stuck class:loose={!prefs.stickyActions} bind:this={actbar} role="toolbar" aria-label="Play">
        {#each barActions as a (a.key)}
          <button class="icon" aria-label={a.label} title={a.label} disabled={a.disabled || (!!starting && starting !== a.key)} aria-busy={starting === a.key} onclick={a.run}
            >{#if starting === a.key}<span class="ring-in"><ProgressRing progress={null} size={24} spin /></span>{:else}<Icon name={a.icon} size={24} />{/if}</button
          >
        {/each}
        <span class="grow"></span>
        {#if crate}
          <!-- What shows the crate stays here (its favourites filters); what is done with it is its menu's (decision
               0058: Sync to this device, Edit, Save as playlist, Start a crate from a genre). -->
          {#if favState}
            <button class="icon" class:fav={favState.filled} aria-label={`Showing: ${favState.label}. Tap for the next`} title={favState.label} onclick={() => crate && cycleFavourites(crate)}
              ><Icon name={favState.icon} size={24} /></button
            >
          {/if}
          {#if albState}
            <button class="icon" class:fav={albState.filled} aria-label={`Showing: ${albState.label}. Tap for the next`} title={albState.label} onclick={() => crate && cycleFavouriteAlbums(crate)}
              ><Icon name={albState.icon} size={24} /></button
            >
          {/if}
          <button class="icon" aria-label={`Menu of ${title}`} onclick={() => (crateMenuOpen = true)}><Icon name="moreVert" size={24} /></button>
        {:else if album?.source === BANDCAMP}
          <!-- A Bandcamp album (decision 0071): no heart, its own menu (lib/BandcampAlbumMenu). -->
          <button class="icon" aria-label={`Menu of ${title}`} onclick={() => (bandcampMenuOpen = true)}><Icon name="moreVert" size={24} /></button>
        {:else if offServer}
          <!-- The device's own music (local mode, decision 0052), an artist from Discover: no heart, no Cratarr menu. -->
        {:else if artist}
          {#if artistHead?.favorite !== undefined}
            <button class="icon" class:fav={artistHead.favorite} aria-pressed={artistHead.favorite} aria-label="Favourite" onclick={toggleArtistFavorite}
              ><Icon name="heart" size={24} /></button
            >
          {/if}
          <button class="icon" aria-label={`Menu of ${artist.name}`} onclick={() => (artistMenuOpen = true)}><Icon name="moreVert" size={24} /></button>
        {:else if album}
          <button class="icon" class:fav={albumFav} aria-pressed={albumFav} aria-label="Favourite" onclick={toggleAlbumFavorite}><Icon name="heart" size={24} /></button>
          <button class="icon" aria-label={`Menu of ${title}`} onclick={() => (albumMenuOpen = true)}><Icon name="moreVert" size={24} /></button>
        {:else}
          {#if p && p.mine !== false}<button class="icon" aria-label={`Edit ${id}`} title="Edit" disabled={!rows} onclick={startEdit}><Icon name="pencil" size={22} /></button>{/if}
          <button class="icon" class:fav={allFav} aria-pressed={allFav} aria-label="Favourite" onclick={() => setPlaylistFavorites([...ids], !allFav)}
            ><Icon name="heart" size={24} /></button
          >
          <button class="icon" aria-label={`Menu of ${title}`} onclick={() => (menuOpen = true)}><Icon name="moreVert" size={24} /></button>
        {/if}
      </div>
      {/if}
      <header class="meta">
        <!-- What it is, and beside it on a wide panel its tempo spread (decision 0041). -->
        <div class="meta-main">
          {#if editing}
          <!-- The name, editable where it is and looking the same; the pencil beside it says so (and puts the caret
               in it too). -->
          <div class="name-row">
          <h2
            bind:this={nameEl}
            class="editable"
            contenteditable="plaintext-only"
            class:saving={editing.saving}
            aria-label="Name"
            data-placeholder="New playlist"
            spellcheck="false"
            bind:textContent={editing.name}
            onkeydown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                e.currentTarget.blur();
              }
            }}
            oninput={(e) => {
              const el = e.currentTarget;
              if ((el.textContent ?? '').length > 100) el.textContent = (el.textContent ?? '').slice(0, 100);
            }}
          ></h2>
          <button class="name-pen" aria-label="Rename" title="Rename" disabled={editing.saving} onclick={editName}><Icon name="pencil" size={16} /></button>
          </div>
          {:else if crateEditing && crateDraft}
          <!-- A crate's name, as a playlist's in edit mode. -->
          <div class="name-row">
          <h2
            bind:this={nameEl}
            class="editable"
            contenteditable="plaintext-only"
            class:saving={crateSaving}
            aria-label="Name"
            data-placeholder="New crate"
            spellcheck="false"
            bind:textContent={crateDraft.name}
            onkeydown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                e.currentTarget.blur();
              }
            }}
            oninput={(e) => {
              const el = e.currentTarget;
              if ((el.textContent ?? '').length > 100) el.textContent = (el.textContent ?? '').slice(0, 100);
            }}
          ></h2>
          <button class="name-pen" aria-label="Rename" title="Rename" disabled={crateSaving} onclick={editName}><Icon name="pencil" size={16} /></button>
          </div>
          {#if permanentNames.includes(crateDraft.name.trim().toLowerCase())}<p class="details dead">That is a permanent crate's name.</p>{/if}
          {:else}
            <h2>{title}</h2>
          {/if}
          {#if details}<p class="details">{#if albumDot}<i class="dot" style:background={albumDot.colour} role="img" aria-label={albumDot.name}></i>{/if}{details}</p>{/if}
          {#if multi}<p class="details">{ids.join(', ')}</p>{/if}
          {#if album && albumHead?.artist}
            <button class="chip" disabled={!albumBy} onclick={() => albumBy && goToArtist(album.source, albumBy, album.source === BANDCAMP ? { id: album.id, title } : null)}
              >{#if covers[0]}<span class="chip-art"><Cover src={covers[0]} /></span>{/if}<span>{albumHead.artist}</span>{#if albumByFavorite}<i class="liked" role="img" aria-label="Favourite"
                  ><Icon name="heart" size={16} /></i
                >{/if}</button
            >
          {:else if artist?.tracks}
            <button class="chip" onclick={() => viewOver(artistSubject(artist.source, artist.name))}
              >{#if heroArt[0]}<span class="chip-art"><Cover src={heroArt[0]} /></span>{/if}<span>{artist.name}</span>{#if artistHead?.favorite}<i class="liked" role="img" aria-label="Favourite"
                  ><Icon name="heart" size={16} /></i
                >{/if}</button
            >
          {/if}
          <!-- A genre opens in the panel (its own crate). -->
          {#if shownGenres.length}<p class="genres">{#each shownGenres as g (g)}<button onclick={() => viewGenre(g)} use:longpress={() => (badgeMenu = CRATE + GENRE + g)}>{g}</button>{/each}</p>{/if}
          <!-- A playlist's or a crate's telling artists (its profile's), each opening the artist, headed "Typical artists";
               not an album's or an artist's own, whose header names them. -->
          {#if shownArtists.length}<p class="genres"><span class="row-head">Typical artists</span>{#each shownArtists as a (a)}<button onclick={() => goToArtist(source || 'cratarr', a)}>{a}</button>{/each}</p>{/if}
          {#if p?.dead || chosenDead}<p class="details dead missing">{p?.dead || chosenDead} missing</p>{/if}
          <!-- A crate's exclusions (decision 0024, amended 2026-10-19): a tap shows them, to put back. -->
          {#if leftOut}<button class="details left-out" onclick={() => (leftOutOpen = true)}><Icon name="crateOut" size={14} /><span>{leftOut.toLocaleString()} excluded</span></button>{/if}
          <!-- The artists left out of it by hand (0.10, decision 0083: Not in this crate), each a tap to take back. -->
          {#if crate && !crateEditing && excludedArtists.crates[crate]?.length}<p class="genres"><span class="row-head">Not in this crate</span>{#each excludedArtists.crates[crate] as a (a.key)}<button aria-label="Take {a.name} back into this crate" title="Take back" onclick={() => setNotInCrate(crate, a.name, false)}>{a.name}</button>{/each}</p>{/if}
        </div>
        <!-- A ranked crate's fit (the owner, 2026-10-23): what it learned not to suggest; a tap corrects it. -->
        {#if crate && like && !multi}<AvoidLine {crate} name={suggestsFor?.name ?? title} round={saved} seed={like.item} onchange={() => saved++} />{/if}
        {#if (shownProfile || (like && curve?.alike.length)) && !multi}
          <div class="head-spread" class:artist-head={!!artist && !artist.tracks}>
            {#if like && curve?.alike.length}<LikeCurve curve={curve.alike} count={like.count} onpreview={(n) => (preview = n)} onchange={(n) => viewLike(like.item, n)} />{/if}
            {#if shownProfile}<ListProfile data={shownProfile} />{/if}
          </div>
        {:else if (prefs.profile && profileComing) || curveComing}
          <div class="head-spread waiting" class:artist-head={!!artist && !artist.tracks} aria-hidden="true"></div>
        {/if}
      </header>
    {/if}

    {#if tab === 'info'}
      <div class="info-tab">
        {#each ids as pid (pid)}
          {#if multi}
            <Section title={pid}>{@render about(pid)}</Section>
          {:else}
            {@render about(pid)}
          {/if}
        {/each}
      </div>
    {:else if tab === 'export'}
      <ExportTab {ids} />
    {:else if tab === 'repair'}
      <RepairTab {ids} />
    {:else if tab === 'sync' && me.admin}
      <SyncTab {ids} />
    {:else if multi}
      <!-- Their columns' names once over all of them; each keeps its own order, so the names do not order. -->
      {#if cols.length}{@render columnHead(false)}{/if}
      {#each ids as sid, k (sid)}
        {@const s = sections[sid]}
        {@const count = shownPlaylists().find((x) => x.id === sid)?.count ?? 0}
        <Section title={sid} dot={layerColor(k, ids.length)} count={playlistsKnown() ? plural(count, 'track') : ''} bind:open={() => !folded[sid], (v) => (folded[sid] = !v)} attach={fromAction(seen, () => sid)}>
          {#if s && 'error' in s}
            <p class="error" role="alert">{s.error}</p>
          {:else if !s}
            <PageLoading />
          {:else if !s.rows.length}
            <p class="empty">No tracks yet.</p>
          {:else}
            <ol class="rows" bind:clientWidth={listWidth}>
              {#each s.rows as r, i (i)}{#if keeps(r)}{@render trackRow(r, s.source, sid, s.rows)}{/if}{/each}
            </ol>
          {/if}
        </Section>
      {/each}
    {:else if crateEditing}
      {#if crateDraft}
        <CrateEditor bind:conditions={crateDraft.conditions} bind:total={crateTotal} />
      {:else if error}
        <p class="error" role="alert">{error}</p>
      {:else}
        <PageLoading />
      {/if}
    {:else if editing}
      <ol class="rows edit" class:dragging={!!reorder.drag}>
        {#each editing.rows as x, i (x.at)}
          {@const r = x.r}
          <li class:lifted={reorder.drag?.from === i} style:transform={reorder.drag ? `translateY(${reorder.offset(i)}px)` : null}>
            <SwipeBehind left={OUT_OF_PLAYLIST} />
            <Row
              size="large"
              title={r.name ?? r.file}
              sub={r.missing ? 'Missing file' : !r.id ? 'Not matched yet' : (r.artist ?? '')}
              off={!r.id || r.missing}
              attach={fromAction(swipe, () => ({ enabled: !reorder.drag && !editing!.saving, left: () => removeSwiped(i) }))}
            >
              {#snippet lead()}{@render cover(r, source)}{/snippet}
              {#snippet action()}
                <button class="tool" aria-label={`Remove ${r.name ?? r.file}`} onclick={() => removeAt(i)}><Icon name="close" size={22} /></button>
                <button
                  class="tool grip"
                  aria-label="Drag to reorder"
                  onpointerdown={(e) => reorder.down(e, i)}
                  onpointermove={reorder.move}
                  onpointerup={reorder.end}
                  onpointercancel={reorder.end}><Icon name="order" size={24} /></button
                >
              {/snippet}
            </Row>
          </li>
        {/each}
      </ol>
      {#if editing.removed.length}
        <h3 class="removed-head">Removed tracks</h3>
        <ol class="rows">
          {#each editing.removed as x (x.at)}
            <li>
              <Row size="large" title={x.r.name ?? x.r.file} sub={x.r.artist ?? ''} off>
                {#snippet lead()}{@render cover(x.r, source)}{/snippet}
                {#snippet action()}<button class="tool" aria-label={`Restore ${x.r.name ?? x.r.file}`} onclick={() => restore(x)}><Icon name="back" size={22} /></button>{/snippet}
              </Row>
            </li>
          {/each}
        </ol>
      {/if}
    {/if}

    {#if tab !== 'tracks' || multi || editing || crateEditing}
      <!-- another tab, the sections, or the edit list: above -->
    {:else if error}
      <p class="error" role="alert">{error}</p>
    {:else if artist && !artist.tracks && rows && artistHead}
      <!-- An artist's page (Symfonium's): their albums, what they appear on, their most played. -->
      <ArtistBody
        source={fromDiscover(artist.source) ? DISCOVER : artist.source}
        albumSource={fromDiscover(artist.source) ? BANDCAMP : artist.source}
        name={artist.name}
        mbid={artistHead.mbid ?? null}
        searching={own}
        albums={artistHead.albums}
        appearsOn={artistHead.appears_on}
        top={topRows.length ? topList : undefined}
        ontracks={() => viewOver(artistSubject(artist.source, artist.name, true))}
        profile={shownProfile ? artistProfile : undefined}
        profileComing={prefs.profile && profileComing}
      />
      {#snippet artistProfile()}<ListProfile data={shownProfile!} fixed />{/snippet}
      {#snippet topList()}
        <ol class="rows">{#each topRows as r (r.id)}{@render trackRow(r, source, title, topRows, null, false)}{/each}</ol>
      {/snippet}
    {:else if rows && !rows.length}
      <p class="empty">No tracks yet.</p>
    {:else if rows && shown}
      {#if openRecord}
        <!-- A record opened: its crate tracks, back to the records (a step of the history). -->
        <SectionHead title={`${openRecord.title || 'Unknown album'} · ${holdsLine(openRecord.at.length, albumTotal(openRecord))}`} onback={() => history.back()} />
        <ol class="rows" class:selecting={!!selecting} bind:clientWidth={listWidth}>
          {#each recordShown as x (x.i)}{@render trackRow(x.r, source, title, recordShown.map((y) => y.r), x.i, false)}{/each}
        </ol>
      {:else if recordsView}
        <ListHead
          fields={RECORD_ORDERS}
          bind:value={() => prefs.crateRecordSort.key, (v) => (prefs.crateRecordSort = { key: v as RecordOrder, down: prefs.crateRecordSort.down })}
          bind:down={() => prefs.crateRecordSort.down, (d) => (prefs.crateRecordSort = { key: prefs.crateRecordSort.key, down: d })}
          count={plural(records.length, 'record')}
          selecting={!!selecting}
          onselect={() => (selecting ? (selecting = null) : startSelect())}
        />
        <ol class="rows" class:selecting={!!selecting} bind:clientWidth={listWidth}>
          {#each records as rec (rec.key)}{@render recordRow(rec)}{/each}
        </ol>
      {:else}
      <ListHead
        fields={sortFields}
        bind:value={() => sortKey, (v) => setSort(v, sortDown)}
        bind:down={() => sortDown, (d) => setSort(sortKey, d)}
        bind:open={sortOpen}
        count={plural(crate ? total : shownPlayable.length, 'track')}
        selecting={!!selecting}
        onselect={() => (selecting ? (selecting = null) : startSelect())}
      />
      {#if cols.length}{@render columnHead()}{/if}
      <ol class="rows" class:numbered={!!album} class:selecting={!!selecting} bind:clientWidth={listWidth}>
        {#each shown as x (x.i)}{#if album}{@render albumRow(x.r, x.i, shownRows)}{:else}{@render trackRow(x.r, source, title, shownRows, x.i)}{/if}{/each}
      </ol>
      {/if}
      {#if crate && rows.length < total}<div class="loading" bind:this={more}><MarkLoader size={40} /></div>{/if}
      <!-- Each list's own kind of similar ones at its foot (the owner, 2026-10-23); a crate's once all its rows are in. -->
      <!-- One's own playlist: tracks to add to it (lib/SuggestedTracks; the owner, 2026-10-23). -->
      {#if !own}
      {#if p && !multi && p.mine !== false}<SuggestedTracks id={p.id} name={p.name} count={p.count} />{/if}
      {#if p && !multi}<SimilarLists kind="playlist" id={p.id} />{/if}
      {#if crate && !seeded(crate) && !crateEditing && rows.length >= total}<SimilarLists kind="crate" id={crate} />{/if}
      {/if}
      {#if album && albumHead && !offServer}
        <!-- As Symfonium's album: the artist's other albums, the similar ones, then About; seen through a search, its
             About only. -->
        {#if !own}
          {@const by = listedHere?.a.artists?.[0] ?? mainArtist(albumHead.artist)}
          {#if by}<AlbumMore source={album.source} album={album.id} artist={by} />{/if}
          <SimilarAlbums source={album.source} album={album.id} />
        {/if}
        <AlbumAbout source={album.source} album={album.id} genres={listGenres ?? []} year={albumHead.year} added={albumHead.added} played={albumHead.played} plays={albumHead.plays} provider={provider} />
      {/if}
      {#if album && albumHead && album.source === BANDCAMP}
        <!-- A Bandcamp album's About (the owner, 2026-10-25: "as much as our own panel"): what its page says. -->
        <AlbumAbout
          source={album.source}
          album={album.id}
          genres={listGenres ?? []}
          year={albumHead.year}
          provider={{ server: 'Bandcamp', library: null, colour: null }}
          more={albumHead}
        />
      {/if}
    {:else}
      <PageLoading />
    {/if}
      <!-- On a phone, over the page: the global results at the end of this list (decision 0072). -->
    {#if (handed || full) && everywhere()}<GlobalResults />{/if}
  </div>

  {#if selecting}
    <SelectBar label="Selected tracks" count="{selecting.size}/{shown?.length ?? 0}" onclose={() => (selecting = null)} every={{ all: !!shown?.length && selecting.size === shown.length, onall: () => selectWhich('all'), onnone: () => selectWhich('none') }}>
        <button class="icon" aria-label="Play" title="Play" disabled={!ticked.length} onclick={() => actOnTicked('play')}><Icon name="play" size={24} /></button>
        <button class="icon" aria-label="Play next" title="Play next" disabled={!ticked.length} onclick={() => actOnTicked('next')}><Icon name="playNext" size={24} /></button>
        <button class="icon" aria-label="Add to queue" title="Add to queue" disabled={!ticked.length} onclick={() => actOnTicked('last')}><Icon name="listAdd" size={24} /></button>
        <button class="icon" class:fav={tickedFav} aria-pressed={tickedFav} aria-label="Favourite" title="Favourite" disabled={!ticked.length} onclick={favouriteTicked}
          ><Icon name="heart" size={24} /></button
        >
        <!-- The menu of what is ticked, at the far right (decision 0058: Add to playlist, Download…; Remove from
             or Exclude from where the rows' own menus have them). -->
        <button class="icon" aria-label="Menu of the ticked tracks" title="More" disabled={!ticked.length} onclick={() => (tickedMenu = { rows: ticked, tracks: tracksOf(ticked) })}
          ><Icon name="moreVert" size={24} /></button
        >
    </SelectBar>
  {/if}
</section>

<!-- The Info tab's part for one playlist: how focused it is, and its closest playlists as large rows (a tap views one). -->
{#snippet about(pid: string)}
  {@const i = insights.indexOf(pid)}
  {@const pt = i >= 0 ? insights.shown!.playlists[i] : null}
  {@const near = insights.closest(i)}
  <div class="about" aria-label={`About ${pid}`}>
    {#if !pt}
      {#if insights.shown}<p class="note">Nothing is known about it yet.</p>{:else}<div class="loading"><MarkLoader size={40} /></div>{/if}
    {:else}
      {#if pt.focus !== null && insights.focus[i] !== undefined}
        <p class="note">More focused than {Math.round(100 * insights.focus[i])}% of your playlists</p>
      {/if}
      {#if near.length}
        <h3 class="list-head">Closest playlists</h3>
        <ul class="plain">
          {#each near as c (c.index)}
            <li>
              <PlaylistItem
                p={shownPlaylists().find((x) => x.id === c.name) ?? { id: c.name, name: c.name }}
                size="large"
                sub={c.shares}
                label={`View ${c.name}`}
                onclick={() => viewPlaylist(c.name)}
                attach={fromAction(longpress, () => () => (badgeMenu = c.name))}
              />
            </li>
          {/each}
        </ul>
      {:else if pt.focus === null}
        <p class="note">Nothing is known about it yet.</p>
      {/if}
    {/if}
  </div>
{/snippet}

<!-- A track's row: a tap plays its playlist (`name`, these rows) from it, a swipe right plays it next, a swipe left takes
     it out of its crate or playlist, a long-press is its menu, the heart adds it to playlists. -->
<!-- The list's column names over its tracks (decision 0042): a row of the same shape, so they line up. A tap on one
     orders the list by it where the list has an order of its own to change (`sorts`). -->
{#snippet columnHead(sorts = true)}
  <div class="column-head">
    <!-- Its end as wide as its rows' (rowEnd: the heart and the menu, the tick while selecting, nothing for the
         device's own music), so the columns share out the same room and line up (the owner, 2026-10-23). -->
    <Row size="large" as="div" head title="Title" count="Length" cells={sorts ? headCells : cols.map((c) => ({ ...c, text: c.label }))} action={onDevice ? undefined : endSpace}>
      {#snippet lead()}<span class="cover-space"></span>{/snippet}
    </Row>
  </div>
{/snippet}

<!-- `columns`: its list's columns where it has room (a short list of top tracks has none). -->
{#snippet trackRow(r: PlaylistRow, src: string, name: string, list: PlaylistRow[], i: number | null = null, columns = true)}
  {@const ok = !!r.id && !r.missing}
  {@const here = ok && !!np && np.id === r.id}
  {@const out = selecting ? null : outOf(r, name)}
  <li class:now={here} class:ticked={!!selecting && i !== null && selecting.has(i)}>
    <SwipeBehind right={ok ? PLAY_NEXT : null} left={out ? (crate ? OUT_OF_CRATE : OUT_OF_PLAYLIST) : null} />
    <Row
      size="large"
      title={r.name ?? r.file}
      count={r.duration_secs ? clockText(r.duration_secs) : undefined}
      cells={columns ? cellsOf(r) : undefined}
      off={!ok || awayDim(r, src)}
      disabled={!ok}
      onclick={() => (selecting && i !== null ? tick(i) : playFrom(name, src, list, r))}
      attach={fromAction(swipe, () => ({ enabled: !selecting, right: ok ? () => playNext(r, src) : null, left: out }))}
      attachMain={fromAction(longpress, () => () => {
        if (!ok || onDevice) return;
        // In select mode it asks what to tick; else the track's menu (decision 0058: Select is the header's).
        if (selecting && i !== null) selectSheet = true;
        else trackMenu = { track: asTrack(r, src), start: 'menu', outs: menuOuts(out, name) };
      })}
    >
      {#snippet lead()}{@render cover(r, src, here)}{/snippet}
      {#snippet sub()}
        {#if r.missing}Missing file{:else if !r.id}Not matched yet{:else if artist}{#if !(columns && shows('album'))}<span class="artist">{r.album ?? ''}</span>{/if}{#if r.year && !(columns && shows('year'))}<span class="album">{r.year}</span>{/if}{:else}<span class="artist">{r.artist ?? ''}</span>{#if r.album && !(columns && shows('album'))}<span class="album">{r.album}</span>{/if}{/if}
      {/snippet}
      {#snippet action()}{#if ok}{@render rowEnd(r, src, i, out, name)}{/if}{/snippet}
    </Row>
  </li>
{/snippet}

<!-- A track's row ends in its heart (as the queue's rows: a tap its playlists, a long press its sheet, lib/HeartSheet;
     not for the device's own music) and ⋮ (its menu, Symfonium's), or in select mode in its tick. -->
{#snippet endSpace()}<span class="end-space" class:ticking={!!selecting}></span>{/snippet}

{#snippet rowEnd(r: PlaylistRow, src: string, i: number | null, out: (() => 'away') | null = null, list = '')}
  {#if suggestsFor && !selecting && r.id}
    <!-- A playlist's suggestions (lib/SuggestedTracks): turned down, or added to it. -->
    <button class="tool" aria-label="Not for {suggestsFor.name}" onclick={() => suggestion(r, src, false)}><Icon name="close" size={20} /></button>
    <button class="tool add" aria-label="Add to {suggestsFor.name}" onclick={() => suggestion(r, src, true)}><Icon name="plus" size={22} /></button>
  {:else if selecting && i !== null}
    <button class="tool tick" class:on={selecting.has(i)} role="checkbox" aria-checked={selecting.has(i)} aria-label={`Select ${r.name ?? r.file}`} onclick={() => tick(i)}
      ><Icon name={selecting.has(i) ? 'circleCheck' : 'circle'} size={24} /></button
    >
  {:else if !offServer}
    <RowHeart
      source={src}
      id={r.id!}
      name={r.name ?? r.file}
      artist={r.artist}
      cover={coverUrl({ source: src, id: r.id!, album_id: r.album_id })}
      outs={crateExcludes(out, r)}
      onopen={() => (trackMenu = { track: asTrack(r, src), start: 'lists' })}
    />
    <button class="tool" aria-label={`Menu of ${r.name ?? r.file}`} onclick={() => (trackMenu = { track: asTrack(r, src), start: 'menu', outs: menuOuts(out, list) })}
      ><Icon name="moreVert" size={22} /></button
    >
  {/if}
{/snippet}

<!-- An album's track (Symfonium's album screen): its number (the playing one's dots instead), the title, and the artist
     only when it is not the album's; the cover is the header's. -->
{#snippet albumRow(r: PlaylistRow, i: number, list: PlaylistRow[])}
  {@const ok = !!r.id && !r.missing}
  {@const here = ok && !!np && np.id === r.id}
  <li class:now={here} class:ticked={!!selecting && selecting.has(i)}>
    <SwipeBehind right={PLAY_NEXT} />
    <Row
      size="large"
      title={r.name ?? r.file}
      sub={r.missing ? 'Missing file' : r.artist && r.artist !== albumHead?.artist ? r.artist : undefined}
      count={r.duration_secs ? clockText(r.duration_secs) : undefined}
      off={!ok}
      disabled={!ok}
      onclick={() => (selecting ? tick(i) : playFrom(title, source, list, r))}
      attach={fromAction(swipe, () => ({ enabled: ok && !selecting, right: () => playNext(r, source) }))}
      attachMain={fromAction(longpress, () => () => ok && !onDevice && (selecting ? (selectSheet = true) : startSelect(i)))}
    >
      {#snippet lead()}
        <span class="num">{#if here}<span class="dots" role="img" aria-label="Playing"><PlayingDots still={paused()} /></span>{:else}{i + 1}{/if}</span>
      {/snippet}
      {#snippet action()}{#if ok}{@render rowEnd(r, source, i)}{/if}{/snippet}
    </Row>
  </li>
{/snippet}

<!-- A crate's record (0.10, lib/crateRecords): its album's cover, title, artist and how much of it the crate holds; a
     tap opens its crate tracks (in select mode ticks them all), a long press or its menu button its menu. -->
{#snippet recordRow(rec: CrateRecord)}
  {@const first = rows![rec.at[0]]}
  {@const all = !!selecting && rec.at.every((i) => selecting!.has(i))}
  <li class:ticked={all}>
    <Row
      size="large"
      title={rec.title || 'Unknown album'}
      label={selecting ? `${all ? 'Untick' : 'Tick'} ${rec.title}` : `Open ${rec.title}`}
      onclick={() => (selecting ? tickRecord(rec) : showRecord(rec.key))}
      attachMain={fromAction(longpress, () => () => (selecting ? (selectSheet = true) : (recordMenu = rec)))}
    >
      {#snippet lead()}{#if first}{@render cover(first, source)}{/if}{/snippet}
      {#snippet sub()}<span class="artist">{rec.artist ?? ''}</span><span class="album">{holdsLine(rec.at.length, albumTotal(rec))}</span>{/snippet}
      {#snippet action()}
        {#if selecting}
          <button class="tool tick" class:on={all} role="checkbox" aria-checked={all} aria-label={`Select ${rec.title}`} onclick={() => tickRecord(rec)}
            ><Icon name={all ? 'circleCheck' : 'circle'} size={24} /></button
          >
        {:else}
          <button class="tool" aria-label={`Menu of ${rec.title}`} onclick={() => (recordMenu = rec)}><Icon name="moreVert" size={22} /></button>
        {/if}
      {/snippet}
    </Row>
  </li>
{/snippet}

{#snippet cover(r: PlaylistRow, src: string, here = false)}
  <TrackCover src={r.id && !r.missing ? coverUrl({ source: src, id: r.id, album_id: r.album_id }) : null} playing={here} paused={paused()} />
{/snippet}

{#if menuOpen && selected.length}
  <PlaylistMenu
    playlists={selected}
    downloadable={inApp && selected.length === 1 && playable.length ? tracksOf(playable) : null}
    keepFrom={inApp && selected.length === 1 && source ? source : null}
    start={menuStart}
    ondeleted={() => (editing = null)}
    onclose={() => {
      menuOpen = false;
      menuStart = 'menu';
    }}
  />
{/if}
{#if selectSheet}
  <SelectSheet onall={() => selectWhich('all')} onnone={() => selectWhich('none')} onothers={() => selectWhich('others')} onclose={() => (selectSheet = false)} />
{/if}
{#if tickedMenu}
  {@const away = tickedOuts(tickedMenu.rows)}
  <SelectionMenu title={plural(tickedMenu.tracks.length, 'track')} tracks={tickedMenu.tracks} onclose={() => ((tickedMenu = null), (selecting = null))}>
    {#snippet extra(close)}
      {#if away.length}
        <MenuGroup>
          {#each away as o (o.label)}
            <MenuEntry
              icon={o.icon}
              label={o.label}
              danger
              onclick={() => {
                close();
                o.run();
              }}
            />
          {/each}
        </MenuGroup>
      {/if}
    {/snippet}
  </SelectionMenu>
{/if}
{#if recordMenu}
  {@const rec = recordMenu}
  {@const rs = recordRows(rec)}
  {@const away = tickedOuts(rs)}
  <SelectionMenu
    title={rec.title || 'Unknown album'}
    chips={[rec.artist ?? '', holdsLine(rec.at.length, albumTotal(rec))].filter(Boolean)}
    tracks={tracksOf(rs.filter((r) => r.id && !r.missing))}
    onclose={() => (recordMenu = null)}
  >
    {#snippet extra(close)}
      {#if rec.albumId}
        <MenuGroup>
          <MenuEntry
            icon="squareArrowOutUpRight"
            label="Go to album"
            onclick={() => {
              close();
              viewOver(albumSubject(source, rec.albumId!));
            }}
          />
        </MenuGroup>
      {/if}
      {#if away.length}
        <MenuGroup>
          {#each away as o (o.label)}
            <MenuEntry
              icon={o.icon}
              label={o.label}
              danger
              onclick={() => {
                close();
                o.run();
              }}
            />
          {/each}
        </MenuGroup>
      {/if}
    {/snippet}
  </SelectionMenu>
{/if}
{#if badgeMenu}
  <SubjectMenu subject={badgeMenu} onclose={() => (badgeMenu = null)} />
{/if}
{#if crateMenuOpen && crate}
  <CrateMenu id={crate} onclose={() => (crateMenuOpen = false)} onsave={() => (making = { name: '', saving: false })} />
{/if}
{#if trackMenu}
  <TrackMenu track={trackMenu.track} start={trackMenu.start} outs={trackMenu.outs} crate={crateEditing ? null : crate} onclose={() => (trackMenu = null)} />
{/if}
{#if leftOutOpen && crate}
  <ExcludedSheet {crate} onchange={() => saved++} onclose={() => (leftOutOpen = false)} />
{/if}
{#if artistMenuOpen && artist}
  <ArtistMenu
    source={artist.source}
    name={artist.name}
    cover={heroArt[0] ?? null}
    albums={artistHead?.albums.length ?? 0}
    tracks={tracksOf(playable)}
    favorite={artistHead?.favorite}
    ontogglefavorite={toggleArtistFavorite}
    onact={(mode) => act(mode)}
    onradio={port().radio && artist.source === 'cratarr' ? artistRadio : undefined}
    onclose={() => (artistMenuOpen = false)}
  />
{/if}
{#if flowMenu}
  {@const m = flowMenu}
  <AlbumMenu {source} album={m} onclose={() => (flowMenu = null)} />
{/if}
{#if bandcampMenuOpen && album && albumHead}
  <BandcampAlbumMenu album={{ id: album.id, title: albumHead.title, artist: albumHead.artist }} tracks={tracksOf((rows ?? []).filter((r) => r.id && !r.missing), BANDCAMP)} onclose={() => (bandcampMenuOpen = false)} />
{/if}
{#if albumMenuOpen && album}
  <AlbumMenu
    source={album.source}
    album={listedHere?.a ?? { id: album.id, title, artist: albumHead?.artist ?? null }}
    cover={covers[0] ?? null}
    tracks={rows ? tracksOf(playable) : null}
    genres={listGenres}
    offer={listedHere ? offerIn(listedHere.s, listedHere.a) : null}
    trashDays={library.view?.trash_days ?? 14}
    music={library.view?.music ?? 'Music'}
    viewable={false}
    onresumegone={() => (resumeAt = null)}
    onclose={() => (albumMenuOpen = false)}
  />
{/if}

<style>
  .view {
    /* The top bar floats over the cover. */
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    /* As the navigation beside the page. */
    background: var(--surface);
  }

  /* Finamp's app bar: no title (the name is in the header), back on the left, the menu on the right. */
  .bar {
    flex: none;
    display: flex;
    justify-content: space-between;
    padding: 4px 4px 0;
  }

  .end {
    display: flex;
    align-items: center;
  }

  /* Another view's bar: back, its name over the playlists', the place. */
  .bar.sub {
    align-items: center;
    gap: 12px;
    padding: 8px 8px 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .subtitle {
    flex: 1;
    min-width: 0;
  }

  .subtitle h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
  }

  .subtitle p {
    margin: 2px 0 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 14px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Edit mode's Save: the accent, as a filled round. Cancel in red once there is something to lose. */
  .actbar .icon.save {
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-text);
  }

  .actbar .icon.destructive {
    color: var(--danger-ink);
  }

  /* Finamp's playlist heart: filled when it is a favourite; the pin filled when pinned. */
  .icon.fav :global(svg) {
    fill: currentColor;
  }

  /* The narrowest panel: the place button a little closer, so the bar's buttons fit on one line. */
  @container (max-width: 299px) {
    .bar :global(.place) {
      width: 42px;
    }
  }

  /* Edit mode's Cancel and Save, where ✕ and the menu are otherwise. */
  .barbtn {
    min-height: 40px;
    margin: 4px 8px;
    padding: 0 14px;
    border: 0;
    border-radius: 10px;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }

  .barbtn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  /* Cancel and Save are icons (their labels on a long-press, lib/labels). */
  .barbtn.iconic {
    padding: 0 10px;
  }

  .barbtn.save {
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .barbtn:disabled {
    opacity: 0.6;
  }

  .tool {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 100%;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .grip {
    cursor: grab;
    touch-action: none;
  }

  .rows.edit.dragging li {
    transition: transform 200ms ease;
  }

  .rows.edit.dragging li.lifted {
    z-index: 1;
    transition: none;
  }

  /* The name in edit mode: the title as it is, with a caret once tapped (no field around it); all of it shows while
     it is being written. */
  h2.editable {
    cursor: text;
    outline: none;
  }

  /* A new crate's name before it has one. */
  h2.editable:empty::before {
    content: attr(data-placeholder);
    color: var(--muted);
  }

  .name-row {
    display: flex;
    align-items: flex-start;
    min-width: 0;
  }

  .name-row h2 {
    flex: 0 1 auto;
    min-width: 0;
  }

  .name-pen {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 24px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    cursor: pointer;
  }

  /* While it saves the name is not written in. */
  h2.editable.saving {
    pointer-events: none;
  }

  h2.editable:focus {
    display: block;
    overflow: visible;
  }

  /* Make playlist's name field, in place of the info beside the cover. */
  .info.fields {
    justify-content: center;
    gap: 8px;
  }

  .field {
    display: grid;
    gap: 4px;
    padding: 0 6px;
    font-size: 13px;
    color: var(--muted);
  }

  .field input {
    min-height: 40px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 15px;
  }

  .removed-head {
    margin: 16px 16px 4px;
    font-size: 14px;
    font-weight: 500;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  /* The pencil while the tracks load: in its place, not yet usable. */
  .icon:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .scroll {
    /* What the action bar's offset is measured in. */
    position: relative;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    /* On a phone the mini-player floats over the end of the list (as Finamp's now-playing bar): room to scroll past it. */
    padding-bottom: calc(32px + var(--mini-h, 0px) + env(safe-area-inset-bottom));
  }

  /* Never wider than the panel (as the tabs and their content below): its one column is the panel's width at most,
     not what its widest part would like. */
  .head {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
    padding: 0 8px 8px;
  }

  /* The cover beside the info, as Finamp's header: 125 px, corners 4. */
  .top {
    display: flex;
    gap: 4px;
  }

  .art {
    flex: none;
    width: 125px;
    height: 125px;
    overflow: hidden;
    border-radius: 4px;
    background: var(--bg);
  }

  .art.mosaic {
    display: grid;
    grid-template: 1fr 1fr / 1fr 1fr;
  }

  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding-bottom: 2px;
  }

  h2 {
    display: -webkit-box;
    overflow: hidden;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    margin: 0;
    padding: 0 6px 6px;
    font-size: 17px;
    font-weight: 500;
    line-height: 1.25;
  }

  .genres button {
    flex: none;
    padding: 1px 8px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text) 10%, transparent);
    color: inherit;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }

  .dead {
    color: var(--danger-ink);
  }

  .tool.add {
    color: var(--accent-ink);
  }

  .left-out {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    cursor: pointer;
  }

  .info-tab {
    padding: 0 8px;
  }

  .about {
    padding: 4px 0 8px;
  }

  /* The list's column names (decision 0042): in from the sides as a track's row is, a cover's room in front. */
  .column-head {
    padding: 0 8px;
  }

  .cover-space {
    display: block;
    width: 60px;
  }

  /* The head's end: a row's heart (36 px), the gap (6 px) and its menu (40 px); while selecting, the tick (40 px). */
  .end-space {
    display: block;
    width: 82px;
  }

  .end-space.ticking {
    width: 40px;
  }

  /* The tempo spread in the header, under the genres (decision 0041); what is missing stays under it. */
  .head-spread {
    align-self: stretch;
    width: 100%;
    margin: 6px 0 2px;
  }

  .meta-main {
    display: contents;
  }

  .meta .missing {
    order: 1;
  }

  /* A wide panel (one over the page, or dragged wide): the spread at the header's right, its top level with the name's,
     so the header is no taller than what it says; it narrows with the panel (its widest: lib/Spread). */
  @container (min-width: 640px) {
    /* An artist's profile goes beside their albums there (lib/ArtistBody), not in the header. */
    .head-spread.artist-head {
      display: none;
    }

    .meta:has(.head-spread:not(.artist-head)) {
      grid-template-columns: clamp(220px, 28cqi, 400px) minmax(280px, 1fr);
      align-items: start;
      column-gap: 32px;
    }

    .meta-main {
      display: grid;
      justify-items: start;
      gap: 8px;
    }

    .head-spread {
      margin: 4px 0 0;
    }

    /* Beside the name nothing is above the profile: no line over it (the owner, 2026-10-24). */
    .head-spread :global(.profile > .rule) {
      display: none;
    }
  }

  .note {
    margin: 4px 8px;
    color: var(--muted);
    font-size: 13px;
  }

  /* A list's heading inside a tab (Closest playlists): small, over its large rows. */
  .list-head {
    margin: 12px 8px 4px;
    color: var(--muted);
    font-size: 13px;
    font-weight: 500;
  }

  ul.plain {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Several playlists on Tracks: a section each, its header folding it. */
  /* Finamp's track rows: 70 px (a 60 px tile and a gap), a 60 px cover with corners 8, two lines of title. */
  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows li {
    position: relative;
    height: 70px;
    padding: 6px 8px 0;
    box-sizing: border-box;
    /* Only rows on screen are drawn (a 500-track playlist shows fast). */
    content-visibility: auto;
    contain-intrinsic-size: auto 70px;
    /* The row is opaque over what its swipe shows. */
    --row-bg: var(--surface);
  }

  /* The playing track: tinted in the cover's colour (Finamp: primary at 30–35 %). */
  .rows li.now {
    --row-bg: color-mix(in srgb, var(--art-accent, var(--accent)) 32%, var(--surface));
  }

  /* A ticked track: the whole row lighter, its tick included (edge to edge as the row is). */
  .rows li.ticked {
    --row-bg: color-mix(in srgb, var(--text) 10%, var(--surface));
  }

  /* Finamp's track line under the title: the artist, then the album lighter. */
  .rows :global(.row .sub) {
    color: inherit;
  }

  .artist {
    opacity: 0.75;
  }

  .album {
    margin-left: 10px;
    font-weight: 300;
    opacity: 0.6;
  }

  /* Symfonium's floating pills, over the cover; under the action bar once that has the top (its 52 px). */
  .pills {
    position: absolute;
    inset: 0 0 auto;
    z-index: 3;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 6px 8px 0;
    pointer-events: none;
    transition: transform 150ms ease;
  }

  .pills > * {
    pointer-events: auto;
  }

  .pills.below {
    transform: translateY(52px);
  }

  .pills .end {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  /* Back and ✕ (lib/TopPill's look). */
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
    border-radius: 22px;
    background: none;
    color: inherit;
    cursor: pointer;
  }

  .pills .end :global(.place) {
    border-radius: 50%;
    background: var(--float-bg);
    -webkit-backdrop-filter: var(--float-blur);
    backdrop-filter: var(--float-blur);
  }

  /* The cover, edge to edge as Symfonium's; beside the page never so tall that the tracks drop out of sight. Its foot
     fades into the panel, where the action bar sits. */
  /* Its own search (`own`): the cover, the action bar and the title's header set aside while it narrows. */
  .scroll.own > .hero,
  .scroll.own > .actbar {
    display: none;
  }

  /* With no cover to start under, the list starts under the glass. */
  .scroll.own {
    padding-top: calc(var(--bar-room, 0px) + var(--mini-top, 0px) + var(--glass-top) + 4px);
  }

  .hero {
    position: relative;
    width: 100%;
    aspect-ratio: 1;
    max-height: 60vh;
    overflow: hidden;
    background: var(--bg);
  }

  /* The photo behind every flow, under a veil (Settings › Appearance; the owner, 2026-10-26), once it is in (App); till
     then the page's own ground, never a grey canvas (the owner, 2026-10-27). A crate's coverflow takes the whole header;
     the fade only under the action bar, so the front cover can use the height (lib/CoverFlow keeps it above). */
  :global(:root[data-flow-photo]) .hero.flowing {
    background:
      linear-gradient(to bottom, rgb(0 0 0 / 0.35), rgb(0 0 0 / 0.55)),
      var(--flow-photo) center 20% / cover no-repeat,
      var(--bg);
  }

  .hero.flowing::after {
    height: 72px;
  }

  .hero.mosaic {
    display: grid;
    grid-template: 1fr 1fr / 1fr 1fr;
  }

  /* Nothing with a cover (a new crate, say): a band with the stand-in (lib/Cover). */
  .hero.bare {
    aspect-ratio: auto;
    height: 168px;
  }

  /* Edit mode's Change image (and Remove), in the middle of the cover's place. */
  .art-tools {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    align-content: center;
    gap: 8px;
    padding: 16px;
  }

  .art-tool {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 0 16px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in srgb, var(--surface) 80%, transparent);
    color: var(--text);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
    backdrop-filter: blur(6px);
  }

  .hero::after {
    content: '';
    position: absolute;
    inset: auto 0 0;
    height: 40%;
    background: linear-gradient(to bottom, transparent, var(--surface));
    pointer-events: none;
  }

  /* The action bar, icons only: over the cover's foot, then staying at the top (with back and the panel's place). */
  /* Not sticky (Display's Sticky action bar off): it scrolls away with the rest. */
  .actbar.loose {
    position: relative;
    top: auto;
  }

  /* A Play under way: the shared ring where its icon was (lib/ProgressRing centres itself on what holds it). */
  .ring-in {
    position: relative;
    display: block;
    width: 24px;
    height: 24px;
  }

  .actbar {
    position: sticky;
    /* Docked under the bar of pages and a mini-player along the top, both drawn over the panel (App). */
    top: calc(var(--bar-top, 0px) + var(--mini-top, 0px));
    z-index: 2;
    display: flex;
    align-items: center;
    height: 52px;
    margin-top: -52px;
    padding: 0 4px;
  }

  .actbar.stuck {
    background: var(--surface);
    box-shadow: 0 1px 0 var(--border);
  }

  .actbar .icon {
    flex: none;
    width: 44px;
    height: 44px;
  }

  .grow {
    flex: 1;
  }

  /* The narrowest panel: the bar's icons a little closer, so all of them fit on one line. */
  @container (max-width: 379px) {
    .actbar .icon {
      width: 38px;
    }
  }

  /* Under the cover: the name large, one line of what it is, the artist as a chip, the genres. */
  .meta {
    display: grid;
    justify-items: start;
    gap: 8px;
    padding: 10px 16px 12px;
  }

  .meta h2 {
    -webkit-line-clamp: 3;
    line-clamp: 3;
    padding: 0;
    font-size: 26px;
    font-weight: 400;
    line-height: 1.2;
  }

  .details {
    margin: 0;
    color: var(--muted);
    font-size: 15px;
  }

  /* The album's artist (Symfonium's chip): it opens the artist. */
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 12px;
    max-width: 100%;
    padding: 0 16px 0 0;
    overflow: hidden;
    border: 0;
    border-radius: 8px;
    background: color-mix(in srgb, var(--text) 9%, transparent);
    color: inherit;
    font: inherit;
    font-size: 15px;
    cursor: pointer;
  }

  .chip:not(:has(img)) {
    padding-left: 16px;
  }

  .chip-art {
    flex: none;
    display: block;
    width: 36px;
    height: 36px;
  }

  .chip span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* A favourite artist: a heart after the name, filled as the panel's own heart is. */
  .chip .liked {
    display: flex;
    flex: none;
    margin-left: -4px;
  }

  .chip .liked :global(svg) {
    fill: currentColor;
  }

  p.genres {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin: 0;
  }

  /* The artists row's heading (the owner, 2026-10-24: "typical artists"), so it reads apart from the genres. */
  .row-head {
    margin-right: 4px;
    color: var(--muted);
    font-size: 12px;
  }

  /* An album's rows (Symfonium's): no cover, the number in front, a little lower than a playlist's. */
  .rows.numbered li {
    height: 58px;
    contain-intrinsic-size: auto 58px;
  }

  .rows.numbered :global(.row.large) {
    min-height: 52px;
  }

  .num {
    display: grid;
    place-items: center;
    width: 32px;
    color: var(--muted);
    font-size: 15px;
    font-variant-numeric: tabular-nums;
  }

  /* The playing track's dots in place of its number (lib/PlayingDots), in the cover's colour. */
  .dots {
    display: inline-flex;
    color: var(--art-accent, var(--accent));
  }

  /* A row's tick in select mode: an empty circle, filled with its check once ticked. */
  .tick :global(svg) {
    opacity: 0.7;
  }

  .tick.on {
    color: var(--art-accent, var(--accent));
  }

  .tick.on :global(svg) {
    opacity: 1;
  }

  /* The select bar: floating over the list's end, above the mini-player, in the cover's colour. */
  /* While ticking, the list scrolls clear of the bar. */
  .rows.selecting {
    padding-bottom: 96px;
  }

  .empty,
  .error {
    margin: 16px;
    color: var(--muted);
  }

  .error {
    color: var(--danger-ink);
  }

  /* The loading sign, small, where more is coming (lib/MarkLoader.svelte). */
  .loading {
    display: grid;
    place-items: center;
    padding: 16px 0;
  }

  /* The album's library's dot, as an album list's tile has it. */
  .details .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-right: 6px;
    border-radius: 50%;
    vertical-align: 1px;
  }
</style>
