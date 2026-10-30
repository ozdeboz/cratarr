<script lang="ts" module>
  import ReplaceTrackSheet from './ReplaceTrackSheet.svelte';
  import ChoiceSheet from './settings/ChoiceSheet.svelte';
  import { markClassical } from './api';
  import { leavePlayer } from './navStep.svelte';
  import { port } from './player/port.svelte';
  import { untrack } from 'svelte';
  import type { AlbumsSource, LibraryInfo, ListedAlbum } from './api';
  import { jobs } from './jobs.svelte';

  /** What an album's library allows (decision 0034): moves into the server's other used libraries, Replace for an
   *  upgrade, Delete; each only when Cratarr can write the album's folder, and with a job in the conversion queue only
   *  Cancel. */
  export type AlbumOffer = {
    /** The album's library. */
    from: LibraryInfo | null;
    /** The libraries it can move into. */
    targets: LibraryInfo[];
    replaceable: boolean;
    deletable: boolean;
    job: number | null;
  };

  /** For an album of the Library's list. One already in Music (an upgrade or a duplicate) does not move there again:
   *  an upgrade replaces. */
  export function offerIn(s: AlbumsSource, a: ListedAlbum): AlbumOffer {
    const deletable = s.deletable.includes(a.id);
    const verdict = s.verdicts[a.id];
    return {
      from: s.library_info.find((l) => l.id === a.library) ?? null,
      targets: deletable ? s.library_info.filter((l) => l.id !== a.library && !(verdict && l.use === 'music')) : [],
      replaceable: deletable && verdict?.kind === 'upgrade',
      deletable,
      job: jobs.of(s.id, a.id)?.id ?? null,
    };
  }
</script>

