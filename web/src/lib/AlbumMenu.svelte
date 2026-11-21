<script lang="ts" module>
  import { openRadio } from './radioAsk.svelte';
  import { leavePlayer } from './navStep.svelte';
  import { port } from './player/port.svelte';
  import { untrack } from 'svelte';
  import type { ListedAlbum } from './api';
</script>

<script lang="ts">
  // An album's menu where it is no Cratarr record (another server's album, this device's own music), and a Cratarr record's
  // out of reach until the server marks what works away (lib/sheet/Record.svelte, RS01, is every Cratarr record's sheet:
  // SCENARIOS-DC1 RS12, DL1). After Symfonium's album sheet: the album at the top (its heart there), the large actions (Play,
  // Resume, Shuffle, Play next, Add to queue), then its entries on rounded cards in the table's order (lib/menuTable):
  // where to go (View, each of its artists, its genres); its rating; Add to playlist (all its tracks, the playlists as
  // toggle cards in a step, as a track's), Download…, Sync to this device and Share; Web search and Genius search; its
  // cover on this device (a picture chosen here, or the server's read again); Remove resume point last.
  import { albumCoverUrl, albumResume, albumView, forgetAlbumResume, setAlbumFavorite } from './api';
  import { albumOfDownload, albumPlay, albumRatings, goToArtist as showArtist, loadAlbumRating, mainArtist, playAlbums, playListedAlbum, rateAlbum } from './albums.svelte';
  import RatingStars from './RatingStars.svelte';
  import { downloads, inApp, removeDownloads } from './app.svelte';
  import { viewGenre } from './crates.svelte';
  import { isFavorite, loadAlbumFavorites, setFavorite } from './favourites.svelte';
  import { type LocalTrack } from './localPlayer.svelte';
  import { isKept, setKept } from './kept.svelte';
  import DownloadStep from './DownloadStep.svelte';
  import MenuTable from './MenuTable.svelte';
  import { ICON_OF, inOrder, viewName, type EntryId } from './menuTable';
  import { offeredNotHad } from './recordNotHad';
  import { playCollection } from './play.svelte';
  import { DEVICE } from './deviceMusic.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import PlaylistsStep from './PlaylistsStep.svelte';
  import { TracksToPlaylist, type ToPlaylistStep } from './tracksToPlaylist.svelte';
  import MenuSheet, { type SheetAction } from './MenuSheet.svelte';
  import { recordWord } from './recordTypeRules';
  import { playPlaylistFrom } from './output.svelte';
  import { shuffled } from './shuffle';
  import { albumSubject, panel, viewOver } from './panel.svelte';
  import { coverKey, localCovers, removeLocalCover, setLocalCover } from './localCovers.svelte';
  import { linkTo } from './addresses';
  import { recordWords, share } from './share';
  import { toast } from './toast.svelte';
  import { askFor, followingNow } from './following.svelte';

  let {
    source: givenSource,
    album: givenAlbum,
    cover = null,
    tracks: givenTracks = null,
    genres: givenGenres = null,
    viewable = true,
    notHad = false,
    start = 'menu',
    onresumegone,
    onclose,
  }: {
    source: string;
    album: Pick<ListedAlbum, 'id' | 'title' | 'artist'> & Partial<ListedAlbum>;
    /** Its cover; the server's cover of the album when not given. */
    cover?: string | null;
    /** Its tracks in play order and its genres, when the opener has them (the panel); else read as the menu opens. */
    tracks?: LocalTrack[] | null;
    genres?: string[] | null;
    /** View in the panel (not from the panel showing it). */
    viewable?: boolean;
    /** A record not had (W9, lib/recordNotHad): only what needs no copy of it; no heart, no large actions. */
    notHad?: boolean;
    /** The step it opens on: the entries, or straight at the playlists. */
    start?: 'menu' | 'lists';
    /** Its resume point was removed here (the panel's Resume goes too). */
    onresumegone?: () => void;
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
  // Named by its release type: this single (lib/recordTypeRules); its View the same (F35.4).
  const word = $derived(recordWord(a.type, a.secondary));
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
    if (!notHad) loadAlbumFavorites(source, [a.id]);
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
    if (notHad || !id.startsWith('al-')) return;
    albumResume(id).then((r) => r.track && (resumeAt = { track: r.track, position: r.position_secs ?? 0 }), () => {});
  });
  async function forgetResume() {
    // What it does once the server answered, fixed as it is chosen: the menu closes at once (tests/menus-run-first).
    const [id, gone] = [a.id, onresumegone];
    onclose();
    try {
      await forgetAlbumResume(id);
      gone?.();
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
  const actions = $derived.by<SheetAction[]>(() => {
    if (notHad || exporting || step !== 'menu') return [];
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
    if (port().radio && t?.length) {
      const seed = t.map((x) => x.id);
      list.push({ icon: 'radio', label: 'Start radio', onclick: () => (onclose(), openRadio({ name: a.title, kind: 'record', within: source === DEVICE ? null : { kind: 'album', source, id: a.id }, seed: async () => seed })) });
    }
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
    share(a.title, recordWords(a.artist, a.title), linkTo('library', albumSubject(source, a.id)));
  }

  // Its cover on this device (lib/localCovers.svelte.ts): a picture chosen here, or the server's again. Not a Cratarr record's
  // (the owner, 2026-10-25): there its administrators change it, for everyone (RS01's Change cover).
  const deviceCovers = $derived(source !== 'cratarr');
  const ownCover = $derived(deviceCovers && !!localCovers[coverKey(source, a.id)]);
  let picker = $state<HTMLInputElement>();
  async function chosen(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    const [src, id] = [source, a.id];
    onclose();
    if (!file) return;
    try {
      await setLocalCover(src, id, file);
    } catch (e) {
      toast.fail(e, "The cover wasn't changed");
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
    // svelte-ignore state_referenced_locally
    `this ${word}`,
  );
  $effect(() => {
    if (ids[0]) toList.ask([ids[0]]);
  });
  const addable = $derived(toList.addable);

  // svelte-ignore state_referenced_locally
  let step = $state<ToPlaylistStep>(start);
  /** Download… (decision 0058, amended 0.10): its tracks, in lib/DownloadStep. */
  let exporting = $state(false);
  // Opened at the playlists for one that cannot go in them: there is nothing to add it to. Since 2026-10-15 every
  // library's tracks can; not only where no Music library is set up (playlists are kept there) or its library is not
  // in use (the owner, 2026-11-02: it said "isn't in Music", untrue of Incoming).
  $effect(() => {
    if (step === 'lists' && toList.first && !toList.first.addable) {
      toast.fail(null, `This ${word} can't go in a playlist: its library isn't in use, or no Music library is set up`);
      onclose();
    }
  });
</script>

{#snippet entry(e: EntryId)}
  {#if notHad && !offeredNotHad(e)}
    <!-- A record not had: nothing of a copy's (lib/recordNotHad). -->
  {:else if e === 'view'}
    {#if viewable && panel.viewed !== albumSubject(source, a.id)}
      <MenuEntry
        icon={ICON_OF.view}
        label={viewName('album', a.type, a.secondary)}
        onclick={() => {
          onclose();
          viewOver(albumSubject(source, a.id));
        }}
      />
    {/if}
  {:else if e === 'goArtist'}
    {#each artistNames as name (name)}<MenuEntry icon={ICON_OF.artist} label="Go to {name}" onclick={() => goToArtist(name)} />{/each}
  {:else if e === 'goGenre'}
    {#each genres as g (g)}<MenuEntry icon={ICON_OF.genre} label="Go to {g}" onclick={() => goToGenre(g)} />{/each}
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
  {:else if e === 'deviceCover'}
    {#if deviceCovers}<MenuEntry icon="image" label="Change cover on this device" onclick={() => picker?.click()} />{/if}
  {:else if e === 'serverCover'}
    <!-- No Read the cover again (the owner, 2026-10-26): a cover's address carries its stamp (lib/coverStamps), so a
         changed one shows by itself. Only a picture chosen on this device is undone here. -->
    {#if ownCover}<MenuEntry icon="imageOff" label="Use the server's cover" onclick={serverCover} />{/if}
  {:else if e === 'removeResume'}
    {#if resumeAt}<MenuEntry icon="resume" label="Remove resume point" onclick={forgetResume} />{/if}
  {/if}
{/snippet}

<MenuSheet
  label={a.title}
  {onclose}
  images={[cover ?? albumCoverUrl(source, a.id)]}
  title={a.title}
  chips={[a.artist ?? 'Unknown artist']}
  heart={notHad || exporting || step !== 'menu' ? null : { on: fav, toggle: toggleFavourite }}
  {actions}
  onback={exporting ? () => (exporting = false) : step === 'menu' ? null : () => (step = step === 'again' ? 'lists' : 'menu')}
  step={exporting ? 'export' : step}
>
  {#if exporting && tracks}
    <DownloadStep what={{ kind: 'tracks', name: a.title, tracks: tracks.map((x) => ({ source: x.source, id: x.id })) }} {tracks} {onclose} />
  {:else if step === 'menu'}
    <!-- The shared groups, in the table's order (decision 0058, lib/menuTable). -->
    <MenuTable kind="album" {entry}>
      <input class="picker" type="file" accept="image/*" bind:this={picker} onchange={chosen} />
    </MenuTable>
  {:else}
    <PlaylistsStep to={toList} bind:step {onclose} />
  {/if}
</MenuSheet>

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
</style>
