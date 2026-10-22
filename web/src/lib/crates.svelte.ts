// The permanent crates and their sizes (decision 0024), for the Crates page and the crate viewed in the panel. Asked for
// when looked at, and again when the playlists change (sorting a track into one shrinks Unlisted).
import { albumCoverUrl, crateImageUrl, crateRows, deleteCrate, getCrates, queueTracks, leaveOutOfCrate, type Excluding, removeCrateImage, setCrateFavouriteAlbums, setCrateFavourites, setCrateImage, type Condition, type Crate, type CrateFavourites, type PlaylistRow } from './api';
import { dropFromQueue, enqueue, local, startRadio, type LocalTrack } from './localPlayer.svelte';
import { isLocal, playPlaylistFrom, shownPlaylists } from './output.svelte';
import { prefs } from './prefs.svelte';
import { viewPlaylist } from './panel.svelte';
import { toast } from './toast.svelte';
import { withUndo } from './undo.svelte';

export const crates = $state<{ list: Crate[] | null; error: string | null }>({ list: null, error: null });

export async function loadCrates() {
  try {
    const list = await getCrates(prefs.source || undefined);
    // A tap still on its way to the server keeps showing.
    crates.list = list.map((c) => ({
      ...c,
      ...(c.id in pending ? { favourites: pending[c.id] } : {}),
      ...(c.id in pendingAlbums ? { favourite_albums: pendingAlbums[c.id] } : {}),
    }));
    crates.error = null;
    await moveBrowserSettings(list);
  } catch (e) {
    crates.error = e instanceof Error ? e.message : 'Unexpected error';
  }
}

// The favourites toggle, one setting per crate, kept on the server (decision 0024, amended): a crate not set yet
// shows no favourites. A tap shows at once; the setting follows to the server.
const pending: Record<string, CrateFavourites> = {};
let saving: Promise<unknown> = Promise.resolve();
export const favouritesOf = (id: string): CrateFavourites => crates.list?.find((c) => c.id === id)?.favourites ?? (genreOf(id) !== null ? 'all' : 'none');
/** Resolves when every tap so far (on either toggle) is on the server (Make playlist reads the settings there). */
export const favouritesSaved = () => saving;
/** How each setting shows, on the panel's toggle and the Crates page's rows: a variation on the heart. */
export const FAVOURITES = {
  all: { icon: 'heart', label: 'All tracks', filled: false },
  only: { icon: 'heart', label: 'Only favourites', filled: true },
  none: { icon: 'heartOff', label: 'No favourites', filled: false },
} as const;
const NEXT: Record<CrateFavourites, CrateFavourites> = { none: 'only', only: 'all', all: 'none' };
/** A tap on the toggle: No favourites, Only favourites, All tracks, and round again. */
export function cycleFavourites(id: string) {
  const c = crates.list?.find((x) => x.id === id);
  if (!c) return;
  const was = c.favourites;
  const next = NEXT[was];
  c.favourites = pending[id] = next;
  saving = saving.then(() =>
    setCrateFavourites(id, next).then(
      () => pending[id] === next && delete pending[id],
      (e) => {
        if (pending[id] !== next) return;
        delete pending[id];
        const shown = crates.list?.find((x) => x.id === id);
        if (shown) shown.favourites = was;
        toast.fail(e, "The crate's setting wasn't saved");
      },
    ),
  );
}