<script lang="ts">
  // An album's menu, the one every album has wherever it is opened (a Library row or tile, the select bar's More, the
  // album panel's ⋮), after Symfonium's album sheet: the album at the top (its heart there), the large actions (Play,
  // Resume, Shuffle, Play next, Add to queue), then its entries on rounded cards in the table's order (lib/menuTable),
  // related ones joined: where to go (View, each of its artists, its genres); its rating; Add to playlist (all its
  // tracks, the playlists as toggle cards in a step, as a track's), Download…, Sync to this device and Share; Web search
  // and Genius search; its cover on this device (a picture chosen here, or the server's read again); what its library
  // allows (Move to another, Replace in Music; decision 0034), each question asked in place; Remove resume point and
  // Delete last.
  import {
    albumCoverUrl,
    albumResume,
    albumView,
    cancelJob,
    deleteAlbum,
    forgetAlbumResume,
    moveAlbum,
    movePreview,
    replaceIncoming,
    getAlbumCovers,
    lookAlbumCovers,
    missingCall,
    stopTask,
    uploadAlbumCover,
    COVER_MAX,
    ApiError,
    setAlbumCover,
    type CoverChoice,
    replacePreview,
    setAlbumFavorite,
    type AlbumMoved,
    type KeepPreview,
    type ReplacePreview,
  } from './api';
  import { albumOfDownload, albumPlay, albumRatings, goToArtist as showArtist, loadAlbumRating, mainArtist, playAlbums, playListedAlbum, rateAlbum } from './albums.svelte';
  import RatingStars from './RatingStars.svelte';
  import { downloads, inApp, removeDownloads } from './app.svelte';
  import { viewGenre, viewLike } from './crates.svelte';
  import { isFavorite, loadAlbumFavorites, setFavorite } from './favourites.svelte';
  import { player } from './live.svelte';
  import { startRadio, type LocalTrack } from './localPlayer.svelte';
  import { isKept, setKept } from './kept.svelte';
  import DownloadStep from './DownloadStep.svelte';
  import MenuTable from './MenuTable.svelte';
  import { inOrder, type EntryId } from './menuTable';
  import { playCollection } from './play.svelte';
  import { DEVICE } from './deviceMusic.svelte';
  import MarkLoader from './MarkLoader.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import PlaylistsStep from './PlaylistsStep.svelte';
  import { TracksToPlaylist, type ToPlaylistStep } from './tracksToPlaylist.svelte';
  import MenuSheet, { type SheetAction } from './MenuSheet.svelte';
  import { playPlaylistFrom } from './output.svelte';
  import { shuffled } from './shuffle';
  import { albumGone, albumSubject, panel, viewPlaylist } from './panel.svelte';
  import { coverKey, localCovers, refreshCover, removeLocalCover, setLocalCover } from './localCovers.svelte';
  import { linkTo, share } from './share';
  import { toast } from './toast.svelte';
  import { askFor, followingNow } from './following.svelte';
  import AlbumCard from './AlbumCard.svelte';
  import Seg from './Seg.svelte';
  import SettingRow from './SettingRow.svelte';
  import Cover from './Cover.svelte';
  import PageLoading from './PageLoading.svelte';
  import Meter from './Meter.svelte';
  import { progressOf, tasks } from './tasks.svelte';
  import { withUndo } from './undo.svelte';
  import { me } from './me.svelte';

  type Ask = 'move' | 'replace' | 'delete' | 'cover' | 'cover-url';
  let {
    source: givenSource,
    album: givenAlbum,
    cover = null,
    tracks: givenTracks = null,
    genres: givenGenres = null,
    offer = null,
    trashDays = 14,
    music = 'Music',
    viewable = true,
    start = 'menu',
    onresumegone,
    ongone,
    onundone,
    onclose,
  }: {
    source: string;
    album: Pick<ListedAlbum, 'id' | 'title' | 'artist'> & Partial<ListedAlbum>;
    /** Its cover; the server's cover of the album when not given. */
    cover?: string | null;
    /** Its tracks in play order and its genres, when the opener has them (the panel); else read as the menu opens. */
    tracks?: LocalTrack[] | null;
    genres?: string[] | null;
    /** What its library allows; none: no library entries (not known yet). */
    offer?: AlbumOffer | null;
    trashDays?: number;
    /** The primary library (Music), by its own name: Replace puts albums in it, and deleting from it asks twice. */
    music?: string;
    /** View in the panel (not from the panel showing it). */
    viewable?: boolean;
    /** The step it opens on: the entries, or straight at the playlists. */
    /** 'cover': straight on Change cover (Health's albums without one). */
    start?: 'menu' | 'lists' | 'cover';
    /** Its resume point was removed here (the panel's Resume goes too). */
    onresumegone?: () => void;
    /** The album left its library (moved, replaced or deleted; not when queued to convert first). */
    ongone?: () => void;
    /** A move's or a delete's Undo went through. */
    onundone?: () => void;
    onclose: () => void;
  } = $props();
  // What the menu was opened on, kept: an opener lets go of it as the menu closes (its {@const} turns null, so its
  // props then read null or throw), while what a tap started still reads it (2026-10-25: Play in a flow view's
  // menu said "Could not start playing"; Play, Shuffle, Play next, Add to queue, Share, the covers all read it after
  // closing).
  let keptAlbum = untrack(() => givenAlbum);
  let keptSource = untrack(() => givenSource);
  const a = $derived.by(() => {
    try {
      return (keptAlbum = givenAlbum ?? keptAlbum);
    } catch {
      return keptAlbum;
    }
  });
  const source = $derived.by(() => {
    try {
      return (keptSource = givenSource ?? keptSource);
    } catch {
      return keptSource;
    }
  });

  // The tracks and genres, read as the menu opens unless given: Shuffle, Resume, Add to playlist and Download need the
  // tracks, Go to genre the genres.
  let read = $state<{ tracks: LocalTrack[]; genres: string[] } | null>(null);
  $effect(() => {
    if (givenTracks && givenGenres) return;
    const id = a.id;
    albumView(source, id).then(
      (r) =>
        (read = {
          tracks: r.rows
            .filter((x) => x.id && !x.missing)
            .map((x) => ({ source: r.source, id: x.id!, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: id, duration_secs: x.duration_secs })),
          genres: r.genres,
        }),
      () => {},
    );
  });
  const tracks = $derived(givenTracks ?? read?.tracks ?? null);
  /** Its tracks kept on this device (the app's downloads): Remove from this device. */
  const kept = $derived(inApp ? downloads.tracks.filter((t) => t.source === source && albumOfDownload(t) === a.id) : []);
  const ids = $derived(tracks?.map((t) => t.id) ?? []);
  const genres = $derived((givenGenres ?? read?.genres ?? []).slice(0, 2));

  // The heart, on the card: the one the panel's top bar shows (lib/favourites.svelte.ts), so both change together.
  $effect(() => {
    loadAlbumFavorites(source, [a.id]);
  });
  const fav = $derived(isFavorite(source, a.id));
  async function toggleFavourite() {
    const on = !fav;
    setFavorite(source, a.id, on);
    navigator.vibrate?.(8);
    try {
      await setAlbumFavorite(source, a.id, on);
    } catch (e) {
      setFavorite(source, a.id, !on);
      toast.fail(e, "The favourite didn't change");
    }
  }

  // Resume (Symfonium's clock): where an album of Cratarr's own library was left (decision 0030).
  let resumeAt = $state<{ track: string; position: number } | null>(null);
  $effect(() => {
    const id = a.id;
    if (!id.startsWith('al-')) return;
    albumResume(id).then((r) => r.track && (resumeAt = { track: r.track, position: r.position_secs ?? 0 }), () => {});
  });
  async function forgetResume() {
    onclose();
    try {
      await forgetAlbumResume(a.id);
      onresumegone?.();
    } catch (e) {
      toast.fail(e, "The resume point wasn't removed");
    }
  }

  // The large actions. Play starts the album as its Play button does; the others work on its tracks.
  async function run(what: () => Promise<unknown>, done?: string) {
    onclose();
    navigator.vibrate?.(8);
    try {
      await what();
      if (done) toast.show({ text: done, tone: 'done' });
    } catch (e) {
      toast.fail(e, 'Could not start playing');
    }
  }
  const listed = () => [{ source, a: a as ListedAlbum }];
  /** Missing tracks open (lib/ReplaceTrackSheet), in the menu's place. */
  let missingOpen = $state(false);
  // Classical… (0.10; administrators'): the record marked classical or not, library-wide, or left to Cratarr's finding;
  // a server that says nothing of it (no `classical`) offers none.
  let classicalOpen = $state(false);
  const CLASSICAL_MARKS: { value: 'auto' | 'classical' | 'not'; label: string }[] = [
    { value: 'auto', label: 'As Cratarr finds it' },
    { value: 'classical', label: 'Classical' },
    { value: 'not', label: 'Not classical' },
  ];
  async function markIt(v: 'auto' | 'classical' | 'not') {
    const album = a as ListedAlbum;
    try {
      const r = await markClassical(source, album.id, v === 'auto' ? null : v);
      album.classical = r.classical;
      album.classical_mark = r.classical_mark ?? undefined;
    } catch (e) {
      toast.fail(e, 'Could not mark it');
    }
  }
  const actions = $derived.by<SheetAction[]>(() => {
    if (ask || exporting || step !== 'menu') return [];
    const t = tracks;
    // Following someone (0.10): its tracks asked of them, next or at the end of their queue.
    if (followingNow()) {
      const askOf = (how: 'next' | 'last') => () => {
        onclose();
        askFor((t ?? []).map((x) => ({ source, id: x.id, name: x.name, artist: x.artist ?? null, album: a.title, album_id: a.id })), how).catch((e) => toast.fail(e, "It couldn't be asked for"));
      };
      return inOrder('album', [
        { icon: 'playNext', label: 'Ask to play next', disabled: !t?.length, onclick: askOf('next') },
        { icon: 'listAdd', label: 'Ask to add to queue', disabled: !t?.length, onclick: askOf('last') },
      ]);
    }
    const at = resumeAt;
    const i = at && t ? t.findIndex((x) => x.id === at.track) : -1;
    const list: SheetAction[] = [
      { icon: 'play', label: 'Play', disabled: albumPlay.starting !== null, onclick: () => run(() => playListedAlbum(source, a as ListedAlbum)) },
    ];
    if (at) list.push({ icon: 'resume', label: 'Resume', disabled: i < 0, onclick: () => run(() => playPlaylistFrom(a.title, source, t!, i, at.position)) });
    list.push(
      {
        icon: 'shuffle',
        label: 'Shuffle',
        disabled: !t?.length,
        // The server's library through the server's choice (decision 0059); the device's own as before.
        onclick: () => run(() => (source === DEVICE ? playPlaylistFrom(a.title, source, shuffled(t!), 0) : playCollection({ kind: 'album', source, id: a.id }, 'shuffle', a.title))),
      },
      { icon: 'playNext', label: 'Play next', onclick: () => run(() => (source === DEVICE ? playAlbums(listed(), 'next') : playCollection({ kind: 'album', source, id: a.id }, 'next', a.title)), 'Plays next.') },
      { icon: 'listAdd', label: 'Add to queue', onclick: () => run(() => (source === DEVICE ? playAlbums(listed(), 'last') : playCollection({ kind: 'album', source, id: a.id }, 'last', a.title)), 'Added to the queue.') },
    );
    // Start radio (decision 0058, as a playlist's): tracks like the album's, from the whole library; on this device.
    if (port().radio && t?.length) list.push({ icon: 'radio', label: 'Start radio', onclick: () => run(() => startRadio(t.map((x) => x.id), a.title)) });
    return inOrder('album', list);
  });

  // Go to artist: the artist's panel, as the track menu's; one for each
  // artist the album is credited to (as Symfonium's). Without the source's list, the credit with a "feat." left out.
  const artistNames = $derived(a.artists?.length ? a.artists : [mainArtist(a.artist) ?? ''].filter(Boolean));
  function goToArtist(name: string) {
    // Read before closing: an opener may drop what it gave the menu on close.
    const from = source;
    onclose();
    showArtist(from, name);
  }
  function goToGenre(name: string) {
    onclose();
    leavePlayer();
    viewGenre(name);
  }
  // Web search and Genius search: the album's artist and title, in a new tab (the app opens it outside).
  function search(site: string) {
    const words = [a.artist, a.title].filter(Boolean).join(' ');
    window.open(`${site}${encodeURIComponent(words)}`, '_blank', 'noopener');
    onclose();
  }

  // Its rating (Symfonium's "Change user rating"): the stars a Subsonic app gives it; a tap on the one it has takes it
  // off. Shown only when its server keeps ratings; the change shows at once, back as it was when refused.
  // The same as its About's (lib/albums albumRatings).
  $effect(() => loadAlbumRating(source, a.id));
  const rating = $derived(albumRatings[`${source}/${a.id}`] ?? null);

  // Share: the album in words and its link here (lib/share.ts).
  function shareIt() {
    onclose();
    share(a.title, [a.artist, a.title].filter(Boolean).join(' – '), linkTo('library', albumSubject(source, a.id)));
  }

  // Its cover on this device (lib/localCovers.svelte.ts): a picture chosen here, or the server's again. Not in
  // Cratarr's own library (the owner, 2026-10-25): there its administrators upload one, for everyone.
  const deviceCovers = $derived(source !== 'cratarr');
  const ownCover = $derived(deviceCovers && !!localCovers[coverKey(source, a.id)]);
  let picker = $state<HTMLInputElement>();
  async function chosen(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    const [src, id, title, here] = [source, a.id, a.title, deviceCovers];
    onclose();
    if (!file) return;
    if (!here) return upload(src, id, title, file);
    try {
      await setLocalCover(src, id, file);
    } catch (e) {
      toast.fail(e, "The cover wasn't changed");
    }
  }
  async function upload(src: string, id: string, title: string, file: File) {
    if (file.size > COVER_MAX) return toast.fail(null, 'That picture is too large (at most 12 MB)');
    try {
      const { undo } = await uploadAlbumCover(src, id, file);
      const again = () => void refreshCover(src, id, albumCoverUrl(src, id));
      again();
      withUndo(`Changed the cover of ${title}.`, undo, { after: again });
    } catch (e) {
      const big = e instanceof ApiError && e.status === 413;
      toast.fail(missingCall(e) || big ? null : e, big ? 'That picture is too large (at most 12 MB)' : missingCall(e) ? "This server can't take a cover yet" : "The cover wasn't uploaded");
    }
  }
  /** The server's cover again, instead of the picture chosen on this device. */
  async function serverCover() {
    onclose();
    try {
      await removeLocalCover(source, a.id);
    } catch (e) {
      toast.fail(e, "The cover wasn't changed");
    }
  }

  // Add to playlist (lib/tracksToPlaylist): all its tracks; the first track's state says whether the album is in Music
  // (only then can it go in a playlist) and which playlists there are.
  const toList = new TracksToPlaylist(
    () => source,
    () => ids,
    'this album',
  );
  $effect(() => {
    if (ids[0]) toList.ask([ids[0]]);
  });
  const addable = $derived(toList.addable);

  // svelte-ignore state_referenced_locally
  let step = $state<ToPlaylistStep>(start === 'cover' ? 'menu' : start);
  $effect(() => {
    if (untrack(() => start) === 'cover') untrack(askCover);
  });
  /** Download… (decision 0058, amended 0.10): its tracks, in lib/DownloadStep. */
  let exporting = $state(false);
  // Opened at the playlists for an album outside Music: there is nothing to add it to.
  $effect(() => {
    if (step === 'lists' && toList.first && !toList.first.addable) {
      toast.fail(null, "This album isn't in Music, so it can't go in a playlist");
      onclose();
    }
  });

  // What its library allows, each asked in place. Deleting from the primary library asks once more.
  // svelte-ignore state_referenced_locally
  const fromName = offer?.from?.name ?? 'Incoming';
  // svelte-ignore state_referenced_locally
  const primary = offer?.from?.use === 'music';
  // Said in the question when the album leaves Incoming and no Lidarr is set to be told to stop fetching it.
  const NO_LIDARR = "Lidarr isn't set up in Settings, so it may fetch this album again.";
  let ask = $state<false | Ask | 'really'>(false);
  /** The move's library. */
  let to = $state<LibraryInfo | null>(null);
  let keep = $state<KeepPreview | null>(null);
  let swap = $state<ReplacePreview | null>(null);
  /** Replace's cover (the owner, 2026-10-25): the album's own kept, or the new copy's. */
  let coverWay = $state<'keep' | 'new'>('keep');
  /** Change cover: the album's own and those to choose from (null while asked). */
  type Covers = { current: CoverChoice | null; candidates: CoverChoice[] };
  let covers = $state<Covers | null>(null);
  // Looked for as a task (the owner, 2026-10-25: Discover's progress, how many from which source): its step under the
  // loader, its result the covers. A server before it: asked in one go, as before.
  let coverTask = $state<number | null>(null);
  const coverLook = $derived(coverTask !== null ? tasks.of(`covers:${a.id}`) : null);
  const coverProgress = $derived(coverLook && coverLook.finished === null ? progressOf(coverLook) : null);
  // As they are found (the owner, 2026-10-25): what the running look has so far, its result once it ends.
  const coversSoFar = $derived(covers ?? ((coverLook?.result as Covers | null | undefined) || null));
  /** Stops the look (a cover chosen, or the menu left): no more of Discogs' pace spent on it. */
  function stopLook() {
    const id = coverTask;
    if (id === null || !coverLook || coverLook.finished !== null) return;
    coverTask = null;
    stopTask(id).catch(() => {});
  }
  $effect(() => () => untrack(stopLook));
  $effect(() => (ask === 'cover' ? tasks.follow() : undefined));
  $effect(() => {
    const t = coverLook;
    if (!t || t.id !== coverTask || t.finished === null) return;
    coverTask = null;
    if (t.error) {
      toast.show({ text: t.error, tone: 'error' });
      onclose();
    } else covers = t.result as Covers;
  });
  async function askCover() {
    ask = 'cover';
    covers = null;
    try {
      const { task } = await lookAlbumCovers(source, a.id);
      tasks.begun(task);
      coverTask = task.id;
    } catch (e) {
      if (!missingCall(e)) {
        toast.fail(e, 'Could not find covers');
        return onclose();
      }
      try {
        covers = await getAlbumCovers(source, a.id);
      } catch (e2) {
        toast.fail(e2, 'Could not find covers');
        onclose();
      }
    }
  }
  /** Change cover's From an address. */
  let coverUrl = $state('');
  /** The cover being made the album's: only its card turns busy (the owner, 2026-10-25). */
  let settingCover = $state<string | null>(null);
  const setCover = (c: CoverChoice) => {
    if (working) return;
    settingCover = c.url;
    stopLook();
    return act(async () => {
      const { undo } = await setAlbumCover(source, a.id, c.url);
      onclose();
      const again = () => void refreshCover(source, a.id, albumCoverUrl(source, a.id));
      again();
      withUndo(`Changed the cover of ${a.title}.`, undo, { after: again });
    }, 'Could not change the cover').finally(() => (settingCover = null));
  };
  const size = (c: CoverChoice) => (c.width && c.height ? `${c.width} × ${c.height}` : '');
  let year = $state('');
  let working = $state(false);

  async function askMove(target: LibraryInfo) {
    ask = 'move';
    to = target;
    keep = null;
    year = '';
    try {
      keep = await movePreview(source, a.id, target.id);
    } catch (e) {
      toast.fail(e, 'Could not work out the names');
      onclose();
    }
  }
  async function askReplace() {
    ask = 'replace';
    swap = null;
    coverWay = 'keep';
    year = '';
    try {
      swap = await replacePreview(source, a.id);
    } catch (e) {
      toast.fail(e, 'Could not pair the tracks');
      onclose();
    }
  }
  // With a year typed in, show what the names would be.
  async function tryYear() {
    if (!/^\d{4}$/.test(year.trim())) return;
    try {
      if (ask === 'replace') swap = await replacePreview(source, a.id, year.trim());
      else if (to) keep = await movePreview(source, a.id, to.id, year.trim());
    } catch {
      /* keep the question open */
    }
  }
  async function act(work: () => Promise<void>, failure: string) {
    if (working) return;
    working = true;
    try {
      await work();
    } catch (e) {
      toast.fail(e, failure);
    } finally {
      working = false;
      onclose();
    }
  }
  // Done: the album leaves its library. Queued (hi-res files to convert first): its row shows the progress instead,
  // and the queue says when it is done.
  function moved(r: AlbumMoved, words: string) {
    if (r.queued) {
      jobs.poke();
      return;
    }
    gone();
    withUndo(words, [r.undo], { undone: `${r.title} is back in ${fromName}.`, after: onundone });
  }
  function gone() {
    ongone?.();
    albumGone(source, a.id);
  }
  const moveIt = () =>
    act(async () => {
      if (!to) return;
      const name = to.name;
      moved(await moveAlbum(source, a.id, to.id, year.trim() || undefined), `Moved ${a.title} to ${name}.`);
    }, 'Could not move the album');
  /** `carry`: Replace and carry over (0.9.7): what Cratarr knew of the album (its ids, numbers, cover) into the new copy. */
  const replaceIt = (carry = false) =>
    act(async () => {
      const r = await replaceIncoming(source, a.id, year.trim() || undefined, swap?.covers ? coverWay : undefined, carry);
      moved(r, `Replaced ${r.title} in ${music}.`);
    }, 'Could not replace the album');
  /** The tag differences shown at first (the rest behind "And N more"). */
  const DIFF_SHOWN = 12;
  let diffAll = $state(false);
  const remove = () =>
    act(async () => {
      const { undo, title } = await deleteAlbum(source, a.id);
      gone();
      withUndo(`Deleted ${title}.`, [undo], { undone: `${title} is back in ${fromName}.`, after: onundone });
    }, 'Could not delete the album');
  async function cancel(id: number) {
    onclose();
    try {
      await cancelJob(id);
      await jobs.poke();
    } catch (e) {
      toast.fail(e, 'Could not cancel it');
    }
  }
