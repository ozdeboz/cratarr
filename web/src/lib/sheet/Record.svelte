<script lang="ts" module>
  import { reach } from '../reach.svelte';
  import { OWN } from '../sources.svelte';

  /** Whether a record of `source` opens on the one sheet: Cratarr's own, while Cratarr answers. Out of reach it opens the sheet
   *  it opens today until the server marks what works away (RS8, RS17: lane A's DC1a part 2; the coordinator, 2026-11-21). */
  export const opensRecord = (source: string) => source === OWN && !reach.away;
</script>

<script lang="ts">
  // RS01, the one record sheet (SCENARIOS-DC1 RS1–RS8, RS14, RS19, RS21, RS23; DOMAIN-CORE §8): a Cratarr record wherever it
  // opens, drawn from its one entity (GET /api/records/{id}, through the one page cache): the Head from its properties and state,
  // the Play row as sent, then its sections in the entity's order, each section's actions and links on the menu's cards by group,
  // the Versions from its entities. Every entry is the server's (`decide()` allowed it, the words table named it); the page only
  // runs each by its name, asking first where the server asks a yes (`confirm`), and says a refusal in the server's words. A
  // place's own addition (a crate's tracks) is its opener's `context` until the server sends contexts (RS10).
  import type { Snippet } from 'svelte';
  import { untrack } from 'svelte';
  import type { ActionView } from '../generated/ActionView';
  import type { LinkView } from '../generated/LinkView';
  import {
    albumCoverUrl,
    albumResume,
    albumView,
    actOn,
    ApiError,
    COVER_MAX,
    deleteAlbum,
    forgetAlbumResume,
    getAlbumCovers,
    lookAlbumCovers,
    missingCall,
    moveAlbum,
    movePreview,
    replaceIncoming,
    replacePreview,
    setAlbumCover,
    setAlbumFavorite,
    stopTask,
    uploadAlbumCover,
    wantAlbum,
    type AlbumMoved,
    type Collection,
    type CoverChoice,
    type KeepPreview,
    type ListedAlbum,
    type ReplacePreview,
  } from '../api';
  import { albumRatings, copiesAdded, copiesGone, copiesMoved, copiesReplaced, library, listedAlbum, loadAlbumRating, playListedAlbum, rateAlbum, readLibrary, sayWantFailed } from '../albums.svelte';
  import { moveSentences, moveSheet, moveWords, replacedWords, replaceSheet, replaceShown } from '../copiesRule';
  import { recordOf, records } from '../cache.svelte';
  import { downloads, inApp, removeDownloads } from '../app.svelte';
  import { albumOfDownload } from '../albums.svelte';
  import { BANDCAMP, albumIdOf } from '../bandcampPreview';
  import { can, offers } from '../capabilities.svelte';
  import { isFavorite, loadAlbumFavorites, setFavorite } from '../favourites.svelte';
  import { whenWritten } from '../fileRuns.svelte';
  import { askFor, followingNow } from '../following.svelte';
  import { jobs } from '../jobs.svelte';
  import { isKept, setKept } from '../kept.svelte';
  import { linkTo } from '../addresses';
  import { hiddenLibraries } from '../listFilter.svelte';
  import { refreshCover } from '../localCovers.svelte';
  import { ENTRY_NAME, ICON_OF, RESUME } from '../menuTable';
  import { playPlaylistFrom } from '../output.svelte';
  import { leavePlayer } from '../navStep.svelte';
  import { albumGone, viewOver } from '../panel.svelte';
  import { playCollection } from '../play.svelte';
  import { likeChip } from '../quickCrate';
  import { grabCrate } from '../quickCrates.svelte';
  import { openRadio } from '../radioAsk.svelte';
  import { recordWords, share } from '../share';
  import { progressOf, tasks } from '../tasks.svelte';
  import { toast } from '../toast.svelte';
  import { TracksToPlaylist, type ToPlaylistStep } from '../tracksToPlaylist.svelte';
  import { withUndo } from '../undo.svelte';
  import { wantOf } from '../wantPath';
  import AlbumCard from '../AlbumCard.svelte';
  import Asking from '../Asking.svelte';
  import Cover from '../Cover.svelte';
  import DownloadStep from '../DownloadStep.svelte';
  import type { IconName } from '../Icon.svelte';
  import MenuCards from '../MenuCards.svelte';
  import MenuEntry from '../MenuEntry.svelte';
  import MenuGroup from '../MenuGroup.svelte';
  import MenuSheet, { type SheetAction } from '../MenuSheet.svelte';
  import Meter from '../Meter.svelte';
  import Notice from '../Notice.svelte';
  import PageLoading from '../PageLoading.svelte';
  import PlaylistsStep from '../PlaylistsStep.svelte';
  import RatingStars from '../RatingStars.svelte';
  import RecordEdit from '../RecordEdit.svelte';
  import ReplaceTrackSheet from '../ReplaceTrackSheet.svelte';
  import Seg from '../Seg.svelte';
  import SettingRow from '../SettingRow.svelte';
  import { deviceItems, headChips, playRow, Runs, sectionsOf, stateLine, versionLines, type Item, type PlaceWord } from './record';

  let {
    id,
    from,
    opener = undefined,
    title: givenTitle = '',
    credit: givenCredit = null,
    start = 'menu',
    context = null,
    ongone,
    onundone,
    onresumegone,
    onclose,
  }: {
    /** The record, by its id (`al-<n>`). */
    id: string;
    /** The place it opens from (the when-table's `from`), and the record's own page's opener. */
    from: PlaceWord;
    opener?: PlaceWord;
    /** What its opener shows of it, for the Head until the entity comes (LV7). */
    title?: string;
    credit?: string | null;
    /** 'cover': straight on Change cover (Health's records without one). */
    start?: 'menu' | 'cover';
    /** The place's own addition (a crate's tracks of it), drawn as its Context section. */
    context?: Snippet | null;
    /** A copy left its library (moved, replaced or deleted): its opener's list kept true. */
    ongone?: () => void;
    /** A move's or a delete's Undo went through. */
    onundone?: () => void;
    /** Its resume point was removed here (its page's Resume goes too). */
    onresumegone?: () => void;
    onclose: () => void;
  } = $props();

  // The record as the cache holds it from this place, the libraries switched off as they stand (LV4: a new switch, a new key).
  const rec = recordOf(() => ({ id, place: { from, without: hiddenLibraries() ?? [], ...(opener ? { opener } : {}) } }));
  const e = $derived(rec.shown?.entity ?? null);
  /** The record it is now (another after a merge, RS18). */
  const rid = $derived(rec.shown?.id ?? id);
  const title = $derived(e?.properties.title.value ?? givenTitle);
  const credit = $derived(e?.properties.credit ?? givenCredit ?? '');
  const shownAt = $derived(e && (rec.look === 'entity' || rec.look === 'kept'));
  const has = (name: string) => !!e?.actions.some((a) => a.name === name);
  // What its reads beside the entity follow: whether it plays, keeps a heart, a rating (each read once, not on every update).
  const plays = $derived(has('play'));
  const hearts = $derived(has('favourite'));
  const rates = $derived(has('rating'));

  // Where the sheet stands: its entries, or a step (the playlists, Download…, Edit, Missing tracks), or a question asked in place
  // (the server's confirm, a move's or a Replace's names first, the covers to choose).
  let step = $state<ToPlaylistStep>('menu');
  let exporting = $state(false);
  let missingOpen = $state(false);
  let editOpen = $state(false);
  type Ask = 'confirm' | 'move' | 'replace' | 'cover' | 'cover-url';
  let ask = $state<false | Ask>(false);
  let asked = $state<ActionView | null>(null);
  let working = $state(false);
  /** The account's resume point here (decision 0030). */
  let resumeAt = $state<{ track: string; position: number } | null>(null);

  // Its tracks, where an entry works on them (Add to playlist, Download…, Start radio), read once as the sheet opens.
  let tracksRead = $state<{ source: string; id: string; name: string; artist: string | null; album: string | null; album_id: string; duration_secs: number | null }[] | null>(null);
  const wantsTracks = $derived(has('add_to_playlist') || has('download') || has('start_radio') || resumeAt !== null);
  $effect(() => {
    if (!wantsTracks || untrack(() => tracksRead)) return;
    const of = untrack(() => rid);
    albumView(OWN, of).then(
      (r) => (tracksRead = r.rows.filter((x) => x.id && !x.missing).map((x) => ({ source: r.source, id: x.id!, name: x.name ?? x.file, artist: x.artist, album: x.album, album_id: of, duration_secs: x.duration_secs }))),
      () => {},
    );
  });
  const ids = $derived(tracksRead?.map((t) => t.id) ?? []);

  // Resume (decision 0030): the account's resume point, the page's own beside the server's Play until the server sends it (its
  // candidates leave Resume to the page); Remove resume point with it.
  $effect(() => {
    if (!plays) return;
    const of = rid;
    albumResume(of).then((r) => r.track && (resumeAt = { track: r.track, position: r.position_secs ?? 0 }), () => {});
  });
  async function forgetResume() {
    const [of, gone] = [rid, onresumegone];
    onclose();
    try {
      await forgetAlbumResume(of);
      gone?.();
    } catch (err) {
      toast.fail(err, "The resume point wasn't removed");
    }
  }
  function resume() {
    const at = resumeAt;
    const list = tracksRead;
    const i = at && list ? list.findIndex((t) => t.id === at.track) : -1;
    if (!at || !list || i < 0) return;
    onclose();
    navigator.vibrate?.(8);
    playPlaylistFrom(title, OWN, list, i, at.position).catch((err) => toast.fail(err, 'Could not start playing'));
  }

  // The heart, the panel's own (lib/favourites): where the server offers Favourite.
  $effect(() => {
    if (hearts) loadAlbumFavorites(OWN, [rid]);
  });
  const fav = $derived(isFavorite(OWN, rid));
  async function toggleFavourite() {
    const on = !fav;
    setFavorite(OWN, rid, on);
    navigator.vibrate?.(8);
    try {
      await setAlbumFavorite(OWN, rid, on);
    } catch (err) {
      setFavorite(OWN, rid, !on);
      toast.fail(err, "The favourite didn't change");
      records.refused(rid);
    }
  }
  $effect(() => {
    if (rates) loadAlbumRating(OWN, rid);
  });
  const rating = $derived(albumRatings[`${OWN}/${rid}`] ?? null);

  /** One run of each action at a time (CN11, RS21). */
  const runs = new Runs();
  /** An action refused: the server's words, and the record read again (CN10, RS19). */
  function refused(err: unknown, fallback: string) {
    toast.fail(err, fallback);
    records.refused(rid);
  }

  // The Play row (RS3, CN4): as sent. Requested shows with no tap until the Request section (OQ-DC1-3 (a)).
  const large = $derived<SheetAction[]>(
    e && shownAt && !ask && !exporting && step === 'menu'
      ? playRow(e).flatMap((a) => [
          { icon: a.icon as IconName, label: a.title, disabled: a.name === 'requested', onclick: () => run(a) },
          ...(a.name === 'play' && resumeAt ? [{ icon: 'resume' as const, label: RESUME, disabled: !tracksRead?.some((t) => t.id === resumeAt?.track), onclick: resume }] : []),
        ])
      : [],
  );

  /** What a tap on an entry does: the server's action by its name, asked first where it asks a yes. */
  function run(a: ActionView) {
    if (a.confirm) {
      asked = a;
      ask = 'confirm';
      return;
    }
    void runs.once(`${a.name}:${a.fields?.from ?? ''}:${a.fields?.to ?? ''}`, () => perform(a));
  }
  async function perform(a: ActionView) {
    switch (a.name) {
      case 'play':
      case 'play_from':
        return playIt(a);
      case 'shuffle':
      case 'play_next':
      case 'add_to_queue':
        return queueIt(a);
      case 'start_radio':
        onclose();
        openRadio({ name: title, kind: 'record', within: a.fields?.what as Collection, seed: async () => ids });
        return;
      case 'want':
        return wantIt(a);
      case 'favourite':
        return toggleFavourite();
      case 'add_to_playlist':
        step = 'lists';
        return;
      case 'download':
        exporting = true;
        return;
      case 'sync':
        return setKept('album', OWN, rid, !isKept('album', OWN, rid), title);
      case 'edit':
        editOpen = true;
        return;
      case 'change_cover':
        return askCover();
      case 'missing_tracks':
        missingOpen = true;
        return;
      case 'save_to':
      case 'move_to':
      case 'add_to':
        return askMove(a);
      case 'replace_in':
        return askReplace(a);
      case 'delete':
      case 'delete_from':
      case 'delete_arrival':
        return remove(a);
      default:
        // Cancel conversion, and what the page has no step of its own for: the server's address as it is.
        onclose();
        try {
          await actOn(a);
          if (a.name === 'cancel_conversion') await jobs.poke();
          records.after(rid);
        } catch (err) {
          refused(err, `Could not ${a.title.toLowerCase()}`);
        }
    }
  }

  /** Play and Play from: the record's tracks queued here, from the copy named; a preview its Bandcamp album's. */
  function playIt(a: ActionView) {
    const [source, raw] = a.href.split('/').slice(3, 5).map(decodeURIComponent);
    const album = source === BANDCAMP && /^https?:/.test(raw) ? albumIdOf(raw) : raw;
    onclose();
    navigator.vibrate?.(8);
    void playListedAlbum(source, { id: album, title }, a.fields?.from);
  }
  /** Shuffle, Play next, Add to queue: the server's pick of the record (`/api/play`); asked of the player followed while following one. */
  async function queueIt(a: ActionView) {
    onclose();
    navigator.vibrate?.(8);
    const how = (a.fields?.how ?? 'play') as 'shuffle' | 'next' | 'last';
    try {
      if (followingNow() && how !== 'shuffle') {
        await askFor((tracksRead ?? []).map((t) => ({ source: OWN, id: t.id, name: t.name, artist: t.artist, album: title, album_id: rid })), how);
        return;
      }
      await playCollection(a.fields?.what as Collection, how, title);
      if (how === 'next') toast.show({ text: 'Plays next.', tone: 'done' });
      if (how === 'last') toast.show({ text: 'Added to the queue.', tone: 'done' });
    } catch (err) {
      toast.fail(err, 'Could not start playing');
    }
  }
  /** Want (F36): by its record; its Requested in its place once the record is read again (CN3, CN11). */
  async function wantIt(a: ActionView) {
    onclose();
    try {
      const { request } = await wantAlbum(wantOf({ record: a.fields?.record ?? rid, artist: credit, album: title }));
      if (request.fetched?.startsWith('Not handed over')) toast.show({ text: request.fetched, tone: 'error' });
      records.after(rid);
    } catch (err) {
      sayWantFailed(err);
      records.refused(rid);
    }
  }

  // The links (Siren's): a place in the panel, a crate made like it, the record shared, or an outside search.
  function go(l: LinkView) {
    const href = l.href;
    onclose();
    if (/^https?:\/\//.test(href)) return void window.open(href, '_blank', 'noopener');
    if (l.name === 'share') return void share(title, recordWords(credit, title), linkTo('library', href));
    const like = href.match(/^crate-new:like:album:(.+)$/);
    if (l.name !== 'view') leavePlayer();
    if (like) return grabCrate([likeChip('album', like[1], title)]);
    viewOver(href);
  }

  // On this device (RS8): the device's own holding picks from what the server sent.
  const keptTracks = $derived(inApp ? downloads.tracks.filter((t) => t.source === OWN && albumOfDownload(t) === rid) : []);
  const deviceShown = $derived(e ? deviceItems(e, { inApp, keeps: isKept('album', OWN, rid) || keptTracks.length > 0 }) : []);
  function removeHere() {
    const ids = keptTracks.map((t) => t.download.id);
    onclose();
    if (isKept('album', OWN, rid)) setKept('album', OWN, rid, false, title);
    if (ids.length) removeDownloads(ids);
  }

  // Add to playlist (lib/tracksToPlaylist): all its tracks.
  const toList = new TracksToPlaylist(
    () => OWN,
    () => ids,
    'this record',
  );
  $effect(() => {
    if (ids[0]) toList.ask([ids[0]]);
  });
  /** Its row as the Library lists it, for Edit's fields; else what the entity says. */
  const listed = $derived(listedAlbum(OWN, rid)?.a ?? ({ id: rid, title, artist: credit || null, year: e?.properties.year.value ?? null, type: e?.properties.type.value ?? null } as ListedAlbum));

  // A move's and a Replace's questions say the libraries by name, as the Library's list has them.
  $effect(() => {
    if (!library.view) readLibrary(false).catch(() => {});
  });
  const libs = $derived(library.view?.sources.find((s) => s.id === OWN)?.library_info ?? []);
  const libraryOf = (lib: string | undefined) => libs.find((l) => l.id === lib) ?? null;
  const fromLib = $derived(libraryOf(asked?.fields?.from));
  const toLib = $derived(libraryOf(asked?.fields?.to));
  const fromName = $derived(fromLib?.name ?? 'Incoming');
  const trashDays = $derived(library.view?.trash_days ?? 14);
  const adding = $derived(asked?.name === 'add_to');
  const NO_LIDARR = "Lidarr isn't set up in Settings, so it may fetch this record again.";
  const converted = (n: number, lib: string) => `${n} lossless ${n === 1 ? 'file' : 'files'} above ${lib}'s quality limit ${n === 1 ? 'is' : 'are'} converted to FLAC first.`;
  let keep = $state<KeepPreview | null>(null);
  const copying = $derived(!!keep?.copy);
  const sentences = (k: KeepPreview) => moveSentences({ title, from: fromName, to: toLib?.name ?? '', folder: k.folder ?? '', files: k.files, copy: !!k.copy, adding });
  let swap = $state<ReplacePreview | null>(null);
  const swapAsks = $derived(replaceSheet(fromLib?.kind));
  let coverWay = $state<'keep' | 'new'>('keep');
  let year = $state('');
  const DIFF_SHOWN = 12;
  let diffAll = $state(false);

  async function act(work: () => Promise<void>, failure: string) {
    if (working) return;
    working = true;
    try {
      await work();
    } catch (err) {
      refused(err, failure);
    } finally {
      working = false;
      onclose();
    }
  }
  /** Moved, saved or added, or queued to convert first (W13): shown at once, the record read again. */
  function moved(r: AlbumMoved, words: string, left: () => void, from: string | undefined, undone = `${r.title} is back in ${fromName}.`) {
    records.after(rid);
    if (r.queued) {
      jobs.queued({ source: OWN, album_id: rid, from });
      jobs.poke();
      return;
    }
    left();
    ongone?.();
    withUndo(words, [r.undo], { undone, after: onundone });
  }

  async function askMove(a: ActionView) {
    asked = a;
    ask = 'move';
    keep = null;
    year = '';
    try {
      keep = await movePreview(OWN, rid, a.fields?.from, a.fields?.to ?? '');
    } catch (err) {
      refused(err, 'Could not work out the names');
      onclose();
    }
  }
  async function askReplace(a: ActionView) {
    asked = a;
    ask = 'replace';
    swap = null;
    coverWay = 'keep';
    year = '';
    try {
      swap = await replacePreview(OWN, rid, a.fields?.from, a.fields?.to ?? '');
    } catch (err) {
      refused(err, 'Could not pair the tracks');
      onclose();
    }
  }
  async function tryYear() {
    if (!/^\d{4}$/.test(year.trim()) || !asked) return;
    try {
      if (ask === 'replace') swap = await replacePreview(OWN, rid, asked.fields?.from, asked.fields?.to ?? '', year.trim());
      else keep = await movePreview(OWN, rid, asked.fields?.from, asked.fields?.to ?? '', year.trim());
    } catch {
      /* keep the question open */
    }
  }
  const moveIt = () =>
    act(async () => {
      const a = asked;
      if (!a?.fields?.to) return;
      const [into, from] = [a.fields.to, a.fields.from];
      // The tap carries what its sheet showed (RF10): the server refuses it where the copy changed since.
      const r = await moveAlbum(OWN, rid, from, into, year.trim() || undefined, moveSheet(keep));
      const said = keep ? sentences(keep) : null;
      const went = [{ source: OWN, id: rid, from, to: into }];
      moved(r, said?.done ?? `${a.title}.`, () => (copying ? copiesAdded(went) : copiesMoved(went)), from, said?.undone);
    }, `Could not ${moveWords(copying, adding).act} the record`);
  const replaceIt = (carry = false) =>
    act(async () => {
      const a = asked;
      if (!a?.fields?.to) return;
      const [into, from, kind, asks] = [a.fields.to, a.fields.from, fromLib?.kind, swapAsks];
      const shown = asks.shown ? replaceShown(swap) : undefined;
      const r = await replaceIncoming(OWN, rid, from, into, year.trim() || undefined, asks.cover && swap?.covers ? coverWay : undefined, asks.carry && carry, shown);
      const said = replacedWords(r.title, toLib?.name ?? '', fromName, kind);
      moved(r, said.done, () => copiesReplaced([{ source: OWN, id: rid, from, kind }]), from, said.undone);
    }, 'Could not replace the record');
  /** Delete, asked first by the server's confirm: one tap, one Delete (RS21). The record stays while a copy does. */
  function remove(a: ActionView) {
    void runs.once(`delete:${a.fields?.from ?? ''}`, () =>
      act(async () => {
        const from = a.fields?.from;
        const { undo, title: was } = await deleteAlbum(OWN, rid, from);
        ongone?.();
        for (const x of copiesGone([{ source: OWN, id: rid, from }])) albumGone(x.source, x.id);
        records.after(rid);
        withUndo(`Deleted ${was}.`, [undo], { undone: `${was} is back in ${libraryOf(from)?.name ?? 'its library'}.`, after: onundone });
      }, `Could not ${a.title.toLowerCase()}`),
    );
  }

  // Change cover (0.9.5; decision 0051: the cover for everyone, where the server offers it): looked for as a task, the album's
  // own first, then Discogs' and the Cover Art Archive's as they are found; from an address, or uploaded.
  type Covers = { current: CoverChoice | null; candidates: CoverChoice[] };
  let covers = $state<Covers | null>(null);
  let coverTask = $state<number | null>(null);
  const coverLook = $derived(coverTask !== null ? tasks.of(`covers:${rid}`) : null);
  const coverProgress = $derived(coverLook && coverLook.finished === null ? progressOf(coverLook) : null);
  const coversSoFar = $derived(covers ?? ((coverLook?.result as Covers | null | undefined) || null));
  function stopLook() {
    const t = coverTask;
    if (t === null || !coverLook || coverLook.finished !== null) return;
    coverTask = null;
    stopTask(t).catch(() => {});
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
  $effect(() => {
    if (untrack(() => start) === 'cover') untrack(askCover);
  });
  async function askCover() {
    ask = 'cover';
    covers = null;
    if (can('cover_look') === false) {
      try {
        covers = await getAlbumCovers(OWN, rid);
      } catch (err) {
        refused(err, 'Could not find covers');
        onclose();
      }
      return;
    }
    try {
      const { task } = await lookAlbumCovers(OWN, rid);
      tasks.begun(task);
      coverTask = task.id;
    } catch (err) {
      if (!missingCall(err)) {
        refused(err, 'Could not find covers');
        return onclose();
      }
      try {
        covers = await getAlbumCovers(OWN, rid);
      } catch (err2) {
        refused(err2, 'Could not find covers');
        onclose();
      }
    }
  }
  let coverUrl = $state('');
  let settingCover = $state<string | null>(null);
  const setCover = (c: CoverChoice) => {
    if (working) return;
    settingCover = c.url;
    stopLook();
    return act(async () => {
      const r = await setAlbumCover(OWN, rid, c.url);
      whenWritten(r, `Changed the cover of ${title}.`, () => void refreshCover(OWN, rid, albumCoverUrl(OWN, rid)));
    }, 'Could not change the cover').finally(() => (settingCover = null));
  };
  const size = (c: CoverChoice) => (c.width && c.height ? `${c.width} × ${c.height}` : '');
  let picker = $state<HTMLInputElement>();
  async function chosen(ev: Event) {
    const file = (ev.currentTarget as HTMLInputElement).files?.[0];
    const [of, name] = [rid, title];
    onclose();
    if (!file) return;
    if (file.size > COVER_MAX) return toast.fail(null, 'That picture is too large (at most 12 MB)');
    try {
      const r = await uploadAlbumCover(OWN, of, file);
      whenWritten(r, `Changed the cover of ${name}.`, () => void refreshCover(OWN, of, albumCoverUrl(OWN, of)));
    } catch (err) {
      const big = err instanceof ApiError && err.status === 413;
      toast.fail(missingCall(err) || big ? null : err, big ? 'That picture is too large (at most 12 MB)' : missingCall(err) ? "This server can't take a cover yet" : "The cover wasn't uploaded");
      records.refused(of);
    }
  }
</script>

{#snippet lookProgress()}
  {#if coverProgress}{#if coverProgress.value !== null}<Meter label={coverProgress.line} value={coverProgress.value} />{:else}<span class="look-line">{coverProgress.line}</span>{/if}{/if}
{/snippet}

{#snippet item(i: Item)}
  {#if i.name === 'favourite'}
    <MenuEntry icon={i.icon as IconName} label={i.title} switched={fav} onclick={toggleFavourite} />
  {:else if i.name === 'rating'}
    {#if rating !== null}
      {@const r = rating}
      <div class="entry rating"><span>{i.title}</span><RatingStars value={r} onrate={(n) => rateAlbum(OWN, rid, n)} /></div>
    {/if}
  {:else if i.name === 'remove_from_device'}
    <MenuEntry icon="trash" label={ENTRY_NAME.removeFromDevice ?? ''} danger onclick={removeHere} />
  {:else if i.action}
    {@const a = i.action}
    <MenuEntry icon={i.icon as IconName} label={i.title} danger={i.group === 'remove'} disabled={working} onclick={() => run(a)} />
  {:else if i.link}
    {@const l = i.link}
    <MenuEntry icon={i.icon as IconName} label={i.title} onclick={() => go(l)} />
  {/if}
{/snippet}

{#if missingOpen}
  <ReplaceTrackSheet source={OWN} album={{ id: rid, title }} {onclose} />
{:else if editOpen}
  <RecordEdit source={OWN} albums={[listed]} {onclose} />
{:else}
  <MenuSheet
    label={title}
    {onclose}
    images={[albumCoverUrl(OWN, rid)]}
    {title}
    chips={e ? headChips(e) : [credit].filter(Boolean)}
    heart={has('favourite') && !ask && !exporting && step === 'menu' ? { on: fav, toggle: toggleFavourite } : null}
    actions={large}
    beside={ask === 'cover' && coverProgress ? lookProgress : null}
    end={ask === 'cover'
      ? [
          ...(coverProgress ? [{ icon: 'search' as const, label: 'Looking for covers', busy: true, onclick: () => {} }] : []),
          { icon: 'link', label: 'From an address', onclick: () => ((coverUrl = ''), (ask = 'cover-url')) },
          ...(offers('cover_upload') ? [{ icon: 'upload' as const, label: 'Upload cover', onclick: () => picker?.click() }] : []),
        ]
      : null}
    onback={ask === 'cover-url' ? () => (ask = 'cover') : ask ? () => (ask = false) : exporting ? () => (exporting = false) : step === 'menu' ? null : () => (step = step === 'again' ? 'lists' : 'menu')}
    step={ask || (exporting ? 'export' : step)}
  >
    {#if exporting && tracksRead}
      <DownloadStep what={{ kind: 'tracks', name: title, tracks: tracksRead.map((x) => ({ source: x.source, id: x.id })) }} tracks={tracksRead} {onclose} />
    {:else if ask === 'confirm' && asked}
      {@const a = asked}
      <div class="form">
        <p class="ask">{a.confirm}{a.group === 'remove' ? ` It stays in the trash for ${trashDays} ${trashDays === 1 ? 'day' : 'days'}.` : ''}</p>
        <div class="actions">
          <button class:danger={a.group === 'remove'} class="solid" disabled={working} aria-busy={working} onclick={() => void runs.once(`${a.name}:${a.fields?.from ?? ''}`, () => perform(a))}>{a.title}</button>
          <button onclick={onclose}>Cancel</button>
        </div>
      </div>
    {:else if ask === 'move'}
      <div class="form">
        {#if !keep}
          <Asking />
        {:else if keep.refused?.kind === 'needs_year'}
          <p class="ask">The tags don't say when {title} came out. Which year?</p>
          <input class="year" inputmode="numeric" maxlength="4" placeholder="Year" aria-label="Release year" bind:value={year} oninput={tryYear} />
        {:else if keep.refused}
          <p class="ask">Can't {moveWords(copying, adding).act} {title}: {keep.refused.detail}</p>
        {:else}
          <p class="ask">
            {sentences(keep).lead}<strong>{keep.folder}</strong>{sentences(keep).tail}{#if keep.convert}
              {` ${converted(keep.convert, toLib?.name ?? '')}`}{/if}{#if !keep.acquisition && fromLib?.kind !== 'main'}
              {` ${NO_LIDARR}`}{/if}
          </p>
        {/if}
        <div class="actions">
          {#if keep && (!keep.refused || keep.refused.kind === 'needs_year')}
            <button class="primary" disabled={working || !!keep.refused} aria-busy={working} onclick={moveIt}>{sentences(keep).button}</button>
          {/if}
          <button onclick={onclose}>Cancel</button>
        </div>
      </div>
    {:else if ask === 'replace'}
      <div class="form">
        {#if !swap}
          <Asking />
        {:else if swap.refused?.kind === 'needs_year'}
          <p class="ask">The tags don't say when {title} came out. Which year?</p>
          <input class="year" inputmode="numeric" maxlength="4" placeholder="Year" aria-label="Release year" bind:value={year} oninput={tryYear} />
        {:else if swap.refused?.kind === 'unmatched'}
          <p class="ask">Can't replace {swap.old}: no match for {swap.refused.detail.join(', ')}.</p>
        {:else if swap.refused}
          <p class="ask">Can't replace {swap.old}: {swap.refused.detail}</p>
        {:else}
          <p class="ask">
            Replace <strong>{swap.old}</strong> with this copy ({swap.tracks} {swap.tracks === 1 ? 'track' : 'tracks'}{swap.added ? `, ${swap.added} new` : ''}){#if swap.folder !== swap.old}, as <strong
                >{swap.folder}</strong
              >{/if}? The old copy stays in the trash for {trashDays}
            {trashDays === 1 ? 'day' : 'days'}.{#if swap.convert}
              {` ${converted(swap.convert, toLib?.name ?? '')}`}{/if}{#if !swap.acquisition && swapAsks.lidarr}
              {` ${NO_LIDARR}`}{/if}
          </p>
          {#if swapAsks.cover && swap.covers && (swap.covers.current || swap.covers.new)}
            {@const c = swap.covers}
            <div class="covers">
              <figure class:on={coverWay === 'keep'}><span class="thumb"><Cover src={c.current} /></span><figcaption>Current</figcaption></figure>
              <figure class:on={coverWay === 'new'}><span class="thumb"><Cover src={c.new} /></span><figcaption>New</figcaption></figure>
            </div>
            <SettingRow label="Cover"><Seg label="Cover" options={[{ value: 'keep', label: 'Keep' }, { value: 'new', label: 'New' }]} bind:value={coverWay} /></SettingRow>
          {/if}
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
          {#if swapAsks.carry && swap.carry?.length}<p class="ask carried">Carried over: {swap.carry.join(', ')}.</p>{/if}
        {/if}
        <div class="actions">
          {#if swap && (!swap.refused || swap.refused.kind === 'needs_year')}
            {#if swapAsks.carry && swap.carry?.length}
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
      <div class="form">
        <p class="ask">The picture's address</p>
        <input class="url" type="url" inputmode="url" placeholder="https://…" aria-label="Picture address" bind:value={coverUrl} />
        <div class="actions">
          <button class="solid" disabled={working || !/^https?:\/\/\S+$/.test(coverUrl.trim())} aria-busy={working} onclick={() => setCover({ url: coverUrl.trim(), width: null, height: null })}>Use it</button>
          <button onclick={() => (ask = 'cover')}>Cancel</button>
        </div>
      </div>
    {:else if ask === 'cover'}
      {#if !coversSoFar}
        {#if !coverProgress}<PageLoading />{/if}
      {:else if covers && !covers.candidates.length}
        <p class="ask pad">No other covers found.</p>
      {:else}
        {@const list = coversSoFar}
        <ul class="cover-grid">
          {#if list.current}<li><AlbumCard source="cover" album={{ id: 'current', title: 'Current', year: null }} cover={list.current.url} lines={[size(list.current)]} onopen={() => onclose()} /></li>{/if}
          {#each list.candidates as c (c.url)}
            <li>
              <AlbumCard
                source="cover"
                album={{ id: c.url, title: c.what ?? (c.from === 'discogs' ? 'Discogs' : 'Cover Art Archive'), year: null }}
                cover={c.url}
                lines={[size(c), c.from === 'discogs' ? 'Discogs' : 'Cover Art Archive']}
                busy={settingCover === c.url}
                onopen={() => setCover(c)}
              />
            </li>
          {/each}
        </ul>
      {/if}
    {:else if step === 'menu'}
      {#if rec.look === 'gone'}
        <Notice text={rec.shown?.gone ?? ''} />
      {:else if rec.look === 'error'}
        <Notice text={rec.shown?.error instanceof Error ? rec.shown.error.message : 'Unexpected error'} onretry={() => records.refused(rid)} />
      {:else if rec.look === 'loader'}
        <PageLoading />
      {:else if e && shownAt}
        <p class="line">{stateLine(e)}</p>
        <MenuCards>
          {#each sectionsOf(e) as part (part.section)}
            {#if part.section === 'versions'}
              <MenuGroup label="Versions">
                {#each versionLines(e) as l (l.key)}
                  {@const tap = l.play}
                  {@const open = l.open}
                  <MenuEntry
                    icon={l.plays ? 'play' : open ? ICON_OF.album : tap ? 'play' : null}
                    label={l.text}
                    disabled={!tap && !open}
                    onclick={() => (tap ? run(tap) : open ? (onclose(), viewOver(open)) : undefined)}
                  />
                {/each}
              </MenuGroup>
            {:else if part.section === 'context'}
              {@render context?.()}
            {:else if part.section === 'device'}
              <MenuGroup>{#each deviceShown as i (i.name)}{@render item(i)}{/each}</MenuGroup>
            {:else}
              {#each part.cards as card (card.group)}
                <MenuGroup label={card.group === 'library' ? 'Library' : undefined}>{#each card.items as i (`${i.name}:${i.title}:${i.link?.href ?? i.action?.href ?? ''}:${i.action?.fields?.from ?? ''}`)}{@render item(i)}{/each}</MenuGroup>
              {/each}
            {/if}
          {/each}
          {#if resumeAt}<MenuGroup><MenuEntry icon="resume" label={ENTRY_NAME.removeResume ?? ''} onclick={forgetResume} /></MenuGroup>{/if}
          <input class="picker" type="file" accept="image/*" bind:this={picker} onchange={chosen} />
        </MenuCards>
      {/if}
    {:else}
      <PlaylistsStep to={toList} bind:step {onclose} />
    {/if}
  </MenuSheet>
{/if}

<style>
  /* The state line under the Head (RS2): its words as sent. */
  .line {
    margin: 0 16px 8px;
    color: var(--muted);
    font-size: 0.88rem;
  }

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

  .actions button.primary,
  .actions button.solid {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .actions button.danger.solid {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
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

  .look-line {
    color: var(--accent-ink);
    font-size: 0.8rem;
  }
</style>