// The favourite albums toggle (decision 0024, amended 2026-10-19), beside the favourites one and as it is: one setting
// per crate on the server, a tap shown at once. A crate not set yet shows all albums. Its sizes are counted with it as
// set, so the crates are read again once a tap is saved.
const pendingAlbums: Record<string, CrateFavourites> = {};
export const favouriteAlbumsOf = (id: string): CrateFavourites => crates.list?.find((c) => c.id === id)?.favourite_albums ?? 'all';
/** How each setting shows: a variation on the album's disc. */
export const FAVOURITE_ALBUMS = {
  all: { icon: 'disc', label: 'All albums', filled: false },
  only: { icon: 'disc', label: 'Only favourite albums', filled: true },
  none: { icon: 'albumsOff', label: 'No favourite albums', filled: false },
} as const;
/** A tap on the albums toggle: No favourite albums, Only favourite albums, All albums, and round again. */
export function cycleFavouriteAlbums(id: string) {
  const c = crates.list?.find((x) => x.id === id);
  if (!c) return;
  const was = c.favourite_albums ?? 'all';
  const next = NEXT[was];
  c.favourite_albums = pendingAlbums[id] = next;
  saving = saving.then(() =>
    setCrateFavouriteAlbums(id, next).then(
      () => {
        if (pendingAlbums[id] !== next) return;
        delete pendingAlbums[id];
        loadCrates();
      },
      (e) => {
        if (pendingAlbums[id] !== next) return;
        delete pendingAlbums[id];
        const shown = crates.list?.find((x) => x.id === id);
        if (shown) shown.favourite_albums = was;
        toast.fail(e, "The crate's setting wasn't saved");
      },
    ),
  );
}

// Before the setting was the server's (2026-10-14), each browser kept its own: sent once, then forgotten here.
const OLD_KEY = 'cratarr-crate-favourites';
async function moveBrowserSettings(list: Crate[]) {
  let read: unknown = null;
  try {
    read = JSON.parse(localStorage.getItem(OLD_KEY) ?? 'null');
    localStorage.removeItem(OLD_KEY);
  } catch {
    return;
  }
  if (!read || typeof read !== 'object') return;
  const old = read as Record<string, CrateFavourites>;
  const moved = list.filter((c) => old[c.id] && old[c.id] !== c.favourites);
  for (const c of moved) {
    try {
      await setCrateFavourites(c.id, old[c.id]);
      const shown = crates.list?.find((x) => x.id === c.id);
      if (shown && !(c.id in pending)) shown.favourites = old[c.id];
    } catch {
      /* the server's setting stays */
    }
  }
}

/** A crate's id in the panel's address (`?view=crate:unlisted`), and back. */
export const CRATE = 'crate:';
export const crateOf = (viewed: string | null) => (viewed?.startsWith(CRATE) ? viewed.slice(CRATE.length) : null);

/** The builder (decision 0024, the panel's third mode): `?view=crate-new`, or `?view=crate-edit:<id>` for one's own crate.
 *  `{ id: null }` is a new crate. */
export const NEW_CRATE = 'crate-new';
export const EDIT_CRATE = 'crate-edit:';
export const builderOf = (viewed: string | null): { id: string | null } | null =>
  viewed === NEW_CRATE ? { id: null } : viewed?.startsWith(EDIT_CRATE) ? { id: viewed.slice(EDIT_CRATE.length) } : null;

/** A crate being made or changed, kept in this browser under its own address until Save or Cancel (it survives the
 *  player over it, a page change and a reload). */
export type CrateDraft = { name: string; conditions: Condition[] };
const draftKey = (id: string | null) => `cratarr-crate-draft:${id ?? 'new'}`;
export function keptDraft(id: string | null): CrateDraft | null {
  try {
    const kept = JSON.parse(localStorage.getItem(draftKey(id)) ?? 'null');
    return kept && Array.isArray(kept.conditions) ? kept : null;
  } catch {
    return null;
  }
}
export function keepDraft(id: string | null, draft: CrateDraft) {
  try {
    localStorage.setItem(draftKey(id), JSON.stringify(draft));
  } catch {
    /* not kept */
  }
}
export function forgetDraft(id: string | null) {
  try {
    localStorage.removeItem(draftKey(id));
  } catch {
    /* nothing kept */
  }
}
export const crateName = (id: string) => crates.list?.find((c) => c.id === id)?.name ?? genreOf(id) ?? id;

