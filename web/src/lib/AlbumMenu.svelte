<script lang="ts" module>
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
  // Resume, Shuffle, Play next, Add to queue), then its entries on rounded cards, related ones joined: where to go
  // (View, each of its artists, its genres); its rating; Add to playlist (all its tracks, the playlists as toggle cards
  // in a step, as a track's), Download and Share; what its library allows (Move to another, Replace in Music, Delete;
  // decision 0034), each question asked in place; its cover on this device (a picture chosen here, or the server's read
  // again); Remove resume point, Web search and Genius search.
  import {
    albumCoverUrl,
    albumResume,
    albumView,
    getAlbumRating,
    serverCoverUrl,
    setAlbumRating,
    cancelJob,
    deleteAlbum,
    forgetAlbumResume,
    moveAlbum,
    movePreview,
    replaceIncoming,
    replacePreview,
    setAlbumFavorite,
    type AlbumMoved,
    type KeepPreview,
    type ReplacePreview,
  } from './api';
  import { albumPlay, goToArtist as showArtist, mainArtist, playAlbums, playListedAlbum } from './albums.svelte';
  import { download, downloads, inApp, removeDownloads } from './app.svelte';
  import { viewGenre } from './crates.svelte';
  import { isFavorite, loadAlbumFavorites, setFavorite } from './favourites.svelte';
  import { player } from './live.svelte';
  import type { LocalTrack } from './localPlayer.svelte';
  import MenuCards from './MenuCards.svelte';
  import MenuEntry from './MenuEntry.svelte';
  import MenuGroup from './MenuGroup.svelte';
  import PlaylistsStep from './PlaylistsStep.svelte';
  import { TracksToPlaylist, type ToPlaylistStep } from './tracksToPlaylist.svelte';
  import MenuSheet, { type SheetAction } from './MenuSheet.svelte';
  import { playPlaylistFrom } from './output.svelte';
  import { albumGone, albumSubject, viewPlaylist } from './panel.svelte';
  import { coverKey, localCovers, refreshCover, removeLocalCover, setLocalCover } from './localCovers.svelte';
  import { linkTo, share } from './share';
  import Icon from './Icon.svelte';
  import { toast } from './toast.svelte';
  import { withUndo } from './undo.svelte';

  type Ask = 'move' | 'replace' | 'delete';
  let {
    source,
    album: a,
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
    start?: 'menu' | 'lists';
    /** Its resume point was removed here (the panel's Resume goes too). */
    onresumegone?: () => void;
    /** The album left its library (moved, replaced or deleted; not when queued to convert first). */
    ongone?: () => void;
    /** A move's or a delete's Undo went through. */
    onundone?: () => void;
    onclose: () => void;
  } = $props();

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
  const kept = $derived(inApp ? downloads.tracks.filter((t) => t.source === source && t.album_id === a.id) : []);
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
  const actions = $derived.by<SheetAction[]>(() => {
    if (ask || step !== 'menu') return [];
    const t = tracks;
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
        onclick: () => run(() => playPlaylistFrom(a.title, source, [...t!].sort(() => Math.random() - 0.5), 0)),
      },
      { icon: 'playNext', label: 'Play next', onclick: () => run(() => playAlbums(listed(), 'next'), 'Plays next.') },
      { icon: 'listAdd', label: 'Add to queue', onclick: () => run(() => playAlbums(listed(), 'last'), 'Added to the queue.') },
    );
    return list;
  });

  // Go to artist: the artist's panel, as the track menu's; one for each
  // artist the album is credited to (as Symfonium's). Without the source's list, the credit with a "feat." left out.
  const artistNames = $derived(a.artists?.length ? a.artists : [mainArtist(a.artist) ?? ''].filter(Boolean));
  function goToArtist(name: string) {
    onclose();
    showArtist(source, name);
  }
  function goToGenre(name: string) {
    onclose();
    player.full = false;
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
  let rating = $state<number | null>(null);
  $effect(() => {
    getAlbumRating(source, a.id).then(
      (r) => (rating = r.rating),
      () => {},
    );
  });
  async function rate(stars: number) {
    const was = rating;
    const now = was === stars ? 0 : stars;
    rating = now;
    navigator.vibrate?.(8);
    try {
      await setAlbumRating(source, a.id, now);
    } catch (e) {
      rating = was;
      toast.fail(e, "The rating didn't change");
    }
  }

  // Share: the album in words and its link here (lib/share.ts).
  function shareIt() {
    onclose();
    share(a.title, [a.artist, a.title].filter(Boolean).join(' – '), linkTo('library', albumSubject(source, a.id)));
  }

  // Its cover on this device (lib/localCovers.svelte.ts): a picture chosen here, or the server's read again.
  const ownCover = $derived(!!localCovers[coverKey(source, a.id)]);
  let picker = $state<HTMLInputElement>();
  async function chosen(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    onclose();
    if (!file) return;
    try {
      await setLocalCover(source, a.id, file);
    } catch (e) {
      toast.fail(e, "The cover wasn't changed");
    }
  }
  async function serverCover() {
    onclose();
    try {
      if (ownCover) await removeLocalCover(source, a.id);
      else await refreshCover(source, a.id, serverCoverUrl(source, a.id));
    } catch (e) {
      toast.fail(e, "The cover wasn't read again");
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
  let step = $state<ToPlaylistStep>(start);
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
  const replaceIt = () =>
    act(async () => {
      const r = await replaceIncoming(source, a.id, year.trim() || undefined);
      moved(r, `Replaced ${r.title} in ${music}.`);
    }, 'Could not replace the album');
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

<MenuSheet
  label={a.title}
  {onclose}
  images={[cover ?? albumCoverUrl(source, a.id)]}
  title={a.title}
  chips={[a.artist ?? 'Unknown artist']}
  heart={ask || step !== 'menu' ? null : { on: fav, toggle: toggleFavourite }}
  {actions}
  onback={ask ? () => (ask = false) : step === 'menu' ? null : () => (step = step === 'again' ? 'lists' : 'menu')}
  step={ask || step}
>
  {#if ask === 'move'}
    <div class="form">
      {#if !keep}
        <p class="ask">Working out the names…</p>
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
        <p class="ask">Pairing the tracks…</p>
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
      {/if}
      <div class="actions">
        {#if swap && (!swap.refused || swap.refused.kind === 'needs_year')}
          <button class="primary" disabled={working || !!swap.refused} aria-busy={working} onclick={replaceIt}>Replace</button>
        {/if}
        <button onclick={onclose}>Cancel</button>
      </div>
    </div>
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
    <MenuCards>
      <MenuGroup>
        {#if viewable}
          <MenuEntry
            icon="squareArrowOutUpRight"
            label="View"
            onclick={() => {
              onclose();
              viewPlaylist(albumSubject(source, a.id));
            }}
          />
        {/if}
        {#each artistNames as name (name)}<MenuEntry icon="user" label="Go to {name}" onclick={() => goToArtist(name)} />{/each}
        {#each genres as g (g)}<MenuEntry icon="music" label="Go to {g}" onclick={() => goToGenre(g)} />{/each}
      </MenuGroup>
      {#if rating !== null}
        {@const r = rating}
        <MenuGroup>
          <div class="entry rating" role="group" aria-label="Rating">
            <span>Rating</span>
            <span class="stars">
              {#each [1, 2, 3, 4, 5] as n (n)}
                <button class:on={n <= r} aria-label={n === 1 ? '1 star' : `${n} stars`} aria-pressed={n === r} onclick={() => rate(n)}><Icon name="star" size={24} /></button>
              {/each}
            </span>
          </div>
        </MenuGroup>
      {/if}
      <MenuGroup>
        {#if addable}<MenuEntry icon="listAdd" label="Add to playlist" onclick={() => (step = 'lists')} />{/if}
        {#if inApp && tracks?.length && kept.length < tracks.length}
          {@const list = tracks}
          <MenuEntry
            icon="arrowDownToLine"
            label="Download"
            onclick={() => {
              onclose();
              download(list);
            }}
          />
        {/if}
        {#if kept.length}
          {@const ids = kept.map((t) => t.download.id)}
          <MenuEntry
            icon="trash"
            label="Remove from this device"
            onclick={() => {
              onclose();
              removeDownloads(ids);
            }}
          />
        {/if}
        <MenuEntry icon="share" label="Share" onclick={shareIt} />
      </MenuGroup>
      {#if offer && (offer.job !== null || offer.targets.length || offer.replaceable || offer.deletable)}
        {@const o = offer}
        <MenuGroup label="Library">
          {#if o.job !== null}
            {@const job = o.job}
            <MenuEntry icon="close" label="Cancel conversion" danger onclick={() => cancel(job)} />
          {:else}
            {#each o.targets as t (t.id)}<MenuEntry icon="intoBox" label={`Move to ${t.name}`} onclick={() => askMove(t)} />{/each}
            {#if o.replaceable}<MenuEntry icon="swap" label={`Replace in ${music}`} onclick={askReplace} />{/if}
            {#if o.deletable}<MenuEntry icon="trash" label="Delete album" danger onclick={() => (ask = 'delete')} />{/if}
          {/if}
        </MenuGroup>
      {/if}
      <MenuGroup>
        <MenuEntry icon="image" label="Change cover on this device" onclick={() => picker?.click()} />
        <MenuEntry icon={ownCover ? 'imageOff' : 'reset'} label={ownCover ? "Use the server's cover" : 'Read the cover again'} onclick={serverCover} />
      </MenuGroup>
      <MenuGroup>
        {#if resumeAt}<MenuEntry icon="resume" label="Remove resume point" onclick={forgetResume} />{/if}
        <MenuEntry icon="search" label="Web search" onclick={() => search('https://duckduckgo.com/?q=')} />
        <MenuEntry icon="search" label="Genius search" onclick={() => search('https://genius.com/search?q=')} />
      </MenuGroup>
      <input class="picker" type="file" accept="image/*" bind:this={picker} onchange={chosen} />
    </MenuCards>
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

  .stars {
    display: flex;
  }

  .stars button {
    display: grid;
    place-items: center;
    width: 40px;
    height: 44px;
    padding: 0;
    background: none;
    border: 0;
    color: color-mix(in srgb, var(--text) 45%, transparent);
    cursor: pointer;
  }

  .stars button.on {
    color: var(--art-accent, var(--accent));
  }

  .stars button.on :global(svg) {
    fill: currentColor;
  }

  .picker {
    display: none;
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
    border-radius: 10px;
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
</style>