</script>

{#snippet lookProgress()}
  {#if coverProgress}{#if coverProgress.value !== null}<Meter label={coverProgress.line} value={coverProgress.value} />{:else}<span class="look-line">{coverProgress.line}</span>{/if}{/if}
{/snippet}

<!-- Missing tracks (0.9.8, lib/ReplaceTrackSheet): the menu gives way to it. -->
{#snippet entry(e: EntryId)}
  {#if e === 'view'}
    {#if viewable && panel.viewed !== albumSubject(source, a.id)}
      <MenuEntry
        icon="squareArrowOutUpRight"
        label="View"
        onclick={() => {
          onclose();
          viewPlaylist(albumSubject(source, a.id));
        }}
      />
    {/if}
  {:else if e === 'goArtist'}
    {#each artistNames as name (name)}<MenuEntry icon="user" label="Go to {name}" onclick={() => goToArtist(name)} />{/each}
  {:else if e === 'goGenre'}
    {#each genres as g (g)}<MenuEntry icon="music" label="Go to {g}" onclick={() => goToGenre(g)} />{/each}
  {:else if e === 'soundsLike'}
    <!-- Sounds like it: a crate seeded with the album's songs (Cratarr's own, which AudioMuse hears). -->
    {#if source === 'cratarr'}<MenuEntry
        icon="music"
        label="Sounds like this"
        onclick={() => {
          // Read before closing: closing lets go of the album.
          const id = a.id;
          onclose();
          leavePlayer();
          viewLike(id);
        }}
      />{/if}
  {:else if e === 'favourite'}
    <MenuEntry icon="heart" label="Favourite" switched={fav} onclick={toggleFavourite} />
  {:else if e === 'rating'}
    {#if rating !== null}
      {@const r = rating}
      <div class="entry rating">
        <span>Rating</span>
        <RatingStars value={r} onrate={(n) => rateAlbum(source, a.id, n)} />
      </div>
    {/if}
  {:else if e === 'addToPlaylist'}
    {#if addable}<MenuEntry icon="playlists" label="Add to playlist" onclick={() => (step = 'lists')} />{/if}
  {:else if e === 'download'}
    {#if tracks?.length && source !== DEVICE}<MenuEntry icon="arrowDownToLine" label="Download…" onclick={() => (exporting = true)} />{/if}
  {:else if e === 'keep'}
    {#if inApp}
      <!-- Kept on this device (decision 0038): stays in step offline; Download… to this device is a one-time copy. -->
      {@const on = isKept('album', source, a.id)}
      <MenuEntry icon="download" label="Sync to this device" switched={on} onclick={() => setKept('album', source, a.id, !on, a.title)} />
    {/if}
  {:else if e === 'removeFromDevice'}
    <!-- Beside what put them there (the owner, 2026-10-26: removing was out of sight, last in the menu). -->
    {#if kept.length}
      {@const ids = kept.map((t) => t.download.id)}
      <MenuEntry
        icon="trash"
        label="Remove from this device"
        danger
        onclick={() => {
          onclose();
          removeDownloads(ids);
        }}
      />
    {/if}
  {:else if e === 'share'}
    <MenuEntry icon="share" label="Share" onclick={shareIt} />
  {:else if e === 'webSearch'}
    <MenuEntry icon="search" label="Web search" onclick={() => search('https://duckduckgo.com/?q=')} />
  {:else if e === 'geniusSearch'}
    <MenuEntry icon="search" label="Genius search" onclick={() => search('https://genius.com/search?q=')} />
  {:else if e === 'changeCover'}
    <!-- Its administrators' (decision 0051): the cover for everyone. -->
    {#if me.admin && source === 'cratarr'}<MenuEntry icon="image" label="Change cover" onclick={askCover} />{/if}
  {:else if e === 'missingTracks'}
    <!-- Its release's tracks, the missing ones found one at a time (0.9.8, single-track replacement). -->
    {#if me.admin && source === 'cratarr'}<MenuEntry icon="search" label="Missing tracks…" onclick={() => (missingOpen = true)} />{/if}
  {:else if e === 'classical'}
    {#if me.admin && (a as ListedAlbum).classical !== undefined}
      <MenuEntry icon="music" label="Classical…" onclick={() => (classicalOpen = true)} />
    {/if}
  {:else if e === 'deviceCover'}
    {#if !(me.admin && source === 'cratarr') && deviceCovers}<MenuEntry icon="image" label="Change cover on this device" onclick={() => picker?.click()} />{/if}
  {:else if e === 'serverCover'}
    <!-- No Read the cover again (the owner, 2026-10-26): a cover's address carries its stamp (lib/coverStamps), so a
         changed one shows by itself. Only a picture chosen on this device is undone here. -->
    {#if ownCover}<MenuEntry icon="imageOff" label="Use the server's cover" onclick={serverCover} />{/if}
  {:else if e === 'cancelConversion'}
    {#if me.admin && offer && offer.job !== null}
      {@const job = offer.job}
      <MenuEntry icon="close" label="Cancel conversion" onclick={() => cancel(job)} />
    {/if}
  {:else if e === 'moveTo'}
    {#if me.admin && offer && offer.job === null}
      {#each offer.targets as t (t.id)}<MenuEntry icon="intoBox" label={`Move to ${t.name}`} onclick={() => askMove(t)} />{/each}
    {/if}
  {:else if e === 'replaceIn'}
    {#if me.admin && offer && offer.job === null && offer.replaceable}<MenuEntry icon="swap" label={`Replace in ${music}`} onclick={askReplace} />{/if}
  {:else if e === 'removeResume'}
    {#if resumeAt}<MenuEntry icon="resume" label="Remove resume point" onclick={forgetResume} />{/if}
  {:else if e === 'deleteAlbum'}
    {#if me.admin && offer && offer.job === null && offer.deletable}<MenuEntry icon="trash" label="Delete album" danger onclick={() => (ask = 'delete')} />{/if}
  {/if}
{/snippet}

{#if missingOpen}
  <ReplaceTrackSheet {source} album={{ id: a.id, title: a.title }} {onclose} />
{:else if classicalOpen}
  <ChoiceSheet
    title="Classical"
    options={CLASSICAL_MARKS}
    value={(a as ListedAlbum).classical_mark ?? 'auto'}
    onchange={(v) => void markIt(v)}
    onclose={() => ((classicalOpen = false), onclose())}
  />
{:else}
<MenuSheet
  label={a.title}
  {onclose}
  images={[cover ?? albumCoverUrl(source, a.id)]}
  title={a.title}
  chips={[a.artist ?? 'Unknown artist']}
  heart={ask || exporting || step !== 'menu' ? null : { on: fav, toggle: toggleFavourite }}
  {actions}
  beside={ask === 'cover' && coverProgress ? lookProgress : null}
  end={ask === 'cover'
    ? [
        ...(coverProgress ? [{ icon: 'search' as const, label: 'Looking for covers', busy: true, onclick: () => {} }] : []),
        { icon: 'link', label: 'From an address', onclick: () => ((coverUrl = ''), (ask = 'cover-url')) },
        { icon: 'upload', label: 'Upload cover', onclick: () => picker?.click() },
      ]
    : null}
  onback={ask === 'cover-url' ? () => (ask = 'cover') : ask ? () => (ask = false) : exporting ? () => (exporting = false) : step === 'menu' ? null : () => (step = step === 'again' ? 'lists' : 'menu')}
  step={ask || (exporting ? 'export' : step)}
>
  {#if exporting && tracks}
    <DownloadStep what={{ kind: 'tracks', name: a.title, tracks: tracks.map((x) => ({ source: x.source, id: x.id })) }} {tracks} {onclose} />
  {:else if ask === 'move'}
    <div class="form">
      {#if !keep}
        <div class="loading"><MarkLoader size={40} /></div>
      {:else if keep.refused?.kind === 'needs_year'}
        <p class="ask">The tags don't say when {a.title} came out. Which year?</p>
        <input class="year" inputmode="numeric" maxlength="4" placeholder="Year" aria-label="Release year" bind:value={year} oninput={tryYear} />
      {:else if keep.refused}
        <p class="ask">Can't move {a.title}: {keep.refused.detail}</p>
      {:else}
        <p class="ask">
          Move {a.title} into {to?.name} as <strong>{keep.folder}</strong> ({keep.files} {keep.files === 1 ? 'file' : 'files'})?{#if keep.convert}
            {` ${keep.convert} hi-res ${keep.convert === 1 ? 'file is' : 'files are'} converted to 16-bit/44.1 kHz first.`}{/if}{#if !keep.acquisition && !primary}
            {` ${NO_LIDARR}`}{/if}
        </p>
      {/if}
      <div class="actions">
        {#if keep && (!keep.refused || keep.refused.kind === 'needs_year')}
          <button class="primary" disabled={working || !!keep.refused} aria-busy={working} onclick={moveIt}>Move to {to?.name}</button>
        {/if}
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
  {:else if ask === 'replace'}
    <div class="form">
      {#if !swap}
        <div class="loading"><MarkLoader size={40} /></div>
      {:else if swap.refused?.kind === 'needs_year'}
        <p class="ask">The tags don't say when {a.title} came out. Which year?</p>
        <input class="year" inputmode="numeric" maxlength="4" placeholder="Year" aria-label="Release year" bind:value={year} oninput={tryYear} />
      {:else if swap.refused?.kind === 'unmatched'}
        <p class="ask">Can't replace {swap.old}: no match for {swap.refused.detail.join(', ')}.</p>
      {:else if swap.refused}
        <p class="ask">Can't replace {swap.old}: {swap.refused.detail}</p>
      {:else}
        <p class="ask">
          Replace <strong>{swap.old}</strong> with this copy ({swap.tracks} {swap.tracks === 1 ? 'track' : 'tracks'}{swap.added
            ? `, ${swap.added} new`
            : ''}){#if swap.folder !== swap.old}, as <strong>{swap.folder}</strong>{/if}? The old copy stays in the trash for {trashDays}
          {trashDays === 1 ? 'day' : 'days'}.{#if swap.convert}
            {` ${swap.convert} hi-res ${swap.convert === 1 ? 'file is' : 'files are'} converted to 16-bit/44.1 kHz first.`}{/if}{#if !swap.acquisition}
            {` ${NO_LIDARR}`}{/if}
        </p>
        <!-- Its cover (the owner, 2026-10-25): the one it has, or the new copy's; both shown. -->
        {#if swap.covers && (swap.covers.current || swap.covers.new)}
          {@const c = swap.covers}
          <div class="covers">
            <figure class:on={coverWay === 'keep'}><span class="thumb"><Cover src={c.current} /></span><figcaption>Current</figcaption></figure>
            <figure class:on={coverWay === 'new'}><span class="thumb"><Cover src={c.new} /></span><figcaption>New</figcaption></figure>
          </div>
          <SettingRow label="Cover"><Seg label="Cover" options={[{ value: 'keep', label: 'Keep' }, { value: 'new', label: 'New' }]} bind:value={coverWay} /></SettingRow>
        {/if}
        <!-- What changes in the tags (0.9.7; the owner, 2026-10-26: only the differences), what the new copy lacks, and what
             carrying over brings along. -->
        {#if swap.lost?.length}<p class="ask lost">The new copy lacks: {swap.lost.join(', ')}.</p>{/if}
        {#if swap.diff?.length}
          <ul class="diff">
            {#each diffAll ? swap.diff : swap.diff.slice(0, DIFF_SHOWN) as d, i (i)}
              <li>
                <small>{d.track ?? 'Every track'} · {d.field}</small>
                <span><span class="was">{d.old || '–'}</span> → <b>{d.new || '–'}</b></span>
              </li>
            {/each}
          </ul>
          {#if !diffAll && swap.diff.length > DIFF_SHOWN}<button class="more" onclick={() => (diffAll = true)}>And {swap.diff.length - DIFF_SHOWN} more</button>{/if}
        {/if}
        {#if swap.carry?.length}<p class="ask carried">Carried over: {swap.carry.join(', ')}.</p>{/if}
      {/if}
      <div class="actions">
        {#if swap && (!swap.refused || swap.refused.kind === 'needs_year')}
          {#if swap.carry?.length}
            <button class="primary" disabled={working || !!swap.refused} aria-busy={working} onclick={() => replaceIt(true)}>Replace and carry over</button>
            <button disabled={working || !!swap.refused} onclick={() => replaceIt()}>Replace only</button>
          {:else}
            <button class="primary" disabled={working || !!swap.refused} aria-busy={working} onclick={() => replaceIt()}>Replace</button>
          {/if}
        {/if}
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
  {:else if ask === 'cover-url'}
    <!-- A picture's address for its cover (the owner, 2026-10-25: beside Upload, top right of Change cover). -->
    <div class="form">
      <p class="ask">The picture's address</p>
      <input class="url" type="url" inputmode="url" placeholder="https://…" aria-label="Picture address" bind:value={coverUrl} />
      <div class="actions">
        <button class="solid" disabled={working || !/^https?:\/\/\S+$/.test(coverUrl.trim())} aria-busy={working} onclick={() => setCover({ url: coverUrl.trim(), width: null, height: null })}>Use it</button>
        <button onclick={() => (ask = 'cover')}>Cancel</button>
      </div>
    </div>
  {:else if ask === 'cover'}
    <!-- Change cover (0.9.5): the album's own first, then Discogs' and the Cover Art Archive's, as an artist's albums;
         a tap makes it the cover, inside every file and beside them (Undo brings the old one back). -->
    <!-- A few seconds with Discogs (its master and up to six releases, at its pace): the covers as they are found,
         how far beside the back button, the busy ring at the top right (the owner, 2026-10-25). -->
    {#if !coversSoFar}
      {#if !coverProgress}<PageLoading />{/if}
    {:else if covers && !covers.candidates.length}
      <p class="ask pad">No other covers found.</p>
    {:else}
      {@const list = coversSoFar}
      <ul class="cover-grid">
        {#if list.current}<li><AlbumCard source="cover" album={{ id: 'current', title: 'Current', year: null }} cover={list.current.url} lines={[size(list.current)]} onopen={() => onclose()} /></li>{/if}
        {#each list.candidates as c (c.url)}
          <li><AlbumCard source="cover" album={{ id: c.url, title: c.what ?? (c.from === 'discogs' ? 'Discogs' : 'Cover Art Archive'), year: null }} cover={c.url} lines={[size(c), c.from === 'discogs' ? 'Discogs' : 'Cover Art Archive']} busy={settingCover === c.url} onopen={() => setCover(c)} /></li>
        {/each}
      </ul>
    {/if}
  {:else if ask === 'delete'}
    <div class="form">
      <p class="ask">Delete {a.title}? It stays in the trash for {trashDays} {trashDays === 1 ? 'day' : 'days'}.</p>
      <div class="actions">
        <button class="danger solid" disabled={working} aria-busy={working} onclick={() => (primary ? (ask = 'really') : remove())}>Delete album</button>
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
  {:else if ask === 'really'}
    <!-- The primary library's extra step (decision 0034). -->
    <div class="form">
      <p class="ask">{a.title} is in {music}, your main library. Delete it from there?</p>
      <div class="actions">
        <button class="danger solid" disabled={working} aria-busy={working} onclick={remove}>Delete from {music}</button>
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
  {:else if step === 'menu'}
    <!-- The shared groups, in the table's order (decision 0058, lib/menuTable). -->
    <MenuTable kind="album" {entry}>
      <input class="picker" type="file" accept="image/*" bind:this={picker} onchange={chosen} />
    </MenuTable>
  {:else}
    <PlaylistsStep to={toList} bind:step {onclose} />
  {/if}
</MenuSheet>
{/if}

<style>
  /* The rating's card: its word, then five stars, filled up to the rating in the accent. */
  .rating {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 60px;
    padding: 0 12px 0 20px;
    border-radius: 20px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    font-size: 16px;
  }

  .rating > span:first-child {
    flex: 1;
  }

  .picker {
    display: none;
  }

  /* Change cover's choices: as an artist's albums, a grid of covers. */
  .cover-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 16px 12px;
    margin: 0;
    padding: 0 16px 12px;
    list-style: none;
  }

  .pad {
    padding: 0 16px;
  }

  /* Replace's two covers, the chosen one ringed in the accent. */
  .covers {
    display: flex;
    gap: 12px;
  }

  .covers figure {
    margin: 0;
    opacity: 0.55;
  }

  .covers figure.on {
    opacity: 1;
  }

  .covers .thumb {
    display: block;
    width: 88px;
    height: 88px;
    overflow: hidden;
    border-radius: 8px;
  }

  .covers figure.on .thumb {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .covers figcaption {
    margin-top: 4px;
    color: var(--muted);
    font-size: 13px;
  }

  .form {
    display: grid;
    gap: 12px;
    padding: 0 16px 8px;
  }

  .ask {
    margin: 0;
    font-size: 0.92rem;
  }

  /* Replace's tag differences (0.9.7): each as it was and as it comes, what the new copy lacks in amber. */
  .ask.lost {
    color: var(--warn-ink);
  }

  .ask.carried {
    color: var(--muted);
  }

  .diff {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 0.88rem;
  }

  .diff li {
    display: grid;
    gap: 2px;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .diff small,
  .diff .was {
    color: var(--muted);
  }

  .more {
    justify-self: start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font: inherit;
    font-size: 0.88rem;
    cursor: pointer;
  }

  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .actions button {
    min-height: 40px;
    padding: 0 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 999px;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .actions button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .actions button.danger.solid {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
    font-weight: 600;
  }

  .actions button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .year {
    min-height: 40px;
    width: 7em;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  /* From an address: the whole width, as the year's field otherwise. */
  .url {
    box-sizing: border-box;
    width: 100%;
    min-height: 40px;
    padding: 0 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    font: inherit;
    color: inherit;
  }

  /* The look's step without a count, beside the back button: as Meter's line. */
  .look-line {
    color: var(--accent-ink);
    font-size: 0.8rem;
  }

  /* A question's answer being worked out: the loading sign, small (as the track menu's). */
  .loading {
    display: grid;
    place-items: center;
    padding: 16px 0;
  }
</style>