// A crate's picture (decided 2026-10-16): its own, else its albums' covers (four as a mosaic, else the first), on its
// card and over it in the panel. Its own is read again after a change (a new address).
const imageRounds = $state<Record<string, number>>({});
/** `own`: false leaves its own picture out (edit mode, about to remove it). */
export function crateArt(id: string, own = true): string[] {
  const c = crates.list?.find((x) => x.id === id);
  if (!c) return [];
  if (own && c.image) return [crateImageUrl(id, imageRounds[id] ?? 0)];
  const covers = (c.stats?.covers ?? []).map((x) => albumCoverUrl(x.source, x.album_id));
  return covers.length >= 4 ? covers.slice(0, 4) : covers.slice(0, 1);
}
/** Sets a crate's own picture (`file`), or removes it (`null`). */
export async function setCrateArt(id: string, file: Blob | null) {
  if (file) await setCrateImage(id, file);
  else await removeCrateImage(id);
  imageRounds[id] = Date.now();
  const c = crates.list?.find((x) => x.id === id);
  if (c) c.image = !!file;
}

/** A genre in the panel: a crate of that one genre, made by the server when asked for (`crate:genre:<name>`), never
 *  saved; reached from a track's menu and from the genres shown on top of a list. */
export const GENRE = 'genre:';
export const genreOf = (crate: string | null) => (crate?.startsWith(GENRE) ? crate.slice(GENRE.length) : null);
export const viewGenre = (name: string) => viewPlaylist(CRATE + GENRE + name);

/** A new crate, named in the name sheet (as a new playlist is): the builder opens on a fresh draft of that name, saved
 *  only with its Save (a crate without conditions would hold the whole library). `conditions`: what it starts from (a
 *  genre's own, from the genre's view). */
export function startCrate(name: string, conditions: Condition[] = []) {
  try {
    localStorage.setItem(`cratarr-crate-draft:new`, JSON.stringify({ name, conditions }));
  } catch {
    /* the builder starts empty */
  }
  viewPlaylist(NEW_CRATE);
}

// A crate shuffled from its card (the owner, 2026-10-19: "a play button on the crates, which automatically plays it with
// shuffle"): its tracks as both its toggles let them through, from anywhere in it, in a random order, as a new queue.
// A crate comes 500 tracks at a time, so a big one is sampled: up to three stretches of 500 from anywhere in it.
// Shuffle (its card's picture, its panel, its menu): SHUFFLED tracks from the whole crate, however big (the owner,
// 2026-10-23: Unlisted's 10,000 and more played the same few each time, the panel's first page; a queue holds 500 at
// most). A crate that small
// whole; else STRETCHES short stretches at random places across all of it (a few requests, each a page the server
// serves at once), shuffled together.
const PAGE_MOST = 500;
const SHUFFLED = 500;
const STRETCHES = 25;
const STRETCH = 20;
const shuffled = <T>(list: T[]) => {
  const a = [...list];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
};
const asTracks = (source: string, rows: PlaylistRow[]): LocalTrack[] =>
  rows
    .filter((r) => r.id && !r.missing)
    .map((r) => ({ source, id: r.id!, name: r.name ?? r.file, artist: r.artist, album: r.album, album_id: r.album_id ?? null, duration_secs: r.duration_secs }));
/** Rows `from`..`from + n` of the crate as its favourites settings have it, in pages the server serves. */
async function crateSpan(id: string, from: number, n: number) {
  const pages = await Promise.all(
    Array.from({ length: Math.ceil(n / PAGE_MOST) }, (_, i) => crateRows(id, from + i * PAGE_MOST, Math.min(PAGE_MOST, n - i * PAGE_MOST), prefs.source || undefined, favouritesOf(id), favouriteAlbumsOf(id))),
  );
  return { source: pages[0]?.source ?? '', total: pages[0]?.total ?? 0, rows: pages.flatMap((p) => p.rows) };
}
export async function shuffleCrate(id: string, name: string) {
  const head = await crateSpan(id, 0, 1);
  const total = head.total;
  let rows: PlaylistRow[];
  if (total <= SHUFFLED * 1.5) rows = (await crateSpan(id, 0, total)).rows;
  else {
    const slots = Math.floor(total / STRETCH);
    const picks = shuffled([...Array(slots).keys()]).slice(0, STRETCHES);
    rows = (await Promise.all(picks.map((k) => crateSpan(id, k * STRETCH, STRETCH)))).flatMap((p) => p.rows);
  }
  const tracks = shuffled(asTracks(head.source, rows)).slice(0, SHUFFLED);
  if (!tracks.length) throw new Error('Nothing in this crate to play');
  await playPlaylistFrom(name, head.source, tracks, 0);
}

// A crate's menu's actions (decision 0058: lib/CrateMenu), from wherever it is opened: its tracks as its favourites
// settings have it (up to MOST, as its panel's Make playlist and radio take), played, put next, queued or radio's seed.
export async function playCrate(id: string, name: string, mode: 'play' | 'next' | 'last') {
  const got = await crateSpan(id, 0, mode === 'play' ? SHUFFLED : PAGE_MOST);
  const { source } = got;
  const tracks = asTracks(source, got.rows);
  if (!tracks.length) throw new Error('Nothing in this crate to play');
  if (mode === 'play') return playPlaylistFrom(name, source, tracks, 0);
  if (isLocal()) return enqueue(tracks, mode);
  await queueTracks(
    tracks.map((t) => t.id),
    mode,
    source,
  );
}
/** Start radio: tracks like the crate's from the whole library (up to 300 of its tracks as the seed, decision 0036). */
export async function crateRadio(id: string, name: string) {
  const ids = (await crateRows(id, 0, PAGE_MOST, 'cratarr')).rows.flatMap((r) => (r.id ? [r.id] : []));
  await startRadio(ids.length > 300 ? shuffled(ids).slice(0, 300) : ids, name);
}
/** One's own crate deleted, Undo bringing it back. */
export async function deleteOwnCrate(id: string) {
  const name = crateName(id);
  const { undo } = await deleteCrate(id);
  withUndo(`Deleted ${name}.`, [undo], { after: () => void loadCrates() });
  await loadCrates();
}

/** An Exclude in the heart's sheet (lib/HeartSheet). */
export type ExcludeEntry = { label: string; icon: 'crateOut'; run: () => void };

/** The playing track's crate Excludes, for the players' hearts (the full player's and the mini player's; not the Queue
 *  sheet's): from the crate the queue came from, the track or its album, for the crate's own tracks. */
export function playingCrateOuts(): ExcludeEntry[] {
  const c = queueCrate();
  const t = local.queue[local.index];
  if (!c || !t || t.lane) return [];
  const name = crateName(c);
  const fail = (e: unknown) => toast.fail(e, "It wasn't excluded");
  const album = t.album_id;
  return [
    { label: `Exclude from ${name}`, icon: 'crateOut', run: () => void leaveOut(c, t.source, { ids: [t.id] }).catch(fail) },
    ...(album ? [{ label: `Exclude album from ${name}`, icon: 'crateOut' as const, run: () => void leaveOut(c, t.source, { albums: [album] }).catch(fail) }] : []),
  ];
}

/** The crate the queue playing here came from, by the queue's name (a playlist of the same name wins): the Queue
 *  sheet's chip, and Remove from it in the full player's menu. */
export function queueCrate(from: string | null = isLocal() ? local.from : null): string | null {
  if (!from || shownPlaylists().some((p) => p.id === from)) return null;
  return crates.list?.find((c) => c.name === from)?.id ?? null;
}

// A crate's exclusions (decision 0024, amended 2026-10-19; the owner's word: "exclude, not remove"): tracks, or a
// track's whole album, from its panel (a swipe left, a track's menu) or the full player (its menu, a long press on its
// heart) while a queue it started plays; not from the Queue sheet (the owner: "It makes no sense to have the button
// there"). It leaves them out whatever its conditions and toggles say; Undo puts them back (the owner: "Undo toast",
// the one notice of its kind). Its sizes are read again.
export async function leaveOut(id: string, source: string, what: Excluding) {
  const name = crateName(id);
  const { undo } = await leaveOutOfCrate(id, source, what);
  // Out of the crate, out of the queue it started too (the owner: "it leaves the queue too"), its own tracks only (not
  // those added by hand, by Radio or by Autoplay); Undo puts them back in the crate only.
  if (queueCrate() === id)
    dropFromQueue((t) => !t.lane && t.source === source && (!!what.ids?.includes(t.id) || (!!t.album_id && !!what.albums?.includes(t.album_id))));
  withUndo(`Excluded from ${name}.`, [undo], { after: () => loadCrates() });
  await loadCrates();
}
